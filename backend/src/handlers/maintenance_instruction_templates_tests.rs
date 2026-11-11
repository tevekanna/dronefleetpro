// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

//! Integration tests for editing maintenance instruction templates
//! (in-place item edit + new revision). Each test runs against a fresh DB via
//! #[sqlx::test(migrations = "./migrations")].

use super::*;
use crate::auth::middleware::Claims;
use crate::errors::AppError;
use axum::extract::{Path, State};
use axum::Json;
use sqlx::{PgPool, Row};

// ---- Claims + seed helpers (mirror checklists_tests.rs conventions) ----

async fn default_org_id(pool: &PgPool) -> i32 {
    let row = sqlx::query("SELECT id FROM organisations ORDER BY id LIMIT 1")
        .fetch_one(pool)
        .await
        .expect("default org seeded by migration 002");
    row.get::<i32, _>("id")
}

async fn default_admin_user(pool: &PgPool) -> (i32, String) {
    let row = sqlx::query("SELECT id, user_code FROM users WHERE role = 'admin' ORDER BY id LIMIT 1")
        .fetch_one(pool)
        .await
        .expect("default admin seeded by migration 002");
    (row.get::<i32, _>("id"), row.get::<String, _>("user_code"))
}

fn admin_claims(org_id: i32, user_id: i32, user_code: &str) -> Claims {
    Claims {
        user_id,
        org_id,
        role: "admin".to_string(),
        roles: vec!["admin".to_string()],
        user_code: user_code.to_string(),
    }
}

fn pilot_claims(org_id: i32, user_id: i32, user_code: &str) -> Claims {
    Claims {
        user_id,
        org_id,
        role: "pilot".to_string(),
        roles: vec!["pilot".to_string()],
        user_code: user_code.to_string(),
    }
}

async fn seed_model(pool: &PgPool, org_id: i32, name: &str) -> i32 {
    let row = sqlx::query("INSERT INTO aircraft_models (org_id, name) VALUES ($1, $2) RETURNING id")
        .bind(org_id)
        .bind(name)
        .fetch_one(pool)
        .await
        .expect("aircraft_models insert");
    row.get::<i32, _>("id")
}

async fn seed_aircraft(pool: &PgPool, org_id: i32, model_id: i32) -> i32 {
    let serial = format!("SN-{}", uuid::Uuid::new_v4());
    let row = sqlx::query(
        "INSERT INTO aircraft (org_id, serial_number, model_id) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(org_id)
    .bind(serial)
    .bind(model_id)
    .fetch_one(pool)
    .await
    .expect("aircraft insert");
    row.get::<i32, _>("id")
}

async fn seed_maintenance(pool: &PgPool, org_id: i32, aircraft_id: i32) -> i32 {
    let row = sqlx::query(
        "INSERT INTO maintenance \
            (org_id, aircraft_id, performed_date, created_by, remark, action_taken, \
             maint_category, maint_type, sign_off) \
         VALUES ($1, $2, CURRENT_DATE, 'ADM', 'inspection', 'checked', 'Scheduled', 'Test', 'ADM') \
         RETURNING id",
    )
    .bind(org_id)
    .bind(aircraft_id)
    .fetch_one(pool)
    .await
    .expect("maintenance insert");
    row.get::<i32, _>("id")
}

async fn seed_completion(pool: &PgPool, maintenance_id: i32, item_id: i32) {
    sqlx::query(
        "INSERT INTO maintenance_checklist_completions \
            (maintenance_id, checklist_item_id, status, completed_by) \
         VALUES ($1, $2, 'ok', 'ADM')",
    )
    .bind(maintenance_id)
    .bind(item_id)
    .execute(pool)
    .await
    .expect("completion insert");
}

async fn seed_schedule(pool: &PgPool, org_id: i32, aircraft_id: i32, template_id: i32) -> i32 {
    let row = sqlx::query(
        "INSERT INTO maintenance_schedules \
            (org_id, aircraft_id, name, interval_type, interval_value, instruction_template_id) \
         VALUES ($1, $2, 'Sched', 'calendar_days', 30, $3) RETURNING id",
    )
    .bind(org_id)
    .bind(aircraft_id)
    .bind(template_id)
    .fetch_one(pool)
    .await
    .expect("schedule insert");
    row.get::<i32, _>("id")
}

async fn seed_secondary_org_with_admin(pool: &PgPool) -> (i32, i32, String) {
    let org_row = sqlx::query("INSERT INTO organisations (name, slug) VALUES ($1, $2) RETURNING id")
        .bind("Checklist Edit Test Org 2")
        .bind(format!("chk-edit-org-{}", uuid::Uuid::new_v4()))
        .fetch_one(pool)
        .await
        .expect("secondary org");
    let org_id: i32 = org_row.get("id");

    let user_row = sqlx::query(
        "INSERT INTO users (org_id, name, email, user_code, password_hash, role) \
         VALUES ($1, $2, $3, $4, $5, 'admin') RETURNING id, user_code",
    )
    .bind(org_id)
    .bind("Admin Two")
    .bind(format!("admin2-{}@example.com", org_id))
    .bind(format!("A{:02}", org_id % 100))
    .bind("$2b$12$placeholder.hash.value.not.used.in.these.tests.xxxxxxxxxx")
    .fetch_one(pool)
    .await
    .expect("secondary admin");
    (
        org_id,
        user_row.get::<i32, _>("id"),
        user_row.get::<String, _>("user_code"),
    )
}

/// Build a `sections` payload with one section and the given item texts.
fn sections(section_name: &str, texts: &[&str]) -> Vec<CreateSectionInput> {
    vec![CreateSectionInput {
        section_name: section_name.into(),
        section_order: 1,
        items: texts
            .iter()
            .enumerate()
            .map(|(i, t)| CreateItemInput {
                item_text: (*t).into(),
                is_required: true,
                reference_manual: None,
                item_order: (i as i32) + 1,
            })
            .collect(),
    }]
}

/// Create a template via the create handler and return its id.
async fn create_template_returning_id(
    pool: &PgPool,
    claims: Claims,
    model_id: Option<i32>,
    texts: &[&str],
) -> i32 {
    let Json(created) = create_template(
        claims,
        State(pool.clone()),
        Json(CreateTemplateRequest {
            name: "Pre-flight inspection".into(),
            aircraft_model_id: model_id,
            description: Some("initial".into()),
            revision: None,
            sections: sections("Airframe", texts),
        }),
    )
    .await
    .expect("create template");
    created["id"].as_i64().expect("id in response") as i32
}

async fn first_item_id(pool: &PgPool, template_id: i32) -> i32 {
    let row = sqlx::query(
        "SELECT id FROM maintenance_checklist_items WHERE template_id = $1 \
         ORDER BY section_order, item_order LIMIT 1",
    )
    .bind(template_id)
    .fetch_one(pool)
    .await
    .expect("first item");
    row.get::<i32, _>("id")
}

// ===========================================================================
// 1. In-place edit succeeds when there are no completions
// ===========================================================================

#[sqlx::test(migrations = "./migrations")]
async fn test_update_items_in_place_succeeds(pool: PgPool) {
    let org_id = default_org_id(&pool).await;
    let (uid, code) = default_admin_user(&pool).await;
    let template_id =
        create_template_returning_id(&pool, admin_claims(org_id, uid, &code), None, &["A", "B"])
            .await;

    let Json(detail) = update_items(
        admin_claims(org_id, uid, &code),
        State(pool.clone()),
        Path(template_id),
        Json(EditTemplateRequest {
            name: Some("Pre-flight inspection v2".into()),
            description: None,
            sections: sections("Airframe", &["A2", "B2", "C2"]),
        }),
    )
    .await
    .expect("in-place edit ok");

    assert_eq!(detail.name, "Pre-flight inspection v2");
    assert_eq!(detail.sections.len(), 1);
    assert_eq!(detail.sections[0].items.len(), 3);
    assert_eq!(detail.sections[0].items[0].item_text, "A2");

    let count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM maintenance_checklist_items WHERE template_id = $1")
            .bind(template_id)
            .fetch_one(&pool)
            .await
            .expect("count");
    assert_eq!(count.0, 3);

    let audit: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM audit_log WHERE table_name = 'maintenance_instruction_templates' \
         AND action = 'UPDATE_CHECKLIST_ITEMS' AND record_id = $1",
    )
    .bind(template_id)
    .fetch_one(&pool)
    .await
    .expect("audit count");
    assert_eq!(audit.0, 1);
}

// ===========================================================================
// 2. In-place edit is refused (409 requires_new_revision) when a completion exists
// ===========================================================================

#[sqlx::test(migrations = "./migrations")]
async fn test_update_items_conflict_when_completion_exists(pool: PgPool) {
    let org_id = default_org_id(&pool).await;
    let (uid, code) = default_admin_user(&pool).await;
    let model_id = seed_model(&pool, org_id, "M600").await;
    let aircraft_id = seed_aircraft(&pool, org_id, model_id).await;
    let template_id = create_template_returning_id(
        &pool,
        admin_claims(org_id, uid, &code),
        Some(model_id),
        &["A", "B"],
    )
    .await;

    let item_id = first_item_id(&pool, template_id).await;
    let maint_id = seed_maintenance(&pool, org_id, aircraft_id).await;
    seed_completion(&pool, maint_id, item_id).await;

    let err = update_items(
        admin_claims(org_id, uid, &code),
        State(pool.clone()),
        Path(template_id),
        Json(EditTemplateRequest {
            name: None,
            description: None,
            sections: sections("Airframe", &["A2"]),
        }),
    )
    .await
    .expect_err("must refuse in-place edit");

    match err {
        AppError::ConflictCode { code, .. } => assert_eq!(code, "requires_new_revision"),
        other => panic!("expected ConflictCode, got {other:?}"),
    }

    // Items are untouched by the refused edit.
    let count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM maintenance_checklist_items WHERE template_id = $1")
            .bind(template_id)
            .fetch_one(&pool)
            .await
            .expect("count");
    assert_eq!(count.0, 2);
}

// ===========================================================================
// 3. Revision flow: new row, items copied with edits, schedules re-pointed,
//    old row inactive, old items + completions untouched
// ===========================================================================

#[sqlx::test(migrations = "./migrations")]
async fn test_create_revision_repoints_and_supersedes(pool: PgPool) {
    let org_id = default_org_id(&pool).await;
    let (uid, code) = default_admin_user(&pool).await;
    let model_id = seed_model(&pool, org_id, "Inspire").await;
    let aircraft_id = seed_aircraft(&pool, org_id, model_id).await;
    let old_id = create_template_returning_id(
        &pool,
        admin_claims(org_id, uid, &code),
        Some(model_id),
        &["A", "B"],
    )
    .await;

    // A completion on the old template, and a schedule linked to it.
    let old_item = first_item_id(&pool, old_id).await;
    let maint_id = seed_maintenance(&pool, org_id, aircraft_id).await;
    seed_completion(&pool, maint_id, old_item).await;
    let schedule_id = seed_schedule(&pool, org_id, aircraft_id, old_id).await;

    let Json(resp) = create_revision(
        admin_claims(org_id, uid, &code),
        State(pool.clone()),
        Path(old_id),
        Json(EditTemplateRequest {
            name: None,
            description: None,
            sections: sections("Airframe", &["A-edited", "B", "C-new"]),
        }),
    )
    .await
    .expect("create revision ok");

    // New template: bumped revision, distinct id, edited items, preserved source.
    assert_eq!(resp.repointed_schedules, 1);
    assert_ne!(resp.template.id, old_id);
    assert_eq!(resp.template.revision, "02");
    assert_eq!(resp.template.source, "custom");
    assert_eq!(resp.template.sections[0].items.len(), 3);
    assert_eq!(resp.template.sections[0].items[0].item_text, "A-edited");
    let new_id = resp.template.id;

    // Schedule now points at the new revision.
    let sched_ptr: (Option<i32>,) = sqlx::query_as(
        "SELECT instruction_template_id FROM maintenance_schedules WHERE id = $1",
    )
    .bind(schedule_id)
    .fetch_one(&pool)
    .await
    .expect("schedule ptr");
    assert_eq!(sched_ptr.0, Some(new_id));

    // Old template deactivated.
    let old_active: (bool,) = sqlx::query_as(
        "SELECT is_active FROM maintenance_instruction_templates WHERE id = $1",
    )
    .bind(old_id)
    .fetch_one(&pool)
    .await
    .expect("old active");
    assert!(!old_active.0, "old template must be deactivated");

    // Old items are untouched (still 2), and the completion still references one.
    let old_items: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM maintenance_checklist_items WHERE template_id = $1")
            .bind(old_id)
            .fetch_one(&pool)
            .await
            .expect("old items");
    assert_eq!(old_items.0, 2);

    let completions: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM maintenance_checklist_completions WHERE checklist_item_id = $1",
    )
    .bind(old_item)
    .fetch_one(&pool)
    .await
    .expect("completions");
    assert_eq!(completions.0, 1);
}

// ===========================================================================
// 4. Org isolation: a template in another org is 404 for both endpoints
// ===========================================================================

#[sqlx::test(migrations = "./migrations")]
async fn test_edit_endpoints_org_isolation_404(pool: PgPool) {
    let org1 = default_org_id(&pool).await;
    let (uid1, code1) = default_admin_user(&pool).await;
    let template_id =
        create_template_returning_id(&pool, admin_claims(org1, uid1, &code1), None, &["A"]).await;

    let (org2, uid2, code2) = seed_secondary_org_with_admin(&pool).await;

    let err = update_items(
        admin_claims(org2, uid2, &code2),
        State(pool.clone()),
        Path(template_id),
        Json(EditTemplateRequest {
            name: None,
            description: None,
            sections: sections("X", &["Z"]),
        }),
    )
    .await
    .expect_err("cross-org update must 404");
    assert!(matches!(err, AppError::NotFound(_)), "got {err:?}");

    let err = create_revision(
        admin_claims(org2, uid2, &code2),
        State(pool.clone()),
        Path(template_id),
        Json(EditTemplateRequest {
            name: None,
            description: None,
            sections: sections("X", &["Z"]),
        }),
    )
    .await
    .expect_err("cross-org revision must 404");
    assert!(matches!(err, AppError::NotFound(_)), "got {err:?}");
}

// ===========================================================================
// 5. Non-admin is rejected on both endpoints
// ===========================================================================

#[sqlx::test(migrations = "./migrations")]
async fn test_edit_endpoints_admin_only(pool: PgPool) {
    let org_id = default_org_id(&pool).await;
    let (uid, code) = default_admin_user(&pool).await;
    let template_id =
        create_template_returning_id(&pool, admin_claims(org_id, uid, &code), None, &["A"]).await;

    let err = update_items(
        pilot_claims(org_id, uid, &code),
        State(pool.clone()),
        Path(template_id),
        Json(EditTemplateRequest {
            name: None,
            description: None,
            sections: sections("X", &["Z"]),
        }),
    )
    .await
    .expect_err("pilot update must 403");
    assert!(matches!(err, AppError::Forbidden(_)), "got {err:?}");

    let err = create_revision(
        pilot_claims(org_id, uid, &code),
        State(pool.clone()),
        Path(template_id),
        Json(EditTemplateRequest {
            name: None,
            description: None,
            sections: sections("X", &["Z"]),
        }),
    )
    .await
    .expect_err("pilot revision must 403");
    assert!(matches!(err, AppError::Forbidden(_)), "got {err:?}");
}

// ===========================================================================
// 6. next_revision rule
// ===========================================================================

#[test]
fn test_next_revision_rule() {
    assert_eq!(next_revision("01"), "02");
    assert_eq!(next_revision("09"), "10");
    assert_eq!(next_revision("99"), "100");
    assert_eq!(next_revision("A"), "A-2");
    assert_eq!(next_revision("Rev1"), "Rev1-2");
    assert_eq!(next_revision("A-2"), "A-3");
}
