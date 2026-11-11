// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

//! Tests for the model_documents handler — upload / replace / list /
//! download / delete plus the cleanup hook called from
//! aircraft_models::permanent_delete.
//!
//! Each test sets UPLOAD_DIR via std::env::set_var to a per-test path
//! under std::env::temp_dir() so filesystem state from one test never
//! leaks into another. The `tempfile` crate isn't available in
//! Cargo.toml, so we roll our own path with a counter + the test name.
//!
//! NOTE: std::env::set_var is process-global. `cargo test` runs in
//! threads inside one process, so tests in this module set UPLOAD_DIR
//! to **the same per-process root** + a per-test subdir derived from
//! the call site. We never read UPLOAD_DIR concurrently from two
//! handlers in conflicting ways: each test owns its model_id and its
//! files live under `aircraft_models/{model_id}/...`, which doesn't
//! collide with parallel tests using different model ids inside the
//! same upload-root. This is intentional pragmatism — refactoring the
//! handler to take an injected upload-dir would be cleaner but is out
//! of PR scope.

use super::*;
use crate::auth::middleware::Claims;
use axum::body::{to_bytes, Body};
use axum::extract::{Path, State};
use axum::http::Request;
use axum_extra::extract::Multipart;
use sqlx::{PgPool, Row};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};

// ---- Shared upload-root setup ----

/// One per-process root for every test in this file. The tests pick a
/// unique subdir under it via `unique_subdir` so cleanup is local and
/// parallel tests can't accidentally see each other's files.
fn upload_root() -> PathBuf {
    let root = std::env::temp_dir().join("dronefleetpro_model_doc_tests");
    let _ = std::fs::create_dir_all(&root);
    root
}

static SUBDIR_COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique_subdir(label: &str) -> PathBuf {
    let n = SUBDIR_COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = upload_root().join(format!("{}-{}", label, n));
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Module-wide lock for env-var-driven UPLOAD_DIR configuration. `cargo
/// test` runs tests in parallel threads of the same process, and
/// `std::env::set_var` is process-global — without a lock, test A's
/// UPLOAD_DIR value can be observed by test B's handler call and the
/// file lands in the wrong place. Every test in this module that
/// touches the filesystem acquires this lock for its duration.
fn upload_dir_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// Acquire the global UPLOAD_DIR lock, set UPLOAD_DIR to a fresh
/// per-test path, and return both. The caller MUST hold onto the
/// `MutexGuard` for the lifetime of the test — drop it after all
/// filesystem assertions are complete.
fn set_upload_dir<'a>(label: &str) -> (PathBuf, MutexGuard<'a, ()>) {
    // Recover from poisoned locks: a panicking test still holds useful
    // exclusivity, so we accept the inner guard either way.
    let guard = upload_dir_lock().lock().unwrap_or_else(|p| p.into_inner());
    let dir = unique_subdir(label);
    std::env::set_var("UPLOAD_DIR", &dir);
    (dir, guard)
}

// ---- DB seed helpers ----

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

async fn seed_secondary_org(pool: &PgPool) -> i32 {
    let row = sqlx::query(
        "INSERT INTO organisations (name, slug) VALUES ($1, $2) RETURNING id",
    )
    .bind("Test Secondary Org")
    .bind(format!("docs-org-{}", uuid::Uuid::new_v4()))
    .fetch_one(pool)
    .await
    .expect("secondary org insert");
    row.get::<i32, _>("id")
}

// ---- Multipart helpers ----

/// Builds a minimal multipart body for the upload endpoint and converts
/// it to axum's Multipart extractor. Reuses axum's own machinery so we
/// don't have to hand-roll a parser.
async fn make_multipart(category: &str, filename: &str, content: &[u8]) -> Multipart {
    let boundary = "----dfp_test_boundary_zzz";
    let mut body = Vec::new();
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        b"Content-Disposition: form-data; name=\"category\"\r\n\r\n",
    );
    body.extend_from_slice(category.as_bytes());
    body.extend_from_slice(b"\r\n");
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        format!(
            "Content-Disposition: form-data; name=\"file\"; filename=\"{}\"\r\n",
            filename
        )
        .as_bytes(),
    );
    body.extend_from_slice(b"Content-Type: application/pdf\r\n\r\n");
    body.extend_from_slice(content);
    body.extend_from_slice(b"\r\n");
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());

    let req = Request::builder()
        .method("POST")
        .uri("/")
        .header(
            "content-type",
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(Body::from(body))
        .expect("build req");

    Multipart::from_request(req, &())
        .await
        .expect("multipart parse")
}

fn pdf_bytes(payload: &str) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(b"%PDF-1.4\n");
    v.extend_from_slice(payload.as_bytes());
    // pad to >=100 bytes for realism
    while v.len() < 100 {
        v.push(b' ');
    }
    v
}

/// More flexible multipart builder for the multi-other-doc tests. Supports
/// optional `title` and `replace_doc_id` fields alongside the standard
/// category + file. When `category` is None, the field is omitted (used by
/// negative tests).
struct MultipartParts<'a> {
    category: Option<&'a str>,
    filename: &'a str,
    content: &'a [u8],
    title: Option<&'a str>,
    replace_doc_id: Option<i32>,
}

async fn make_multipart_full(p: MultipartParts<'_>) -> Multipart {
    let boundary = "----dfp_test_boundary_yyy";
    let mut body = Vec::new();

    if let Some(cat) = p.category {
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        body.extend_from_slice(b"Content-Disposition: form-data; name=\"category\"\r\n\r\n");
        body.extend_from_slice(cat.as_bytes());
        body.extend_from_slice(b"\r\n");
    }
    if let Some(t) = p.title {
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        body.extend_from_slice(b"Content-Disposition: form-data; name=\"title\"\r\n\r\n");
        body.extend_from_slice(t.as_bytes());
        body.extend_from_slice(b"\r\n");
    }
    if let Some(rid) = p.replace_doc_id {
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        body.extend_from_slice(b"Content-Disposition: form-data; name=\"replace_doc_id\"\r\n\r\n");
        body.extend_from_slice(rid.to_string().as_bytes());
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        format!(
            "Content-Disposition: form-data; name=\"file\"; filename=\"{}\"\r\n",
            p.filename
        )
        .as_bytes(),
    );
    body.extend_from_slice(b"Content-Type: application/pdf\r\n\r\n");
    body.extend_from_slice(p.content);
    body.extend_from_slice(b"\r\n");
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());

    let req = Request::builder()
        .method("POST")
        .uri("/")
        .header(
            "content-type",
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(Body::from(body))
        .expect("build req");
    Multipart::from_request(req, &())
        .await
        .expect("multipart parse")
}

// ---- Tests ----

use axum::extract::FromRequest;

#[sqlx::test(migrations = "./migrations")]
async fn test_upload_creates_document_writes_file_and_audits(pool: PgPool) {
    let (root, _lock) = set_upload_dir("upload_creates");
    let org_id = default_org_id(&pool).await;
    let (user_id, user_code) = default_admin_user(&pool).await;
    let model_id = seed_model(&pool, org_id, "TestUploadCreates").await;

    let body = pdf_bytes("hello");
    let mp = make_multipart("instruction_manual", "manual.pdf", &body).await;

    let Json(resp) = upload(
        admin_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path(model_id),
        mp,
    )
    .await
    .expect("upload ok");

    assert_eq!(resp.category, "instruction_manual");
    assert_eq!(resp.original_filename, "manual.pdf");
    assert_eq!(resp.size_bytes as usize, body.len());

    // File exists on disk under the per-test UPLOAD_DIR.
    let expected = root.join(format!("aircraft_models/{}/{}.pdf", model_id, resp.id));
    assert!(expected.exists(), "expected file at {:?}", expected);
    let written = std::fs::read(&expected).expect("read written");
    assert_eq!(written, body);

    // Audit row written with action UPLOAD_DOCUMENT.
    let audit_count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM audit_log \
         WHERE table_name = 'model_documents' AND record_id = $1 \
           AND action = 'UPLOAD_DOCUMENT'",
    )
    .bind(resp.id)
    .fetch_one(&pool)
    .await
    .expect("audit count");
    assert_eq!(audit_count.0, 1);
}

#[sqlx::test(migrations = "./migrations")]
async fn test_upload_replaces_existing_for_same_category(pool: PgPool) {
    let (root, _lock) = set_upload_dir("upload_replaces");
    let org_id = default_org_id(&pool).await;
    let (user_id, user_code) = default_admin_user(&pool).await;
    let model_id = seed_model(&pool, org_id, "TestUploadReplaces").await;

    let body_a = pdf_bytes("version-A");
    let mp_a = make_multipart("instruction_manual", "v1.pdf", &body_a).await;
    let Json(first) = upload(
        admin_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path(model_id),
        mp_a,
    )
    .await
    .expect("upload v1");
    let first_path = root.join(format!("aircraft_models/{}/{}.pdf", model_id, first.id));
    assert!(first_path.exists());

    let body_b = pdf_bytes("version-B with much more content padding for realism");
    let mp_b = make_multipart("instruction_manual", "v2.pdf", &body_b).await;
    let Json(second) = upload(
        admin_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path(model_id),
        mp_b,
    )
    .await
    .expect("upload v2 (replace)");
    let second_path = root.join(format!("aircraft_models/{}/{}.pdf", model_id, second.id));

    // Old gone, new there.
    assert!(!first_path.exists(), "v1 file should have been unlinked");
    assert!(second_path.exists(), "v2 file should exist");

    // Exactly one DB row in the same category.
    let count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM model_documents \
         WHERE model_id = $1 AND category = 'instruction_manual'",
    )
    .bind(model_id)
    .fetch_one(&pool)
    .await
    .expect("count");
    assert_eq!(count.0, 1);

    // Audit log: one UPLOAD_DOCUMENT (for first) + one REPLACE_DOCUMENT
    // (for second). The REPLACE row carries previous filename/size.
    let upload_audits: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM audit_log \
         WHERE table_name = 'model_documents' AND action = 'UPLOAD_DOCUMENT' AND record_id = $1",
    )
    .bind(first.id)
    .fetch_one(&pool)
    .await
    .expect("upload audit");
    assert_eq!(upload_audits.0, 1);

    let replace_diff: (serde_json::Value,) = sqlx::query_as(
        "SELECT diff_jsonb FROM audit_log \
         WHERE table_name = 'model_documents' AND action = 'REPLACE_DOCUMENT' AND record_id = $1",
    )
    .bind(second.id)
    .fetch_one(&pool)
    .await
    .expect("replace audit");
    let d = &replace_diff.0;
    assert_eq!(d.get("previous_filename").and_then(|v| v.as_str()), Some("v1.pdf"));
    assert_eq!(
        d.get("previous_size_bytes").and_then(|v| v.as_i64()),
        Some(body_a.len() as i64)
    );
    assert_eq!(d.get("new_filename").and_then(|v| v.as_str()), Some("v2.pdf"));
}

#[sqlx::test(migrations = "./migrations")]
async fn test_upload_rejects_non_pdf(pool: PgPool) {
    let (_root, _lock) = set_upload_dir("upload_rejects_non_pdf");
    let org_id = default_org_id(&pool).await;
    let (user_id, user_code) = default_admin_user(&pool).await;
    let model_id = seed_model(&pool, org_id, "TestRejectsNonPdf").await;

    let body = b"This is not a PDF file, just some plain text padding to exceed minimum length";
    // Use a manual category so the PDF-magic-byte check fires before any
    // 'other'-specific title-required validation.
    let mp = make_multipart("instruction_manual", "fake.pdf", body).await;

    let err = upload(
        admin_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path(model_id),
        mp,
    )
    .await
    .expect_err("non-PDF must be rejected");

    match err {
        AppError::BadRequest(m) => assert!(
            m.contains("PDF"),
            "expected PDF rejection message, got: {m}"
        ),
        other => panic!("expected BadRequest, got {other:?}"),
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn test_upload_rejects_non_admin(pool: PgPool) {
    let (_root, _lock) = set_upload_dir("upload_rejects_non_admin");
    let org_id = default_org_id(&pool).await;
    let (user_id, user_code) = default_admin_user(&pool).await;
    let model_id = seed_model(&pool, org_id, "TestRejectsNonAdmin").await;

    let body = pdf_bytes("admin-only");
    let mp = make_multipart("instruction_manual", "x.pdf", &body).await;

    let err = upload(
        pilot_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path(model_id),
        mp,
    )
    .await
    .expect_err("pilot must be rejected on upload");

    match err {
        AppError::Forbidden(_) => {}
        other => panic!("expected Forbidden, got {other:?}"),
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn test_list_documents_accessible_to_non_admin(pool: PgPool) {
    let (_root, _lock) = set_upload_dir("list_non_admin");
    let org_id = default_org_id(&pool).await;
    let (user_id, user_code) = default_admin_user(&pool).await;
    let model_id = seed_model(&pool, org_id, "TestListNonAdmin").await;

    let body = pdf_bytes("listing");
    let mp = make_multipart("instruction_manual", "doc.pdf", &body).await;
    let _ = upload(
        admin_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path(model_id),
        mp,
    )
    .await
    .expect("seed upload");

    let Json(rows) = list(
        pilot_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path(model_id),
    )
    .await
    .expect("pilot list ok");

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].category, "instruction_manual");
}

#[sqlx::test(migrations = "./migrations")]
async fn test_download_returns_pdf_with_disposition_header(pool: PgPool) {
    let (_root, _lock) = set_upload_dir("download_pdf");
    let org_id = default_org_id(&pool).await;
    let (user_id, user_code) = default_admin_user(&pool).await;
    let model_id = seed_model(&pool, org_id, "TestDownloadPdf").await;

    let body = pdf_bytes("download me");
    let mp = make_multipart("maintenance_manual", "service.pdf", &body).await;
    let Json(doc) = upload(
        admin_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path(model_id),
        mp,
    )
    .await
    .expect("seed upload");

    let resp = download(
        admin_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path((model_id, doc.id)),
    )
    .await
    .expect("download ok");

    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers().get(axum::http::header::CONTENT_TYPE).and_then(|v| v.to_str().ok()),
        Some("application/pdf")
    );
    let disp = resp
        .headers()
        .get(axum::http::header::CONTENT_DISPOSITION)
        .and_then(|v| v.to_str().ok())
        .expect("disp header");
    assert!(disp.contains("attachment"));
    assert!(disp.contains("service.pdf"));

    let body_bytes = to_bytes(resp.into_body(), 10 * 1024 * 1024)
        .await
        .expect("body");
    assert_eq!(body_bytes.as_ref(), body.as_slice());
}

#[sqlx::test(migrations = "./migrations")]
async fn test_download_accessible_to_non_admin(pool: PgPool) {
    let (_root, _lock) = set_upload_dir("download_non_admin");
    let org_id = default_org_id(&pool).await;
    let (user_id, user_code) = default_admin_user(&pool).await;
    let model_id = seed_model(&pool, org_id, "TestDownloadNonAdmin").await;

    let body = pdf_bytes("viewable by all");
    let mp = make_multipart("instruction_manual", "viewable.pdf", &body).await;
    let Json(doc) = upload(
        admin_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path(model_id),
        mp,
    )
    .await
    .expect("seed upload");

    let resp = download(
        pilot_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path((model_id, doc.id)),
    )
    .await
    .expect("pilot download ok");
    assert_eq!(resp.status(), 200);
}

#[sqlx::test(migrations = "./migrations")]
async fn test_download_404_for_other_org_model(pool: PgPool) {
    let (_root, _lock) = set_upload_dir("download_other_org");
    let org_id_1 = default_org_id(&pool).await;
    let (user_id, user_code) = default_admin_user(&pool).await;
    let org_id_2 = seed_secondary_org(&pool).await;
    let foreign_model = seed_model(&pool, org_id_2, "ForeignModel").await;

    // Pretend the caller has an ID, but they're in org 1. We don't even
    // seed a real document in org 2 — the SELECT WHERE org_id=$1 returns
    // None regardless. 404 is what we expect.
    let err = download(
        admin_claims(org_id_1, user_id, &user_code),
        State(pool.clone()),
        Path((foreign_model, 999_999)),
    )
    .await
    .expect_err("foreign-org doc must 404");

    match err {
        AppError::NotFound(_) => {}
        other => panic!("expected NotFound, got {other:?}"),
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn test_delete_unlinks_file_and_audits_admin_only(pool: PgPool) {
    let (root, _lock) = set_upload_dir("delete_admin");
    let org_id = default_org_id(&pool).await;
    let (user_id, user_code) = default_admin_user(&pool).await;
    let model_id = seed_model(&pool, org_id, "TestDeleteAdmin").await;

    let body = pdf_bytes("to be deleted");
    let mp = make_multipart("instruction_manual", "del.pdf", &body).await;
    let Json(doc) = upload(
        admin_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path(model_id),
        mp,
    )
    .await
    .expect("seed upload");
    let file_path = root.join(format!("aircraft_models/{}/{}.pdf", model_id, doc.id));
    assert!(file_path.exists());

    // Pilot is rejected.
    let err = delete(
        pilot_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path((model_id, doc.id)),
    )
    .await
    .expect_err("pilot must be rejected on delete");
    match err {
        AppError::Forbidden(_) => {}
        other => panic!("expected Forbidden, got {other:?}"),
    }
    // File should still exist after the rejected delete.
    assert!(file_path.exists(), "rejected delete must not unlink file");

    // Admin succeeds.
    let resp = delete(
        admin_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path((model_id, doc.id)),
    )
    .await
    .expect("admin delete ok");
    assert_eq!(resp.status(), axum::http::StatusCode::NO_CONTENT);

    // File gone, row gone, audit row written.
    assert!(!file_path.exists(), "file must be unlinked");
    let count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM model_documents WHERE id = $1")
            .bind(doc.id)
            .fetch_one(&pool)
            .await
            .expect("count");
    assert_eq!(count.0, 0);
    let audit_count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM audit_log \
         WHERE table_name = 'model_documents' AND action = 'DELETE_DOCUMENT' AND record_id = $1",
    )
    .bind(doc.id)
    .fetch_one(&pool)
    .await
    .expect("audit count");
    assert_eq!(audit_count.0, 1);
}

#[sqlx::test(migrations = "./migrations")]
async fn test_permanent_delete_model_unlinks_documents_first(pool: PgPool) {
    let (root, _lock) = set_upload_dir("permanent_delete_unlinks");
    let org_id = default_org_id(&pool).await;
    let (user_id, user_code) = default_admin_user(&pool).await;
    let model_id = seed_model(&pool, org_id, "TestPermanentDeleteUnlinks").await;

    // Two docs (different categories — UNIQUE(model_id, category) blocks
    // two of the same).
    let body_a = pdf_bytes("doc a");
    let mp_a = make_multipart("instruction_manual", "a.pdf", &body_a).await;
    let Json(doc_a) = upload(
        admin_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path(model_id),
        mp_a,
    )
    .await
    .expect("seed a");
    let path_a = root.join(format!("aircraft_models/{}/{}.pdf", model_id, doc_a.id));

    let body_b = pdf_bytes("doc b");
    let mp_b = make_multipart("maintenance_manual", "b.pdf", &body_b).await;
    let Json(doc_b) = upload(
        admin_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path(model_id),
        mp_b,
    )
    .await
    .expect("seed b");
    let path_b = root.join(format!("aircraft_models/{}/{}.pdf", model_id, doc_b.id));

    assert!(path_a.exists() && path_b.exists());

    // Call the cleanup hook directly. Mirrors what
    // aircraft_models::permanent_delete does before the CASCADE.
    super::unlink_files_for_model(&pool, model_id, org_id).await;

    assert!(!path_a.exists(), "doc_a file must be unlinked");
    assert!(!path_b.exists(), "doc_b file must be unlinked");
}

// ===========================================================================
// PR #80 — multi-'other' tests
// ===========================================================================

/// Uploading category='other' with no title field must return 400 before
/// touching the disk or the DB.
#[sqlx::test(migrations = "./migrations")]
async fn test_upload_other_requires_title(pool: PgPool) {
    let (_root, _lock) = set_upload_dir("other_requires_title");
    let org_id = default_org_id(&pool).await;
    let (user_id, user_code) = default_admin_user(&pool).await;
    let model_id = seed_model(&pool, org_id, "TestOtherRequiresTitle").await;

    let body = pdf_bytes("no title here");
    let mp = make_multipart_full(MultipartParts {
        category: Some("other"),
        filename: "x.pdf",
        content: &body,
        title: None,
        replace_doc_id: None,
    })
    .await;

    let err = upload(
        admin_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path(model_id),
        mp,
    )
    .await
    .expect_err("must reject missing title");

    match err {
        AppError::BadRequest(m) => assert!(
            m.to_lowercase().contains("title"),
            "expected title-required message, got: {m}"
        ),
        other => panic!("expected BadRequest, got {other:?}"),
    }
}

/// Three uploads with category='other' and distinct titles must all
/// succeed. Partial unique index permits this; rows persist with their
/// individual titles.
#[sqlx::test(migrations = "./migrations")]
async fn test_upload_other_with_title_creates_new_row(pool: PgPool) {
    let (_root, _lock) = set_upload_dir("other_with_title");
    let org_id = default_org_id(&pool).await;
    let (user_id, user_code) = default_admin_user(&pool).await;
    let model_id = seed_model(&pool, org_id, "TestOtherMultiple").await;

    for (n, title) in [
        ("first", "Battery Replacement Guide"),
        ("second", "Mounting Bracket Diagram"),
        ("third", "GPS Calibration Procedure"),
    ] {
        let body = pdf_bytes(n);
        let mp = make_multipart_full(MultipartParts {
            category: Some("other"),
            filename: &format!("{}.pdf", n),
            content: &body,
            title: Some(title),
            replace_doc_id: None,
        })
        .await;
        let Json(doc) = upload(
            admin_claims(org_id, user_id, &user_code),
            State(pool.clone()),
            Path(model_id),
            mp,
        )
        .await
        .expect("upload other");
        assert_eq!(doc.category, "other");
        assert_eq!(doc.title.as_deref(), Some(title));
    }

    let rows: Vec<(i32, String, Option<String>)> = sqlx::query_as(
        "SELECT id, category, title FROM model_documents \
         WHERE model_id = $1 ORDER BY id",
    )
    .bind(model_id)
    .fetch_all(&pool)
    .await
    .expect("list rows");
    assert_eq!(rows.len(), 3);
    let titles: Vec<&str> = rows.iter().filter_map(|(_, _, t)| t.as_deref()).collect();
    assert_eq!(
        titles,
        vec![
            "Battery Replacement Guide",
            "Mounting Bracket Diagram",
            "GPS Calibration Procedure"
        ]
    );
}

/// Manuals (instruction_manual / maintenance_manual) must not accept a
/// title field. Reject before the CHECK constraint catches it so the
/// caller gets a friendly 400 instead of a generic DB conflict.
#[sqlx::test(migrations = "./migrations")]
async fn test_upload_manual_with_title_rejected(pool: PgPool) {
    let (_root, _lock) = set_upload_dir("manual_with_title");
    let org_id = default_org_id(&pool).await;
    let (user_id, user_code) = default_admin_user(&pool).await;
    let model_id = seed_model(&pool, org_id, "TestManualWithTitle").await;

    let body = pdf_bytes("title not allowed here");
    let mp = make_multipart_full(MultipartParts {
        category: Some("instruction_manual"),
        filename: "m.pdf",
        content: &body,
        title: Some("Should not be set"),
        replace_doc_id: None,
    })
    .await;

    let err = upload(
        admin_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path(model_id),
        mp,
    )
    .await
    .expect_err("title on manual must be rejected");

    match err {
        AppError::BadRequest(m) => assert!(
            m.to_lowercase().contains("title"),
            "expected title-forbidden message, got: {m}"
        ),
        other => panic!("expected BadRequest, got {other:?}"),
    }
}

/// replace_doc_id targets a specific 'other' document for replacement.
/// The targeted doc is unlinked + DB-deleted; the new doc lands with a
/// fresh id and new title; sibling 'other' docs are untouched; audit
/// REPLACE_DOCUMENT row carries previous_id.
#[sqlx::test(migrations = "./migrations")]
async fn test_upload_other_replace_specific_doc(pool: PgPool) {
    let (root, _lock) = set_upload_dir("other_replace_specific");
    let org_id = default_org_id(&pool).await;
    let (user_id, user_code) = default_admin_user(&pool).await;
    let model_id = seed_model(&pool, org_id, "TestOtherReplaceSpecific").await;

    // Seed two 'other' docs.
    let body_a = pdf_bytes("first other");
    let mp_a = make_multipart_full(MultipartParts {
        category: Some("other"),
        filename: "a.pdf",
        content: &body_a,
        title: Some("First Other"),
        replace_doc_id: None,
    })
    .await;
    let Json(doc_a) = upload(
        admin_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path(model_id),
        mp_a,
    )
    .await
    .expect("seed a");
    let path_a = root.join(format!("aircraft_models/{}/{}.pdf", model_id, doc_a.id));

    let body_b = pdf_bytes("second other");
    let mp_b = make_multipart_full(MultipartParts {
        category: Some("other"),
        filename: "b.pdf",
        content: &body_b,
        title: Some("Second Other"),
        replace_doc_id: None,
    })
    .await;
    let Json(doc_b) = upload(
        admin_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path(model_id),
        mp_b,
    )
    .await
    .expect("seed b");
    let path_b = root.join(format!("aircraft_models/{}/{}.pdf", model_id, doc_b.id));

    assert!(path_a.exists() && path_b.exists());

    // Replace doc_a specifically.
    let body_c = pdf_bytes("replacement for a");
    let mp_c = make_multipart_full(MultipartParts {
        category: Some("other"),
        filename: "a_v2.pdf",
        content: &body_c,
        title: Some("First Other (revised)"),
        replace_doc_id: Some(doc_a.id),
    })
    .await;
    let Json(doc_c) = upload(
        admin_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path(model_id),
        mp_c,
    )
    .await
    .expect("replace specific");

    assert_ne!(doc_c.id, doc_a.id, "new doc must have a fresh id");
    assert_eq!(doc_c.title.as_deref(), Some("First Other (revised)"));

    // doc_a is gone (row + file). doc_b untouched.
    let a_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM model_documents WHERE id = $1")
            .bind(doc_a.id)
            .fetch_one(&pool)
            .await
            .expect("count a");
    assert_eq!(a_count.0, 0, "doc_a row must be deleted");
    assert!(!path_a.exists(), "doc_a file must be unlinked");
    assert!(path_b.exists(), "doc_b file must be intact");

    // Audit row REPLACE_DOCUMENT carries previous_id = doc_a.id.
    let diff: (serde_json::Value,) = sqlx::query_as(
        "SELECT diff_jsonb FROM audit_log \
         WHERE table_name = 'model_documents' AND action = 'REPLACE_DOCUMENT' AND record_id = $1",
    )
    .bind(doc_c.id)
    .fetch_one(&pool)
    .await
    .expect("replace audit");
    let d = &diff.0;
    assert_eq!(
        d.get("previous_id").and_then(|v| v.as_i64()),
        Some(doc_a.id as i64)
    );
    assert_eq!(
        d.get("previous_title").and_then(|v| v.as_str()),
        Some("First Other")
    );
    assert_eq!(
        d.get("new_title").and_then(|v| v.as_str()),
        Some("First Other (revised)")
    );
}

/// replace_doc_id must target a document belonging to the same model.
#[sqlx::test(migrations = "./migrations")]
async fn test_upload_other_replace_doc_id_must_match_model(pool: PgPool) {
    let (_root, _lock) = set_upload_dir("other_replace_cross_model");
    let org_id = default_org_id(&pool).await;
    let (user_id, user_code) = default_admin_user(&pool).await;
    let model_a = seed_model(&pool, org_id, "TestReplaceCrossModelA").await;
    let model_b = seed_model(&pool, org_id, "TestReplaceCrossModelB").await;

    // Seed an 'other' doc on model A.
    let body = pdf_bytes("model A doc");
    let mp = make_multipart_full(MultipartParts {
        category: Some("other"),
        filename: "a.pdf",
        content: &body,
        title: Some("Model A Doc"),
        replace_doc_id: None,
    })
    .await;
    let Json(doc_a) = upload(
        admin_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path(model_a),
        mp,
    )
    .await
    .expect("seed");

    // Try to "replace" it via an upload to model B.
    let body2 = pdf_bytes("cross-model attempt");
    let mp2 = make_multipart_full(MultipartParts {
        category: Some("other"),
        filename: "b.pdf",
        content: &body2,
        title: Some("Cross-Model Attempt"),
        replace_doc_id: Some(doc_a.id),
    })
    .await;
    let err = upload(
        admin_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path(model_b),
        mp2,
    )
    .await
    .expect_err("cross-model replace must reject");

    match err {
        AppError::BadRequest(m) => assert!(
            m.contains("not found"),
            "expected not-found message, got: {m}"
        ),
        other => panic!("expected BadRequest, got {other:?}"),
    }
}

/// replace_doc_id targeted at a manual (non-'other') document must be
/// rejected — the manual replace-by-category path doesn't go through
/// replace_doc_id, and we don't want to let one upload silently cross
/// category boundaries.
#[sqlx::test(migrations = "./migrations")]
async fn test_upload_other_replace_doc_id_must_be_other_category(pool: PgPool) {
    let (_root, _lock) = set_upload_dir("other_replace_cross_category");
    let org_id = default_org_id(&pool).await;
    let (user_id, user_code) = default_admin_user(&pool).await;
    let model_id = seed_model(&pool, org_id, "TestReplaceCrossCategory").await;

    // Seed an instruction_manual.
    let body = pdf_bytes("instruction manual content");
    let mp = make_multipart_full(MultipartParts {
        category: Some("instruction_manual"),
        filename: "im.pdf",
        content: &body,
        title: None,
        replace_doc_id: None,
    })
    .await;
    let Json(manual) = upload(
        admin_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path(model_id),
        mp,
    )
    .await
    .expect("seed manual");

    // Try to replace it via category='other' with replace_doc_id pointing at
    // the manual's id.
    let body2 = pdf_bytes("attempt cross-category replace");
    let mp2 = make_multipart_full(MultipartParts {
        category: Some("other"),
        filename: "x.pdf",
        content: &body2,
        title: Some("Should be rejected"),
        replace_doc_id: Some(manual.id),
    })
    .await;
    let err = upload(
        admin_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path(model_id),
        mp2,
    )
    .await
    .expect_err("cross-category replace must reject");

    match err {
        AppError::BadRequest(m) => assert!(
            m.to_lowercase().contains("category"),
            "expected category-mismatch message, got: {m}"
        ),
        other => panic!("expected BadRequest, got {other:?}"),
    }
}

/// Direct INSERT (not via handler) — the partial unique index must allow
/// multiple 'other' rows for the same model.
#[sqlx::test(migrations = "./migrations")]
async fn test_partial_unique_index_allows_multiple_other_per_model(pool: PgPool) {
    let org_id = default_org_id(&pool).await;
    let (user_id, _user_code) = default_admin_user(&pool).await;
    let model_id = seed_model(&pool, org_id, "TestPartialIdxAllowsOther").await;

    for n in 1..=2 {
        sqlx::query(
            "INSERT INTO model_documents \
               (model_id, org_id, category, title, original_filename, file_path, \
                size_bytes, mime_type, uploaded_by) \
             VALUES ($1, $2, 'other', $3, $4, $5, 1, 'application/pdf', $6)",
        )
        .bind(model_id)
        .bind(org_id)
        .bind(format!("Other Title {}", n))
        .bind(format!("file_{}.pdf", n))
        .bind(format!("aircraft_models/{}/inserted_{}.pdf", model_id, n))
        .bind(user_id)
        .execute(&pool)
        .await
        .expect("direct insert of 'other' row");
    }

    let cnt: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM model_documents \
         WHERE model_id = $1 AND category = 'other'",
    )
    .bind(model_id)
    .fetch_one(&pool)
    .await
    .expect("count");
    assert_eq!(cnt.0, 2);
}

/// Direct INSERT — the partial unique index must still block duplicate
/// non-'other' rows for the same model+category.
#[sqlx::test(migrations = "./migrations")]
async fn test_partial_unique_index_blocks_duplicate_manual(pool: PgPool) {
    let org_id = default_org_id(&pool).await;
    let (user_id, _user_code) = default_admin_user(&pool).await;
    let model_id = seed_model(&pool, org_id, "TestPartialIdxBlocksManual").await;

    sqlx::query(
        "INSERT INTO model_documents \
           (model_id, org_id, category, title, original_filename, file_path, \
            size_bytes, mime_type, uploaded_by) \
         VALUES ($1, $2, 'instruction_manual', NULL, $3, $4, 1, 'application/pdf', $5)",
    )
    .bind(model_id)
    .bind(org_id)
    .bind("manual_1.pdf")
    .bind(format!("aircraft_models/{}/m1.pdf", model_id))
    .bind(user_id)
    .execute(&pool)
    .await
    .expect("first manual insert");

    let dup = sqlx::query(
        "INSERT INTO model_documents \
           (model_id, org_id, category, title, original_filename, file_path, \
            size_bytes, mime_type, uploaded_by) \
         VALUES ($1, $2, 'instruction_manual', NULL, $3, $4, 1, 'application/pdf', $5)",
    )
    .bind(model_id)
    .bind(org_id)
    .bind("manual_2.pdf")
    .bind(format!("aircraft_models/{}/m2.pdf", model_id))
    .bind(user_id)
    .execute(&pool)
    .await;

    let err = dup.expect_err("duplicate manual must be blocked");
    let msg = err.to_string();
    assert!(
        msg.to_lowercase().contains("unique")
            || msg.to_lowercase().contains("duplicate")
            || msg.to_lowercase().contains("model_documents_model_category_unique_idx"),
        "expected unique-violation error, got: {msg}"
    );
}
