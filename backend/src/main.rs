// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

mod auth;
mod config;
mod db;
mod errors;
mod handlers;
mod models;
mod services;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::{
    extract::DefaultBodyLimit,
    http::{HeaderValue, Method},
    routing::{delete, get, patch, post, put},
    Router,
};
use tower_governor::{
    errors::GovernorError, governor::GovernorConfigBuilder,
    key_extractor::SmartIpKeyExtractor, GovernorLayer,
};
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

/// Rate-limit response handler. Emits `Retry-After` (the standard header
/// per RFC 7231 §7.1.3) in addition to tower_governor's default
/// `x-ratelimit-after`, so HTTP clients and Bruno tests can read either.
/// The body is a small JSON envelope matching the rest of the API's error
/// shape.
fn rate_limit_error_response(err: GovernorError) -> axum::response::Response {
    use axum::body::Body;
    use axum::http::{HeaderValue, Response, StatusCode};

    match err {
        GovernorError::TooManyRequests { wait_time, .. } => {
            let body = format!(
                "{{\"error\":\"Too many requests\",\"retry_after_seconds\":{wait_time}}}"
            );
            let mut resp = Response::new(Body::from(body));
            *resp.status_mut() = StatusCode::TOO_MANY_REQUESTS;
            let headers = resp.headers_mut();
            headers.insert(
                axum::http::header::RETRY_AFTER,
                HeaderValue::from(wait_time),
            );
            headers.insert(
                axum::http::header::CONTENT_TYPE,
                HeaderValue::from_static("application/json"),
            );
            // Keep the legacy header for backwards-compatibility with any
            // existing clients that already consume it.
            if let Ok(v) = HeaderValue::try_from(wait_time.to_string()) {
                headers.insert("x-ratelimit-after", v);
            }
            resp
        }
        GovernorError::UnableToExtractKey => {
            let mut resp = Response::new(Body::from(
                r#"{"error":"Unable to extract rate-limit key"}"#,
            ));
            *resp.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
            resp.headers_mut().insert(
                axum::http::header::CONTENT_TYPE,
                HeaderValue::from_static("application/json"),
            );
            resp
        }
        GovernorError::Other { code, msg, headers } => {
            let body = msg.unwrap_or_else(|| "Rate limiter error".to_string());
            let mut resp = Response::new(Body::from(body));
            *resp.status_mut() = code;
            if let Some(h) = headers {
                for (k, v) in h.iter() {
                    resp.headers_mut().insert(k, v.clone());
                }
            }
            resp
        }
    }
}

#[tokio::main]
async fn main() {
    // Initialize logging.
    //
    // Default level when RUST_LOG is not set is `info`. RUST_LOG, when set,
    // overrides per-target — e.g. `RUST_LOG=info,sqlx=warn` — and is the
    // standard tracing-subscriber convention. Production deploys should
    // leave RUST_LOG unset (or set to `info`) to avoid leaking debug-level
    // detail into logs.
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(env_filter).init();

    // Load environment variables
    dotenvy::dotenv().ok();
    let config = config::Config::from_env();

    // Document uploads (model_documents handler) read UPLOAD_DIR directly from
    // env at request time — surfacing the path here at startup confirms the
    // value made it through dotenv and resolves the dead-code warning on
    // `Config.upload_dir`.
    tracing::info!("Upload directory: {}", config.upload_dir);

    // Create database pool
    let pool = db::create_pool(&config.database_url).await;

    // Run migrations.
    // Note: `sqlx::migrate!` is a compile-time macro that embeds the
    // contents of ./migrations into the binary; adding a new .sql file
    // requires a recompile of this source file.
    tracing::info!("Running database migrations...");
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("Failed to run database migrations");
    tracing::info!("Migrations complete.");

    // Neutralise the publicly known seed admin password (migration 002)
    // before the server accepts any connection. An empty value is treated
    // as unset. If this step fails the backend must not start: it would be
    // serving logins against a password that is published in the repository.
    let bootstrap_password = std::env::var("DFP_BOOTSTRAP_ADMIN_PASSWORD")
        .ok()
        .filter(|v| !v.is_empty());
    match services::admin_bootstrap::run(&pool, bootstrap_password).await {
        Ok(report) if report.replaced > 0 || report.pending > 0 => tracing::info!(
            "Admin bootstrap: {} seed password(s) replaced, {} account(s) awaiting a set-password link.",
            report.replaced,
            report.pending
        ),
        Ok(_) => {}
        Err(e) => {
            tracing::error!(
                "Admin bootstrap failed: {:?}. Refusing to start while an account may still carry the public seed password.",
                e
            );
            std::process::exit(1);
        }
    }

    // CORS — debug builds (cargo run, Dockerfile.dev) are dev-permissive so
    // LAN access (e.g. http://192.168.x.x:5173) works without env-var
    // tuning. Release builds keep the explicit allowlist from CORS_ORIGINS.
    // Safe to use Any in dev because JWT is sent in the Authorization
    // header (not a cookie), so allow_credentials(true) is not needed —
    // and Any cannot be combined with allow_credentials in any case.
    let cors = if cfg!(debug_assertions) {
        tracing::info!("CORS: dev-permissive mode (allow_origin: Any)");
        CorsLayer::new()
            .allow_origin(Any)
            .allow_methods([
                Method::GET,
                Method::POST,
                Method::PUT,
                Method::DELETE,
                Method::OPTIONS,
            ])
            .allow_headers(Any)
    } else {
        // The default covers local use only. A production deployment must
        // list its own public origin in CORS_ORIGINS.
        let default_origins = "http://localhost:5173";
        let cors_origins_env = std::env::var("CORS_ORIGINS")
            .unwrap_or_else(|_| default_origins.to_string());
        let allowed_origins: Vec<HeaderValue> = cors_origins_env
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .filter_map(|s| s.parse::<HeaderValue>().ok())
            .collect();
        if allowed_origins.is_empty() {
            panic!("CORS_ORIGINS resolved to an empty list; refusing to start with no allowed origins");
        }
        tracing::info!(
            "CORS allowed origins: {:?}",
            allowed_origins
                .iter()
                .filter_map(|v| v.to_str().ok())
                .collect::<Vec<_>>()
        );
        CorsLayer::new()
            .allow_origin(allowed_origins)
            .allow_methods([
                Method::GET,
                Method::POST,
                Method::PUT,
                Method::DELETE,
                Method::OPTIONS,
            ])
            .allow_headers([
                axum::http::header::AUTHORIZATION,
                axum::http::header::CONTENT_TYPE,
                axum::http::header::ACCEPT,
            ])
    };

    // Rate limiter for unauthenticated/credential-handling auth endpoints.
    // Steady-state: 1 token per `RATE_LIMIT_AUTH_PER_SECONDS` seconds, with
    // a `RATE_LIMIT_AUTH_BURST` cell bucket. Default 12 / 6 → effectively
    // 5 attempts before a lockout, replenishing at 5/min (matches the
    // security plan; burst is set to 6 because tower_governor's
    // accounting trips on the burst_size'th request, not the
    // (burst_size+1)'th).
    //
    // The env vars exist so dev/test environments — where the Bruno
    // user-management suite legitimately makes a dozen auth calls per
    // run — can boost the limit without changing code. SmartIpKeyExtractor
    // reads the client IP from X-Forwarded-For (set by nginx) and falls
    // back to the peer address. Returns HTTP 429 with `Retry-After` on
    // rate-limit hit.
    let rate_per_seconds: u64 = std::env::var("RATE_LIMIT_AUTH_PER_SECONDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(12);
    let rate_burst: u32 = std::env::var("RATE_LIMIT_AUTH_BURST")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(6);
    tracing::info!(
        "Auth rate limit: 1 token per {} s, burst {}",
        rate_per_seconds,
        rate_burst
    );
    let governor_conf = Arc::new(
        GovernorConfigBuilder::default()
            .per_second(rate_per_seconds)
            .burst_size(rate_burst)
            .key_extractor(SmartIpKeyExtractor)
            .error_handler(rate_limit_error_response)
            .finish()
            .expect("rate limiter config must build"),
    );
    // Periodic cleanup of stale per-IP buckets. Without this, the
    // limiter's storage grows unbounded over the lifetime of the process.
    let governor_limiter = governor_conf.limiter().clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(60));
        // Skip the immediate first tick.
        tick.tick().await;
        loop {
            tick.tick().await;
            governor_limiter.retain_recent();
        }
    });

    // Sub-router: rate-limited auth-credential endpoints. Listed here only;
    // not added to the main router below to avoid double-routing.
    let rate_limited_auth = Router::new()
        .route("/api/v1/auth/login", post(handlers::auth::login))
        .route("/api/v1/auth/redeem-token", post(handlers::user_tokens::redeem))
        .route("/api/v1/auth/change-password", post(handlers::auth::change_password))
        .layer(GovernorLayer { config: governor_conf });

    // Main router — all non-rate-limited routes. Auth is enforced
    // per-handler via the `Claims` extractor.
    let main_router = Router::new()
        // Public auth (token issuance/inspection — not the credential paths)
        .route("/api/v1/auth/refresh", post(handlers::auth::refresh))
        .route("/api/v1/auth/logout", post(handlers::auth::logout))
        .route("/api/v1/auth/me", get(handlers::auth::me))
        // Dashboard
        .route("/api/v1/dashboard", get(handlers::dashboard::get_dashboard))
        .route("/api/v1/dashboard/flight-map", get(handlers::dashboard::get_flight_map))
        // Flights
        .route("/api/v1/flights", get(handlers::flights::list_flights).post(handlers::flights::create_flight))
        .route("/api/v1/flights/stats", get(handlers::flights::get_stats))
        .route("/api/v1/flights/filters", get(handlers::flights::filter_options))
        .route("/api/v1/flights/import", post(handlers::flights::import_dji_log))
        // ArduPilot import opts up to a 256 MB body limit — DataFlash .bin
        // logs routinely reach 100+ MB (nginx /api/ is already 256M).
        .route("/api/v1/flights/import-ardupilot",
               post(handlers::flights::import_ardupilot_log)
                   .layer(DefaultBodyLimit::max(256 * 1024 * 1024)))
        .route("/api/v1/flights/commit", post(handlers::flights::commit_imported_flight))
        .route("/api/v1/flights/:id", get(handlers::flights::get_flight).put(handlers::flights::update_flight).delete(handlers::flights::delete_flight))
        .route("/api/v1/flights/:id/detail", get(handlers::flights::get_flight_detail))
        // Aircraft
        .route("/api/v1/aircraft", get(handlers::aircraft::list_aircraft).post(handlers::aircraft::create_aircraft))
        .route("/api/v1/aircraft/:id", get(handlers::aircraft::get_aircraft).put(handlers::aircraft::update_aircraft).delete(handlers::aircraft::retire_aircraft))
        .route("/api/v1/aircraft/:id/airworthiness",
               patch(handlers::aircraft::patch_airworthiness))
        .route("/api/v1/aircraft/:id/detail", get(handlers::aircraft_detail::get_aircraft_detail))
        .route("/api/v1/aircraft/:id/breakdowns", get(handlers::aircraft_detail::get_aircraft_breakdowns))
        .route("/api/v1/aircraft/:source_id/merge", post(handlers::aircraft::merge_aircraft_handler))
        // Aircraft models (admin-only catalog)
        .route("/api/v1/aircraft-models",
               get(handlers::aircraft_models::list).post(handlers::aircraft_models::create))
        .route("/api/v1/aircraft-models/:id",
               get(handlers::aircraft_models::get)
                   .put(handlers::aircraft_models::update)
                   .delete(handlers::aircraft_models::retire))
        .route("/api/v1/aircraft-models/:id/merge",
               post(handlers::aircraft_models::merge))
        .route("/api/v1/aircraft-models/:id/permanent-delete",
               post(handlers::aircraft_models::permanent_delete))
        .route("/api/v1/aircraft-models/:id/stats",
               get(handlers::aircraft_models::get_stats))
        // Model documents — list (any role) + upload (admin) on the same path;
        // download + delete on the per-doc path. Upload route opts up to a
        // 100 MB body limit; the rest inherit the global 50 MB.
        .route("/api/v1/aircraft-models/:id/documents",
               get(handlers::model_documents::list)
                   .post(handlers::model_documents::upload)
                   .layer(DefaultBodyLimit::max(100 * 1024 * 1024)))
        .route("/api/v1/aircraft-models/:id/documents/:doc_id",
               delete(handlers::model_documents::delete))
        .route("/api/v1/aircraft-models/:id/documents/:doc_id/download",
               get(handlers::model_documents::download))
        // Checklists (per-model). View by anyone; mutate by admin.
        .route("/api/v1/aircraft-models/:id/checklists",
               get(handlers::checklists::list)
                   .post(handlers::checklists::create))
        .route("/api/v1/checklists/:id",
               get(handlers::checklists::get)
                   .put(handlers::checklists::update)
                   .delete(handlers::checklists::delete))
        // Maintenance instruction templates (read-only; any authenticated role)
        .route("/api/v1/maintenance-instruction-templates",
               get(handlers::maintenance_instruction_templates::list_templates))
        .route("/api/v1/maintenance-instruction-templates/:id",
               get(handlers::maintenance_instruction_templates::get_template_detail))
        .route("/api/v1/maintenance-instruction-templates",
               post(handlers::maintenance_instruction_templates::create_template))
        .route("/api/v1/maintenance-instruction-templates/:id",
               patch(handlers::maintenance_instruction_templates::patch_template))
        .route("/api/v1/maintenance-instruction-templates/:id/items",
               put(handlers::maintenance_instruction_templates::update_items))
        .route("/api/v1/maintenance-instruction-templates/:id/revisions",
               post(handlers::maintenance_instruction_templates::create_revision))
        // Batteries
        .route("/api/v1/batteries", get(handlers::batteries::list_batteries).post(handlers::batteries::create_battery))
        .route("/api/v1/batteries/:id", get(handlers::batteries::get_battery).put(handlers::batteries::update_battery).delete(handlers::batteries::retire_battery))
        .route("/api/v1/batteries/:id/tests",
               get(handlers::battery_tests::list_battery_tests)
                   .post(handlers::battery_tests::create_battery_test))
        // Fleet batch import
        .route("/api/v1/fleet/batch-import", post(handlers::batch_import::preview))
        .route("/api/v1/fleet/batch-commit", post(handlers::batch_import::commit))
        // Controllers
        .route("/api/v1/controllers", get(handlers::controllers::list_controllers).post(handlers::controllers::create_controller))
        .route("/api/v1/controllers/:id", get(handlers::controllers::get_controller).put(handlers::controllers::update_controller).delete(handlers::controllers::retire_controller))
        // Accessories
        .route("/api/v1/accessories", get(handlers::accessories::list_accessories).post(handlers::accessories::create_accessory))
        .route("/api/v1/accessories/:id", get(handlers::accessories::get_accessory).put(handlers::accessories::update_accessory).delete(handlers::accessories::retire_accessory))
        // Maintenance
        .route("/api/v1/maintenance", get(handlers::maintenance::list_maintenance).post(handlers::maintenance::create_maintenance))
        .route("/api/v1/maintenance/hub", get(handlers::maintenance_hub::get_maintenance_hub))
        // Maintenance hub v2 — aircraft-centric. Both routes must come BEFORE
        // `/maintenance/:id` so Axum doesn't try to match a literal segment
        // like "aircraft" or "fleet-status" as a maintenance record id.
        .route("/api/v1/maintenance/fleet-status",
               get(handlers::maintenance_aircraft::get_fleet_maintenance_status))
        .route("/api/v1/maintenance/aircraft/:id",
               get(handlers::maintenance_aircraft::get_aircraft_maintenance_detail))
        // Maintenance drafts — same precedence concern, must come before /:id.
        .route(
            "/api/v1/maintenance/drafts/:schedule_id",
            axum::routing::get(handlers::maintenance_drafts::get_draft)
                .put(handlers::maintenance_drafts::upsert_draft)
                .delete(handlers::maintenance_drafts::delete_draft),
        )
        .route("/api/v1/maintenance/:id", get(handlers::maintenance::get_maintenance).put(handlers::maintenance::update_maintenance).delete(handlers::maintenance::delete_maintenance))
        .route("/api/v1/maintenance/:id/detail", get(handlers::maintenance::get_maintenance_detail))
        .route("/api/v1/maintenance/:id/completions",
               post(handlers::maintenance::save_completions))
        // Maintenance schedules
        .route("/api/v1/maintenance-schedules", get(handlers::maintenance_schedules::list_schedules).post(handlers::maintenance_schedules::create_schedule))
        .route("/api/v1/maintenance-schedules/dashboard", get(handlers::maintenance_schedule_actions::schedule_dashboard))
        .route("/api/v1/maintenance-schedules/seed-defaults", post(handlers::maintenance_schedules::seed_defaults))
        .route("/api/v1/maintenance-schedules/:id", get(handlers::maintenance_schedules::get_schedule).put(handlers::maintenance_schedules::update_schedule).delete(handlers::maintenance_schedules::delete_schedule))
        .route("/api/v1/maintenance-schedules/:id/complete", post(handlers::maintenance_schedule_actions::complete_schedule))
        // Maintenance schedule templates (per-model)
        .route("/api/v1/maintenance-schedule-templates", get(handlers::maintenance_schedule_templates::list_templates).post(handlers::maintenance_schedule_templates::create_template))
        .route("/api/v1/maintenance-schedule-templates/:id", get(handlers::maintenance_schedule_templates::get_template).put(handlers::maintenance_schedule_templates::update_template).delete(handlers::maintenance_schedule_templates::delete_template))
        .route("/api/v1/maintenance-schedule-templates/:id/sync", post(handlers::maintenance_schedule_templates::sync_template))
        .route("/api/v1/maintenance-schedule-templates/:id/permanent-delete", post(handlers::maintenance_schedule_templates::permanent_delete))
        // Defects
        .route("/api/v1/defects", get(handlers::defects::list_defects).post(handlers::defects::create_defect))
        .route("/api/v1/defects/:id", put(handlers::defects::update_defect_status).delete(handlers::defects::delete_defect))
        // Pilots
        .route("/api/v1/pilots", get(handlers::pilots::list_pilots).post(handlers::pilots::create_pilot))
        .route("/api/v1/pilots/:id", get(handlers::pilots::get_pilot).put(handlers::pilots::update_pilot).delete(handlers::pilots::delete_pilot))
        .route("/api/v1/pilots/:id/stats", get(handlers::pilot_stats::get_pilot_stats))
        .route("/api/v1/pilots/:id/detail", get(handlers::pilot_detail::get_pilot_detail))
        // Incidents
        .route("/api/v1/occurrence-taxonomy", get(handlers::occurrence_taxonomy::get_taxonomy))
        .route("/api/v1/incidents", get(handlers::incidents::list_incidents).post(handlers::incidents::create_incident))
        .route("/api/v1/incidents/:id", get(handlers::incidents::get_incident).put(handlers::incidents::update_incident).delete(handlers::incidents::delete_incident))
        .route("/api/v1/incidents/:id/attachments", post(handlers::incidents::upload_attachment).layer(DefaultBodyLimit::max(256 * 1024 * 1024)))
        .route("/api/v1/incidents/:id/attachments/:att_id", delete(handlers::incidents::delete_attachment))
        .route("/api/v1/incidents/:id/attachments/:att_id/download", get(handlers::incidents::download_attachment))
        .route("/api/v1/incidents/:id/notes", post(handlers::incidents::add_note))
        // Users
        .route("/api/v1/users", get(handlers::users::list).post(handlers::users::create))
        .route("/api/v1/users/:id", put(handlers::users::update).delete(handlers::users::deactivate))
        .route("/api/v1/users/:id/reactivate", post(handlers::users::reactivate))
        .route("/api/v1/users/:id/reset-token", post(handlers::users::issue_reset_token))
        // Reports (PDF generation)
        .route("/api/v1/reports/flights", get(handlers::reports::flight_log_report))
        .route("/api/v1/reports/maintenance", get(handlers::reports::maintenance_report))
        .route("/api/v1/reports/pilot-logbook", get(handlers::reports::pilot_logbook))
        .route("/api/v1/reports/fleet", get(handlers::reports::fleet_status_report))
        .route("/api/v1/reports/equipment-ids", get(handlers::reports::equipment_id_sheet))
        // Admin purge (destructive — admin role enforced in each handler).
        // The preflight route reports the retention impact a confirmation
        // body must acknowledge (EASA UAS.SPEC.050(1)(g)).
        .route("/api/v1/admin/purge/:target/preflight", get(handlers::purge::purge_preflight))
        .route("/api/v1/admin/purge/flights", delete(handlers::purge::purge_flights))
        .route("/api/v1/admin/purge/aircraft", delete(handlers::purge::purge_aircraft))
        .route("/api/v1/admin/purge/batteries", delete(handlers::purge::purge_batteries))
        .route("/api/v1/admin/purge/controllers", delete(handlers::purge::purge_controllers))
        .route("/api/v1/admin/purge/pilots", delete(handlers::purge::purge_pilots))
        .route("/api/v1/admin/purge/incidents", delete(handlers::purge::purge_incidents))
        // Settings
        .route("/api/v1/settings", get(handlers::settings::get_all_settings))
        .route("/api/v1/settings/:key", get(handlers::settings::get_setting).put(handlers::settings::upsert_setting))
        // Organisation branding (sidebar logo). Read is open to any
        // authenticated role; upload/delete are admin-only (enforced in-handler).
        .route("/api/v1/organisation/branding", get(handlers::organisation::get_branding))
        .route("/api/v1/organisation/logo/:variant",
               post(handlers::organisation::upload_logo)
                   .delete(handlers::organisation::delete_logo))
        // Audit log (read-only, all authenticated roles). Note: /filters must come
        // before /:id so Axum matches the fixed path first.
        .route("/api/v1/audit-log", get(handlers::audit_log::list))
        .route("/api/v1/audit-log/filters", get(handlers::audit_log::filters))
        .route("/api/v1/audit-log/:id", get(handlers::audit_log::get))
        // Fleet statistics (read-only). Fixed-path routes listed before :id routes.
        .route("/api/v1/fleet-stats/overview", get(handlers::fleet_stats::overview))
        .route("/api/v1/fleet-stats/dashboard", get(handlers::fleet_stats::fleet_dashboard))
        .route("/api/v1/fleet-stats/aircraft-by-model", get(handlers::fleet_stats::aircraft_by_model))
        .route("/api/v1/fleet-stats/battery-cycle-distribution", get(handlers::fleet_stats::battery_cycle_distribution))
        .route("/api/v1/fleet-stats/export/batteries", get(handlers::fleet_stats::export_batteries_csv))
        .route("/api/v1/fleet-stats/controllers/export", get(handlers::fleet_stats::export_controllers_csv))
        .route("/api/v1/fleet-stats/batteries/model/:model", get(handlers::fleet_stats::battery_model_comparison))
        .route("/api/v1/fleet-stats/batteries/:id", get(handlers::fleet_stats::battery_detail))
        .route("/api/v1/fleet-stats/controllers/:id", get(handlers::fleet_stats::controller_detail))
        // Data export / import (admin-only, role enforced in each handler)
        .route("/api/v1/data/export/preview", get(handlers::data_transfer::export_preview))
        .route("/api/v1/data/export", post(handlers::data_transfer::export))
        .route("/api/v1/data/import/preview", post(handlers::data_transfer::import_preview))
        .route("/api/v1/data/import", post(handlers::data_transfer::import_apply))
        .route("/api/v1/data/jobs", get(handlers::data_transfer::list_jobs))
        // Health check (public — no auth; used by frontend version badge and load balancers)
        .route("/api/v1/health", get(handlers::health::health));

    // Compose: rate-limited routes + the rest. State + CORS + body-limit
    // apply to both via the merged router.
    let app = main_router
        .merge(rate_limited_auth)
        .with_state(pool)
        .layer(DefaultBodyLimit::max(50 * 1024 * 1024)) // 50 MB
        .layer(cors)
        .layer(axum::middleware::from_fn(
            services::client_ip::capture_client_ip_middleware,
        ))
        .layer(TraceLayer::new_for_http());

    // Start server. `into_make_service_with_connect_info::<SocketAddr>()`
    // is required for both:
    //   - SmartIpKeyExtractor's peer-IP fallback (when X-Forwarded-For is absent)
    //   - the client_ip middleware's ConnectInfo extraction (proxy-trust gate)
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
        .await
        .expect("Failed to bind to port 3000");

    tracing::info!("DroneFleetPro backend listening on 0.0.0.0:3000");
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .expect("Server error");
}
