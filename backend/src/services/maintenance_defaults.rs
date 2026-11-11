// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

use sqlx::FromRow;

use crate::errors::AppError;

/// Outcome of `insert_defaults_for_aircraft` so callers (the seed-defaults
/// endpoint, batch import, manual create, log-import auto-register) can
/// report accurate counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertDefaultsOutcome {
    /// Rows were created. The count is the number of schedules inserted.
    Inserted(usize),
    /// Skipped because the aircraft already had at least one schedule.
    /// Prevents duplicates without relying on per-row uniqueness alone.
    SkippedHadSchedules,
    /// Skipped because the aircraft has no model_id, so there is no
    /// template family to apply.
    SkippedNoModel,
    /// Skipped because the model exists but has no active templates in
    /// `maintenance_schedule_templates`.
    SkippedNoTemplates,
    /// Skipped because the aircraft is flagged `needs_manual_sn = TRUE`.
    /// The SN hasn't been confirmed by a human, so the row may turn out
    /// to be a duplicate that gets merged away. Seeding templates now
    /// would create schedule rows we'd have to clean up after the merge
    /// (see the 2026-05-16 dedupe incident). Wait until the operator
    /// confirms or merges via the pending-review UI.
    SkippedNeedsManualSn,
}

#[derive(Debug, FromRow)]
struct TemplateRow {
    name: String,
    description: Option<String>,
    interval_type: String,
    interval_value: f32,
    warning_threshold: f32,
    is_critical: bool,
    secondary_interval_type: Option<String>,
    secondary_interval_value: Option<f32>,
}

/// Spawn per-aircraft maintenance schedules from this org+model's
/// templates in `maintenance_schedule_templates`.
///
/// **Idempotent**: if the aircraft already has any schedule (active or
/// inactive), returns `SkippedHadSchedules` and inserts nothing — so all
/// callers (manual create, log-import auto-register, fleet batch-import,
/// the admin seed-defaults endpoint) are safe from duplicates and never
/// clobber manually-added rows.
///
/// Takes `&mut Transaction` so callers can scope the inserts inside an
/// outer transaction. The aircraft-create paths roll back both the
/// aircraft INSERT and the seeded schedules atomically if anything fails.
pub async fn insert_defaults_for_aircraft(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    org_id: i32,
    aircraft_id: i32,
    model_id: Option<i32>,
    created_by: &str,
) -> Result<InsertDefaultsOutcome, AppError> {
    // Idempotency gate.
    let existing: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM maintenance_schedules WHERE aircraft_id = $1",
    )
    .bind(aircraft_id)
    .fetch_one(&mut **tx)
    .await?;
    if existing.0 > 0 {
        return Ok(InsertDefaultsOutcome::SkippedHadSchedules);
    }

    // Pending-review gate. Aircraft auto-registered from DJI logs with an
    // unmatched SN are flagged needs_manual_sn=TRUE so a human can confirm
    // the row isn't actually an FC SN belonging to an existing aircraft.
    // Seeding templates before that confirmation creates schedules we'd
    // have to clean up if the row turns out to be a duplicate.
    let nms: (bool,) = sqlx::query_as(
        "SELECT needs_manual_sn FROM aircraft WHERE id = $1 AND org_id = $2",
    )
    .bind(aircraft_id)
    .bind(org_id)
    .fetch_one(&mut **tx)
    .await?;
    if nms.0 {
        return Ok(InsertDefaultsOutcome::SkippedNeedsManualSn);
    }

    let Some(model_id) = model_id else {
        return Ok(InsertDefaultsOutcome::SkippedNoModel);
    };

    let templates: Vec<TemplateRow> = sqlx::query_as(
        "SELECT name, description, interval_type, interval_value, \
                warning_threshold, is_critical, \
                secondary_interval_type, secondary_interval_value \
         FROM maintenance_schedule_templates \
         WHERE org_id = $1 AND model_id = $2 AND is_active = TRUE",
    )
    .bind(org_id)
    .bind(model_id)
    .fetch_all(&mut **tx)
    .await?;

    if templates.is_empty() {
        return Ok(InsertDefaultsOutcome::SkippedNoTemplates);
    }

    let mut inserted = 0usize;
    for t in &templates {
        sqlx::query(
            "INSERT INTO maintenance_schedules \
             (org_id, aircraft_id, name, description, interval_type, interval_value, \
              warning_threshold, is_critical, \
              secondary_interval_type, secondary_interval_value, \
              is_active, created_by) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,TRUE,$11) \
             ON CONFLICT (org_id, aircraft_id, name) DO NOTHING",
        )
        .bind(org_id)
        .bind(aircraft_id)
        .bind(&t.name)
        .bind(&t.description)
        .bind(&t.interval_type)
        .bind(t.interval_value)
        .bind(t.warning_threshold)
        .bind(t.is_critical)
        .bind(&t.secondary_interval_type)
        .bind(t.secondary_interval_value)
        .bind(created_by)
        .execute(&mut **tx)
        .await?;
        inserted += 1;
    }
    Ok(InsertDefaultsOutcome::Inserted(inserted))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::PgPool;

    async fn default_org_id(pool: &PgPool) -> i32 {
        let row: (i32,) = sqlx::query_as("SELECT id FROM organisations ORDER BY id LIMIT 1")
            .fetch_one(pool)
            .await
            .expect("default org seeded");
        row.0
    }

    async fn seed_model(pool: &PgPool, org_id: i32, name: &str) -> i32 {
        let row: (i32,) = sqlx::query_as(
            "INSERT INTO aircraft_models (org_id, name) VALUES ($1, $2) RETURNING id",
        )
        .bind(org_id)
        .bind(name)
        .fetch_one(pool)
        .await
        .expect("model insert");
        row.0
    }

    async fn seed_template(pool: &PgPool, org_id: i32, model_id: i32, name: &str) {
        sqlx::query(
            "INSERT INTO maintenance_schedule_templates \
             (org_id, model_id, name, interval_type, interval_value, \
              warning_threshold, is_critical, is_active) \
             VALUES ($1, $2, $3, 'flight_hours', 50.0, 0.8, FALSE, TRUE)",
        )
        .bind(org_id)
        .bind(model_id)
        .bind(name)
        .execute(pool)
        .await
        .expect("template insert");
    }

    async fn seed_aircraft_with_nms(
        pool: &PgPool,
        org_id: i32,
        model_id: i32,
        serial: &str,
        needs_manual_sn: bool,
    ) -> i32 {
        let row: (i32,) = sqlx::query_as(
            "INSERT INTO aircraft (org_id, serial_number, model_id, status, needs_manual_sn) \
             VALUES ($1, $2, $3, 'active', $4) RETURNING id",
        )
        .bind(org_id)
        .bind(serial)
        .bind(model_id)
        .bind(needs_manual_sn)
        .fetch_one(pool)
        .await
        .expect("aircraft insert");
        row.0
    }

    /// Aircraft flagged needs_manual_sn=TRUE must short-circuit with
    /// SkippedNeedsManualSn and write zero schedules — even when the
    /// model has active templates available. Regression guard for the
    /// 2026-05-16 dedupe incident where duplicate aircraft auto-registered
    /// from DJI log imports each received a fresh copy of the template
    /// schedules.
    #[sqlx::test(migrations = "./migrations")]
    async fn skips_when_needs_manual_sn_true(pool: PgPool) {
        let org_id = default_org_id(&pool).await;
        let model_id = seed_model(&pool, org_id, "Inspire 1").await;
        seed_template(&pool, org_id, model_id, "Annual inspection").await;
        let aircraft_id =
            seed_aircraft_with_nms(&pool, org_id, model_id, "0410000001", true).await;

        let mut tx = pool.begin().await.unwrap();
        let outcome = insert_defaults_for_aircraft(
            &mut tx, org_id, aircraft_id, Some(model_id), "TST",
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();

        assert_eq!(outcome, InsertDefaultsOutcome::SkippedNeedsManualSn);

        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM maintenance_schedules WHERE aircraft_id = $1",
        )
        .bind(aircraft_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count.0, 0, "no schedules should be seeded for needs_manual_sn aircraft");
    }

    /// Control: identical setup but needs_manual_sn=FALSE seeds templates
    /// normally. Proves the new gate doesn't regress the happy path.
    #[sqlx::test(migrations = "./migrations")]
    async fn seeds_normally_when_needs_manual_sn_false(pool: PgPool) {
        let org_id = default_org_id(&pool).await;
        let model_id = seed_model(&pool, org_id, "Mavic 3").await;
        seed_template(&pool, org_id, model_id, "Annual inspection").await;
        let aircraft_id =
            seed_aircraft_with_nms(&pool, org_id, model_id, "1581F5F", false).await;

        let mut tx = pool.begin().await.unwrap();
        let outcome = insert_defaults_for_aircraft(
            &mut tx, org_id, aircraft_id, Some(model_id), "TST",
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();

        assert_eq!(outcome, InsertDefaultsOutcome::Inserted(1));

        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM maintenance_schedules WHERE aircraft_id = $1",
        )
        .bind(aircraft_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count.0, 1);
    }
}
