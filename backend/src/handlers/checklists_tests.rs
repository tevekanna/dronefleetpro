// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

//! Integration tests for the checklists handler. Each test runs against a
//! fresh DB via #[sqlx::test(migrations = "./migrations")] so migration 034
//! is in the loop on every run.

use super::*;
use crate::auth::middleware::Claims;
use crate::models::checklist::{
    ChecklistInputBody, ChecklistItemInput, ChecklistSectionInput,
};
use axum::extract::{Path, State};
use sqlx::{PgPool, Row};

// ---- Seed helpers (mirror model_documents_tests.rs conventions) ----

async fn default_org_id(pool: &PgPool) -> i32 {
    let row = sqlx::query("SELECT id FROM organisations ORDER BY id LIMIT 1")
        .fetch_one(pool)
        .await
        .expect("default org seeded by migration 002");
    row.get::<i32, _>("id")
}

async fn default_admin_user(pool: &PgPool) -> (i32, String) {
    let row = sqlx::query(
        "SELECT id, user_code FROM users WHERE role = 'admin' ORDER BY id LIMIT 1",
    )
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
    let row = sqlx::query(
        "INSERT INTO aircraft_models (org_id, name) VALUES ($1, $2) RETURNING id",
    )
    .bind(org_id)
    .bind(name)
    .fetch_one(pool)
    .await
    .expect("aircraft_models insert");
    row.get::<i32, _>("id")
}

async fn seed_secondary_org_with_admin(pool: &PgPool) -> (i32, i32, String) {
    let org_row = sqlx::query(
        "INSERT INTO organisations (name, slug) VALUES ($1, $2) RETURNING id",
    )
    .bind("Checklists Test Org 2")
    .bind(format!("checklists-org-{}", uuid::Uuid::new_v4()))
    .fetch_one(pool)
    .await
    .expect("secondary org");
    let org_id: i32 = org_row.get("id");

    // The admin user belongs to the secondary org. user_code is per-org
    // unique; the suffix avoids clashes if multiple secondary orgs were ever
    // created in the same test.
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

/// Build a small but realistic payload with two sections and three items
/// per section. Used as the happy-path baseline; individual tests tweak it
/// where needed.
fn sample_body(r#type: &str, title: &str) -> ChecklistInputBody {
    ChecklistInputBody {
        r#type: r#type.into(),
        title: title.into(),
        revision: Some("01".into()),
        revision_date: None,
        sections: vec![
            ChecklistSectionInput {
                title: "Pre-flight".into(),
                sort_order: 1,
                start_on_new_page: false,
                items: vec![
                    ChecklistItemInput {
                        label: "Propellers".into(),
                        value: "Inspect and confirm secure".into(),
                        responsibility: "PF".into(),
                        sort_order: 1,
                    },
                    ChecklistItemInput {
                        label: "Battery level".into(),
                        value: ">= 90%".into(),
                        responsibility: "PM".into(),
                        sort_order: 2,
                    },
                    ChecklistItemInput {
                        label: "GPS lock".into(),
                        value: "Confirmed (12+ sats)".into(),
                        responsibility: "PF/PM".into(),
                        sort_order: 3,
                    },
                ],
            },
            ChecklistSectionInput {
                title: "Take-off".into(),
                sort_order: 2,
                start_on_new_page: true,
                items: vec![
                    ChecklistItemInput {
                        label: "Clearance".into(),
                        value: "Verbal confirmation".into(),
                        responsibility: "PF".into(),
                        sort_order: 1,
                    },
                    ChecklistItemInput {
                        label: "Bystanders".into(),
                        value: "30 m minimum".into(),
                        responsibility: "PM".into(),
                        sort_order: 2,
                    },
                    ChecklistItemInput {
                        label: "Camera".into(),
                        value: "Optional".into(),
                        responsibility: "N/A".into(),
                        sort_order: 3,
                    },
                ],
            },
        ],
    }
}

// ===========================================================================
// 1. Happy path: create with nested sections + items
// ===========================================================================

#[sqlx::test(migrations = "./migrations")]
async fn test_create_checklist_with_nested_sections_and_items(pool: PgPool) {
    let org_id = default_org_id(&pool).await;
    let (user_id, user_code) = default_admin_user(&pool).await;
    let model_id = seed_model(&pool, org_id, "TestCreateNested").await;

    let body = sample_body("normal_ops", "Normal Operations");
    let Json(full) = create(
        admin_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path(model_id),
        Json(body),
    )
    .await
    .expect("create ok");

    assert_eq!(full.checklist.r#type, "normal_ops");
    assert_eq!(full.checklist.title, "Normal Operations");
    assert_eq!(full.checklist.created_by, user_id);
    assert_eq!(full.checklist.updated_by, Some(user_id));
    assert_eq!(full.sections.len(), 2);
    assert_eq!(full.sections[0].items.len(), 3);
    assert_eq!(full.sections[1].items.len(), 3);
    assert!(full.sections[1].section.start_on_new_page);

    let item_count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM checklist_items i \
         JOIN checklist_sections s ON s.id = i.section_id \
         WHERE s.checklist_id = $1",
    )
    .bind(full.checklist.id)
    .fetch_one(&pool)
    .await
    .expect("item count");
    assert_eq!(item_count.0, 6);

    let audit_count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM audit_log \
         WHERE table_name = 'checklists' AND record_id = $1 \
           AND action = 'CREATE_CHECKLIST'",
    )
    .bind(full.checklist.id)
    .fetch_one(&pool)
    .await
    .expect("audit count");
    assert_eq!(audit_count.0, 1);
}

// ===========================================================================
// 2. Duplicate (model_id, type) -> 409 Conflict
// ===========================================================================

#[sqlx::test(migrations = "./migrations")]
async fn test_create_rejects_duplicate_model_type(pool: PgPool) {
    let org_id = default_org_id(&pool).await;
    let (user_id, user_code) = default_admin_user(&pool).await;
    let model_id = seed_model(&pool, org_id, "TestDuplicateType").await;

    let _ = create(
        admin_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path(model_id),
        Json(sample_body("normal_ops", "First")),
    )
    .await
    .expect("first create");

    let err = create(
        admin_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path(model_id),
        Json(sample_body("normal_ops", "Second attempt")),
    )
    .await
    .expect_err("duplicate must reject");

    match err {
        AppError::Conflict(msg) => assert!(msg.contains("normal_ops"), "got: {msg}"),
        other => panic!("expected Conflict, got {other:?}"),
    }
}

// ===========================================================================
// 3. normal_ops + qrh on the same model both succeed
// ===========================================================================

#[sqlx::test(migrations = "./migrations")]
async fn test_create_allows_normal_ops_and_qrh_for_same_model(pool: PgPool) {
    let org_id = default_org_id(&pool).await;
    let (user_id, user_code) = default_admin_user(&pool).await;
    let model_id = seed_model(&pool, org_id, "TestBothTypes").await;

    let _ = create(
        admin_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path(model_id),
        Json(sample_body("normal_ops", "Normal")),
    )
    .await
    .expect("normal_ops create");

    let _ = create(
        admin_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path(model_id),
        Json(sample_body("qrh", "QRH")),
    )
    .await
    .expect("qrh create");

    let cnt: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM checklists WHERE model_id = $1",
    )
    .bind(model_id)
    .fetch_one(&pool)
    .await
    .expect("count");
    assert_eq!(cnt.0, 2);
}

// ===========================================================================
// 4. type='emergency' rejected with 400
// ===========================================================================

#[sqlx::test(migrations = "./migrations")]
async fn test_create_rejects_invalid_type(pool: PgPool) {
    let org_id = default_org_id(&pool).await;
    let (user_id, user_code) = default_admin_user(&pool).await;
    let model_id = seed_model(&pool, org_id, "TestInvalidType").await;

    let body = sample_body("emergency", "Bogus");
    let err = create(
        admin_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path(model_id),
        Json(body),
    )
    .await
    .expect_err("invalid type must reject");

    match err {
        AppError::BadRequest(m) => assert!(m.to_lowercase().contains("type"), "got: {m}"),
        other => panic!("expected BadRequest, got {other:?}"),
    }
}

// ===========================================================================
// 5. item.responsibility='Backup' rejected with 400
// ===========================================================================

#[sqlx::test(migrations = "./migrations")]
async fn test_create_rejects_invalid_responsibility(pool: PgPool) {
    let org_id = default_org_id(&pool).await;
    let (user_id, user_code) = default_admin_user(&pool).await;
    let model_id = seed_model(&pool, org_id, "TestInvalidResp").await;

    let mut body = sample_body("normal_ops", "Bad Resp");
    body.sections[0].items[0].responsibility = "Backup".into();

    let err = create(
        admin_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path(model_id),
        Json(body),
    )
    .await
    .expect_err("invalid responsibility must reject");

    match err {
        AppError::BadRequest(m) => assert!(
            m.to_lowercase().contains("responsibility"),
            "got: {m}"
        ),
        other => panic!("expected BadRequest, got {other:?}"),
    }
}

// ===========================================================================
// 6. title='' rejected with 400
// ===========================================================================

#[sqlx::test(migrations = "./migrations")]
async fn test_create_rejects_empty_title(pool: PgPool) {
    let org_id = default_org_id(&pool).await;
    let (user_id, user_code) = default_admin_user(&pool).await;
    let model_id = seed_model(&pool, org_id, "TestEmptyTitle").await;

    let mut body = sample_body("normal_ops", "");
    body.title = "   ".into();

    let err = create(
        admin_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path(model_id),
        Json(body),
    )
    .await
    .expect_err("empty title must reject");

    match err {
        AppError::BadRequest(m) => assert!(m.to_lowercase().contains("title"), "got: {m}"),
        other => panic!("expected BadRequest, got {other:?}"),
    }
}

// ===========================================================================
// 7. GET /checklists/:id returns nested structure
// ===========================================================================

#[sqlx::test(migrations = "./migrations")]
async fn test_get_returns_nested_structure(pool: PgPool) {
    let org_id = default_org_id(&pool).await;
    let (user_id, user_code) = default_admin_user(&pool).await;
    let model_id = seed_model(&pool, org_id, "TestGetNested").await;

    let Json(created) = create(
        admin_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path(model_id),
        Json(sample_body("normal_ops", "Fetchable")),
    )
    .await
    .expect("create");

    let Json(fetched) = get(
        admin_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path(created.checklist.id),
    )
    .await
    .expect("get");

    assert_eq!(fetched.checklist.id, created.checklist.id);
    assert_eq!(fetched.sections.len(), 2);
    assert_eq!(fetched.sections[0].items.len(), 3);
    assert_eq!(fetched.sections[0].section.title, "Pre-flight");
    assert_eq!(fetched.sections[1].section.title, "Take-off");
    assert_eq!(fetched.sections[0].items[0].label, "Propellers");
    assert_eq!(fetched.sections[0].items[0].responsibility, "PF");
}

// ===========================================================================
// 8. GET /aircraft-models/:id/checklists returns summaries
// ===========================================================================

#[sqlx::test(migrations = "./migrations")]
async fn test_list_for_model_returns_summaries(pool: PgPool) {
    let org_id = default_org_id(&pool).await;
    let (user_id, user_code) = default_admin_user(&pool).await;
    let model_a = seed_model(&pool, org_id, "TestListModelA").await;
    let model_b = seed_model(&pool, org_id, "TestListModelB").await;

    // Two checklists for model A (one of each type).
    let _ = create(
        admin_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path(model_a),
        Json(sample_body("normal_ops", "A Normal")),
    )
    .await
    .expect("A normal");
    let _ = create(
        admin_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path(model_a),
        Json(sample_body("qrh", "A QRH")),
    )
    .await
    .expect("A qrh");

    // One checklist for model B — must not surface in model A's list.
    let _ = create(
        admin_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path(model_b),
        Json(sample_body("normal_ops", "B Normal")),
    )
    .await
    .expect("B normal");

    let Json(rows) = list(
        admin_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path(model_a),
    )
    .await
    .expect("list");

    assert_eq!(rows.len(), 2);
    for r in &rows {
        assert_eq!(r.model_id, model_a);
        assert_eq!(r.section_count, 2);
        assert_eq!(r.item_count, 6);
    }
    // Sorted by type asc, so normal_ops before qrh.
    assert_eq!(rows[0].r#type, "normal_ops");
    assert_eq!(rows[1].r#type, "qrh");
}

// ===========================================================================
// 9. Update replaces full nested payload (children swapped)
// ===========================================================================

#[sqlx::test(migrations = "./migrations")]
async fn test_update_replaces_full_nested_payload(pool: PgPool) {
    let org_id = default_org_id(&pool).await;
    let (user_id, user_code) = default_admin_user(&pool).await;
    let model_id = seed_model(&pool, org_id, "TestUpdateReplace").await;

    let Json(created) = create(
        admin_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path(model_id),
        Json(sample_body("normal_ops", "Original")),
    )
    .await
    .expect("create");
    assert_eq!(created.sections.len(), 2);

    // Replace with one section, two items.
    let updated_body = ChecklistInputBody {
        r#type: "normal_ops".into(),
        title: "Revised".into(),
        revision: Some("02".into()),
        revision_date: None,
        sections: vec![ChecklistSectionInput {
            title: "Only Section".into(),
            sort_order: 1,
            start_on_new_page: false,
            items: vec![
                ChecklistItemInput {
                    label: "Only A".into(),
                    value: "v".into(),
                    responsibility: "PF".into(),
                    sort_order: 1,
                },
                ChecklistItemInput {
                    label: "Only B".into(),
                    value: "v".into(),
                    responsibility: "PM".into(),
                    sort_order: 2,
                },
            ],
        }],
    };

    let Json(after) = update(
        admin_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path(created.checklist.id),
        Json(updated_body),
    )
    .await
    .expect("update");

    assert_eq!(after.checklist.title, "Revised");
    assert_eq!(after.checklist.revision, "02");
    assert_eq!(after.sections.len(), 1);
    assert_eq!(after.sections[0].items.len(), 2);

    let section_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM checklist_sections WHERE checklist_id = $1")
            .bind(created.checklist.id)
            .fetch_one(&pool)
            .await
            .expect("sections");
    assert_eq!(section_count.0, 1);

    let item_count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM checklist_items i \
         JOIN checklist_sections s ON s.id = i.section_id \
         WHERE s.checklist_id = $1",
    )
    .bind(created.checklist.id)
    .fetch_one(&pool)
    .await
    .expect("items");
    assert_eq!(item_count.0, 2);

    // Audit row carries before/after.
    let diff: (serde_json::Value,) = sqlx::query_as(
        "SELECT diff_jsonb FROM audit_log \
         WHERE table_name = 'checklists' AND action = 'UPDATE_CHECKLIST' AND record_id = $1",
    )
    .bind(created.checklist.id)
    .fetch_one(&pool)
    .await
    .expect("audit diff");
    let d = &diff.0;
    assert!(d.get("before").is_some(), "audit must carry before snapshot");
    assert!(d.get("after").is_some(), "audit must carry after snapshot");
    assert_eq!(
        d.pointer("/before/title").and_then(|v| v.as_str()),
        Some("Original")
    );
    assert_eq!(
        d.pointer("/after/title").and_then(|v| v.as_str()),
        Some("Revised")
    );
}

// ===========================================================================
// 10. type is immutable on update
// ===========================================================================

#[sqlx::test(migrations = "./migrations")]
async fn test_update_preserves_type_immutability(pool: PgPool) {
    let org_id = default_org_id(&pool).await;
    let (user_id, user_code) = default_admin_user(&pool).await;
    let model_id = seed_model(&pool, org_id, "TestTypeImmutable").await;

    let Json(created) = create(
        admin_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path(model_id),
        Json(sample_body("normal_ops", "Stay normal")),
    )
    .await
    .expect("create");

    let mut bad = sample_body("qrh", "Become QRH");
    bad.revision = Some("02".into());

    let err = update(
        admin_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path(created.checklist.id),
        Json(bad),
    )
    .await
    .expect_err("type change must reject");

    match err {
        AppError::BadRequest(m) => assert!(m.to_lowercase().contains("type"), "got: {m}"),
        other => panic!("expected BadRequest, got {other:?}"),
    }
}

// ===========================================================================
// 11. DELETE cascades to sections + items
// ===========================================================================

#[sqlx::test(migrations = "./migrations")]
async fn test_delete_cascades_sections_and_items(pool: PgPool) {
    let org_id = default_org_id(&pool).await;
    let (user_id, user_code) = default_admin_user(&pool).await;
    let model_id = seed_model(&pool, org_id, "TestDeleteCascade").await;

    let Json(created) = create(
        admin_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path(model_id),
        Json(sample_body("normal_ops", "Doomed")),
    )
    .await
    .expect("create");

    let resp = delete(
        admin_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path(created.checklist.id),
    )
    .await
    .expect("delete");
    assert_eq!(resp.status(), axum::http::StatusCode::NO_CONTENT);

    let cl: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM checklists WHERE id = $1")
            .bind(created.checklist.id)
            .fetch_one(&pool)
            .await
            .expect("count checklist");
    let sec: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM checklist_sections WHERE checklist_id = $1")
            .bind(created.checklist.id)
            .fetch_one(&pool)
            .await
            .expect("count sections");
    let items: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM checklist_items WHERE section_id IN \
            (SELECT id FROM checklist_sections WHERE checklist_id = $1)",
    )
    .bind(created.checklist.id)
    .fetch_one(&pool)
    .await
    .expect("count items");
    assert_eq!(cl.0, 0);
    assert_eq!(sec.0, 0);
    assert_eq!(items.0, 0);

    let audit_count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM audit_log \
         WHERE table_name = 'checklists' AND action = 'DELETE_CHECKLIST' AND record_id = $1",
    )
    .bind(created.checklist.id)
    .fetch_one(&pool)
    .await
    .expect("audit count");
    assert_eq!(audit_count.0, 1);
}

// ===========================================================================
// 12. Pilot is rejected on create/update/delete (all three mutating verbs)
// ===========================================================================

#[sqlx::test(migrations = "./migrations")]
async fn test_mutations_admin_only(pool: PgPool) {
    let org_id = default_org_id(&pool).await;
    let (admin_id, admin_code) = default_admin_user(&pool).await;
    let model_id = seed_model(&pool, org_id, "TestAdminOnly").await;

    // The pilot user reuses the admin's user_id — we only need a Claims with
    // role="pilot"; the user_id field doesn't gate authorisation.
    let pilot = pilot_claims(org_id, admin_id, &admin_code);

    // CREATE as pilot → 403
    let err = create(
        pilot.clone(),
        State(pool.clone()),
        Path(model_id),
        Json(sample_body("normal_ops", "Pilot attempt")),
    )
    .await
    .expect_err("create as pilot must 403");
    assert!(matches!(err, AppError::Forbidden(_)), "got {err:?}");

    // Seed a real checklist via admin so UPDATE and DELETE have a target.
    let Json(seeded) = create(
        admin_claims(org_id, admin_id, &admin_code),
        State(pool.clone()),
        Path(model_id),
        Json(sample_body("normal_ops", "Admin seed")),
    )
    .await
    .expect("seed");

    // UPDATE as pilot → 403
    let err = update(
        pilot.clone(),
        State(pool.clone()),
        Path(seeded.checklist.id),
        Json(sample_body("normal_ops", "Pilot edit")),
    )
    .await
    .expect_err("update as pilot must 403");
    assert!(matches!(err, AppError::Forbidden(_)), "got {err:?}");

    // DELETE as pilot → 403
    let err = delete(pilot, State(pool.clone()), Path(seeded.checklist.id))
        .await
        .expect_err("delete as pilot must 403");
    assert!(matches!(err, AppError::Forbidden(_)), "got {err:?}");
}

// ===========================================================================
// 13. Pilots can list and get
// ===========================================================================

#[sqlx::test(migrations = "./migrations")]
async fn test_list_and_get_accessible_to_pilot(pool: PgPool) {
    let org_id = default_org_id(&pool).await;
    let (user_id, user_code) = default_admin_user(&pool).await;
    let model_id = seed_model(&pool, org_id, "TestPilotReadOnly").await;

    let Json(seeded) = create(
        admin_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path(model_id),
        Json(sample_body("normal_ops", "Visible")),
    )
    .await
    .expect("seed");

    let Json(rows) = list(
        pilot_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path(model_id),
    )
    .await
    .expect("pilot list");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, seeded.checklist.id);

    let Json(full) = get(
        pilot_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path(seeded.checklist.id),
    )
    .await
    .expect("pilot get");
    assert_eq!(full.checklist.id, seeded.checklist.id);
    assert_eq!(full.sections.len(), 2);
}

// ===========================================================================
// 14. Cross-org access is 404 (existence not leaked)
// ===========================================================================

#[sqlx::test(migrations = "./migrations")]
async fn test_org_isolation_returns_404(pool: PgPool) {
    let org1 = default_org_id(&pool).await;
    let (admin1_id, admin1_code) = default_admin_user(&pool).await;
    let model_in_org1 = seed_model(&pool, org1, "TestOrgIsolation").await;

    let Json(seeded) = create(
        admin_claims(org1, admin1_id, &admin1_code),
        State(pool.clone()),
        Path(model_in_org1),
        Json(sample_body("normal_ops", "Org-1 secret")),
    )
    .await
    .expect("seed");

    // Spin up a second org with its own admin and try to read the org-1
    // checklist with org-2 credentials.
    let (org2, admin2_id, admin2_code) = seed_secondary_org_with_admin(&pool).await;

    let err = get(
        admin_claims(org2, admin2_id, &admin2_code),
        State(pool.clone()),
        Path(seeded.checklist.id),
    )
    .await
    .expect_err("cross-org get must 404");
    assert!(matches!(err, AppError::NotFound(_)), "got {err:?}");

    // Same for list against the model in org 1 from org 2's perspective —
    // the model_id is in another org, so ensure_model_in_org returns 404.
    let err = list(
        admin_claims(org2, admin2_id, &admin2_code),
        State(pool.clone()),
        Path(model_in_org1),
    )
    .await
    .expect_err("cross-org list must 404");
    assert!(matches!(err, AppError::NotFound(_)), "got {err:?}");
}
