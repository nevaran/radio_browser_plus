// Main application entrypoint and HTTP routing setup
use axum::{
    extract::{Json, Path, Query, Request, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{any, get, post},
    Router,
};
use serde_json::json;
use std::env;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tower_http::{
    limit::RequestBodyLimitLayer,
    services::{ServeDir, ServeFile},
    set_header::SetResponseHeaderLayer,
    timeout::TimeoutLayer,
};
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

use radio_browser_plus::{
    auth::{AuthService, ChangePasswordRequest, CreateUserRequest, HeartbeatRequest, LoginRequest},
    domain::{
        Country, FavoritesResponse, HealthResponse, Language, Station, Tag, ToggleFavoriteRequest,
        UpdateFavoriteRequest,
    },
    error::Result,
    features::{
        stations::{PaginationParams, SearchParams, StationsQueryParams},
        CollectionsHandlers, FavoritesHandlers, HealthHandlers, StationsHandlers,
    },
    infra::RadioBrowserClient,
};

#[tokio::main]
async fn main() {
    // Initialize tracing/logging
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(false)
        .with_writer(std::io::stdout)
        .init();

    info!("Starting Radio Browser Plus server");

    if !std::path::Path::new("dist/index.html").exists() {
        tracing::warn!(
            "dist/index.html not found - build the Leptos frontend first \
             (cd frontend && trunk build --release) or run it via `trunk serve`"
        );
    }

    // Runtime configuration
    let data_dir = env::var("DATA_DIR").unwrap_or_else(|_| "data".to_string());
    info!(data_dir = %data_dir, "Using data directory");
    let allow_guest = allow_guest_access();
    let admin_stats_enabled = admin_stats_enabled();
    let log_retention_days = log_retention_days();
    if allow_guest {
        tracing::warn!("ALLOW_GUEST_ACCESS is enabled: the app can be browsed without logging in");
    } else {
        info!("Guest access disabled: sign-in required");
    }
    if admin_stats_enabled {
        tracing::info!(
            "ADMIN_STATS_ENABLED is on: active user/guest statistics are exposed to admins"
        );
    }
    if log_retention_days > 0 {
        tracing::info!(
            days = log_retention_days,
            "Activity logs retained for {} days",
            log_retention_days
        );
    }

    // Initialize infrastructure
    let radio_client = RadioBrowserClient::new(None);
    let auth = Arc::new(AuthService::new(&data_dir));
    auth.ensure_default_admin("admin", "admin");
    if auth.admin_uses_default_password("admin", "admin") {
        tracing::warn!(
            "The 'admin' account still uses the default password - \
             sign in and change it immediately, especially when exposed \
             through a reverse proxy"
        );
    }

    // Initialize handlers
    let stations = Arc::new(StationsHandlers::new(radio_client.clone()));
    let favorites = Arc::new(FavoritesHandlers::new(&data_dir, auth.clone()));
    let collections = Arc::new(CollectionsHandlers::new(radio_client));
    let health = Arc::new(HealthHandlers::new());

    let app = build_router(allow_guest, auth, stations, favorites, collections, health);

    // Start server
    let addr = SocketAddr::from(([0, 0, 0, 0], 8000));
    let listener = TcpListener::bind(&addr).await.unwrap_or_else(|err| {
        error!("Failed to bind to {}: {}", addr, err);
        std::process::exit(1);
    });

    info!("Server listening on http://{}", addr);

    axum::serve(listener, app).await.unwrap_or_else(|err| {
        error!("Server error: {}", err);
        std::process::exit(1);
    });
}

/// Assemble the complete application: public health endpoint, the /api
/// router (behind the auth middleware), hardened security layers, and the
/// SPA fallback.
fn build_router(
    allow_guest: bool,
    auth: Arc<AuthService>,
    stations: Arc<StationsHandlers>,
    favorites: Arc<FavoritesHandlers>,
    collections: Arc<CollectionsHandlers>,
    health: Arc<HealthHandlers>,
) -> Router {
    let api = Router::new()
        .merge(station_routes(stations))
        .merge(collection_routes(collections))
        .merge(auth_routes(auth.clone()))
        .merge(favorite_routes(favorites))
        .merge(config_routes(allow_guest))
        // Unknown API paths must not fall through to the SPA shell: answer
        // with JSON 404 so API clients (and scanners) get a clear signal.
        // (Also behind the auth layer below, like every other /api route.)
        .route("/api/{*rest}", any(api_not_found))
        .route_layer(middleware::from_fn_with_state(
            ApiAuth {
                auth: auth.clone(),
                allow_guest,
            },
            require_auth,
        ));

    Router::new()
        // Health check stays public so reverse proxies and orchestrators can
        // probe liveness without credentials.
        .merge(health_routes(health))
        .merge(api)
        .fallback_service(ServeDir::new("dist").fallback(ServeFile::new("dist/index.html")))
        // Request hardening. Layers apply inside-out, so response headers are
        // set last and cover every response, including rejections below.
        .layer(RequestBodyLimitLayer::new(64 * 1024))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(30),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::REFERRER_POLICY,
            HeaderValue::from_static("no-referrer"),
        ))
        // Note: `script-src`/`style-src` fall back to `default-src` when absent,
        // so they must be listed explicitly. Trunk boots the app via an inline
        // module script and the UI compiles WebAssembly at load, hence
        // `'unsafe-inline'` and `'wasm-unsafe-eval'`; the UI also relies on
        // inline style attributes. `frame-ancestors 'none'` still blocks
        // clickjacking via framing.
        .layer(SetResponseHeaderLayer::if_not_present(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static(
                "default-src 'self'; base-uri 'self'; form-action 'self'; \
                 frame-ancestors 'none'; script-src 'self' 'unsafe-inline' \
                 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'; \
                 img-src 'self' data: https:; media-src http: https: data: blob:; \
                 connect-src 'self'; font-src 'self'; object-src 'none'",
            ),
        ))
}

// ---------------------------------------------------------------------------
// Route wiring
//
// Every service lives behind an `Arc` and is passed as axum `State`, so
// routes wire up one service each without the numbered `.clone()` plumbing
// that per-route closures used to need.
// ---------------------------------------------------------------------------

/// GET endpoints for stations (search/listings).
fn station_routes(handlers: Arc<StationsHandlers>) -> Router {
    Router::new()
        .route("/api/stations", get(get_stations))
        .route("/api/popular", get(list_popular))
        .route("/api/search", get(search_stations))
        .route("/api/station/{id}", get(get_station_by_id))
        .with_state(handlers)
}

/// GET endpoints for the country/language/tag/genre collections.
fn collection_routes(handlers: Arc<CollectionsHandlers>) -> Router {
    Router::new()
        .route("/api/countries", get(list_countries))
        .route("/api/languages", get(list_languages))
        .route("/api/tags", get(list_tags))
        .route("/api/genres", get(list_genres))
        .with_state(handlers)
}

/// Session and account endpoints (plus the admin activity stats feed).
fn auth_routes(auth: Arc<AuthService>) -> Router {
    Router::new()
        .route("/api/login", post(login))
        .route("/api/logout", post(logout))
        .route("/api/me", get(current_user))
        .route("/api/users", post(create_user))
        .route("/api/change-password", post(change_password))
        .route("/api/admin/stats", get(admin_stats))
        .route("/api/heartbeat", post(heartbeat))
        .with_state(auth)
}

/// Per-user favorite CRUD endpoints.
fn favorite_routes(handlers: Arc<FavoritesHandlers>) -> Router {
    Router::new()
        .route("/api/favorites", get(list_favorites))
        .route("/api/favorites/toggle", post(toggle_favorite))
        .route("/api/favorites/update", post(update_favorite))
        .with_state(handlers)
}

/// Public runtime config (guest mode flag) so the UI knows whether
/// browsing without a session is allowed. Stays behind the auth layer
/// like every /api route, but allowlisted inside it.
fn config_routes(allow_guest: bool) -> Router {
    Router::new()
        .route("/api/config", get(runtime_config))
        .with_state(allow_guest)
}

/// Liveness endpoint, served outside the auth layer.
fn health_routes(handlers: Arc<HealthHandlers>) -> Router {
    Router::new()
        .route("/health", get(health_check))
        .with_state(handlers)
}

// ---------------------------------------------------------------------------
// Handlers
//
// Each wrapper announces its extractors (so axum's ordering rules are checked
// in one obvious place) and delegates to the service method.
// ---------------------------------------------------------------------------

async fn get_stations(
    State(s): State<Arc<StationsHandlers>>,
    params: Query<StationsQueryParams>,
) -> Result<Json<Vec<Station>>> {
    s.get_stations(params).await
}

async fn list_popular(
    State(s): State<Arc<StationsHandlers>>,
    params: Query<PaginationParams>,
) -> Result<Json<Vec<Station>>> {
    s.list_popular(params).await
}

async fn search_stations(
    State(s): State<Arc<StationsHandlers>>,
    params: Query<SearchParams>,
) -> Result<Json<Vec<Station>>> {
    s.search(params).await
}

async fn get_station_by_id(
    State(s): State<Arc<StationsHandlers>>,
    Path(id): Path<String>,
) -> Result<Json<Station>> {
    s.get_by_id(id).await
}

async fn list_countries(State(c): State<Arc<CollectionsHandlers>>) -> Result<Json<Vec<Country>>> {
    c.countries().await
}

async fn list_languages(State(c): State<Arc<CollectionsHandlers>>) -> Result<Json<Vec<Language>>> {
    c.languages().await
}

async fn list_tags(State(c): State<Arc<CollectionsHandlers>>) -> Result<Json<Vec<Tag>>> {
    c.tags().await
}

async fn list_genres(State(c): State<Arc<CollectionsHandlers>>) -> Result<Json<Vec<Tag>>> {
    c.genres().await
}

async fn login(
    State(a): State<Arc<AuthService>>,
    headers: HeaderMap,
    payload: Json<LoginRequest>,
) -> Response {
    a.login_response(headers, payload)
}

async fn logout(State(a): State<Arc<AuthService>>, headers: HeaderMap) -> Response {
    a.logout_response(headers)
}

async fn current_user(State(a): State<Arc<AuthService>>, headers: HeaderMap) -> Response {
    a.me_response(headers)
}

async fn create_user(
    State(a): State<Arc<AuthService>>,
    headers: HeaderMap,
    payload: Json<CreateUserRequest>,
) -> Response {
    a.create_user_response(headers, payload)
}

async fn change_password(
    State(a): State<Arc<AuthService>>,
    headers: HeaderMap,
    payload: Json<ChangePasswordRequest>,
) -> Response {
    a.change_password_response(headers, payload)
}

async fn admin_stats(State(a): State<Arc<AuthService>>, headers: HeaderMap) -> Response {
    a.admin_stats_response(headers)
}

async fn heartbeat(
    State(a): State<Arc<AuthService>>,
    headers: HeaderMap,
    payload: Json<HeartbeatRequest>,
) -> Response {
    a.heartbeat_response(headers, payload)
}

async fn list_favorites(
    State(f): State<Arc<FavoritesHandlers>>,
    headers: HeaderMap,
) -> Result<Json<FavoritesResponse>> {
    f.list(headers).await
}

async fn toggle_favorite(
    State(f): State<Arc<FavoritesHandlers>>,
    headers: HeaderMap,
    body: Json<ToggleFavoriteRequest>,
) -> Result<Json<FavoritesResponse>> {
    f.toggle(headers, body).await
}

async fn update_favorite(
    State(f): State<Arc<FavoritesHandlers>>,
    headers: HeaderMap,
    body: Json<UpdateFavoriteRequest>,
) -> Result<Json<FavoritesResponse>> {
    f.update(headers, body).await
}

async fn runtime_config(State(allow_guest): State<bool>) -> impl IntoResponse {
    Json(json!({ "allow_guest": allow_guest }))
}

async fn health_check(State(h): State<Arc<HealthHandlers>>) -> Result<Json<HealthResponse>> {
    h.check().await
}

async fn api_not_found() -> impl IntoResponse {
    (StatusCode::NOT_FOUND, Json(json!({ "error": "Not found" })))
}

// Whether anonymous browsing is allowed. Opt-in via ALLOW_GUEST_ACCESS=1
// (also accepts true/yes/on); anything else, including unset, means login is
// required. Parsed separately for unit testing without touching the env.
fn parse_guest_flag(raw: Option<&str>) -> bool {
    raw.is_some_and(|v| {
        matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}

fn allow_guest_access() -> bool {
    parse_guest_flag(env::var("ALLOW_GUEST_ACCESS").ok().as_deref())
}

fn parse_admin_stats_flag(raw: Option<&str>) -> bool {
    parse_guest_flag(raw)
}

fn admin_stats_enabled() -> bool {
    parse_admin_stats_flag(env::var("ADMIN_STATS_ENABLED").ok().as_deref())
}

fn parse_log_retention_days(raw: Option<&str>) -> u64 {
    raw.and_then(|value| value.trim().parse::<u64>().ok())
        .unwrap_or(0)
}

fn log_retention_days() -> u64 {
    parse_log_retention_days(env::var("LOG_RETENTION_DAYS").ok().as_deref())
}

/// Auth state shared with the API middleware.
#[derive(Clone)]
struct ApiAuth {
    auth: Arc<AuthService>,
    allow_guest: bool,
}

// API routes that work without a session: sign-in itself, idempotent
// sign-out, and the public runtime config. Everything else under /api
// requires authentication unless guest access is enabled, in which case the
// UI browses anonymously (per-user features like favorites still need login).
fn is_public_api_path(path: &str) -> bool {
    matches!(path, "/api/login" | "/api/logout" | "/api/config")
}

async fn require_auth(
    State(api_auth): State<ApiAuth>,
    headers: HeaderMap,
    req: Request,
    next: Next,
) -> Response {
    if api_auth.allow_guest || is_public_api_path(req.uri().path()) {
        return next.run(req).await;
    }
    let authorized = AuthService::extract_session_id(&headers)
        .and_then(|id| api_auth.auth.current_user_from_session(&id))
        .is_some();
    if !authorized {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "Not authenticated" })),
        )
            .into_response();
    }
    next.run(req).await
}

#[cfg(test)]
mod tests {
    use super::{
        is_public_api_path, parse_admin_stats_flag, parse_guest_flag, parse_log_retention_days,
    };

    #[test]
    fn guest_flag_defaults_to_off() {
        for raw in [
            None,
            Some(""),
            Some("0"),
            Some("false"),
            Some("no"),
            Some("off"),
            Some("2"),
        ] {
            assert!(!parse_guest_flag(raw), "{raw:?} must keep login required");
        }
        for raw in ["1", "true", "TRUE", "yes", "Yes", "on", "ON"] {
            assert!(parse_guest_flag(Some(raw)), "{raw:?} must allow guests");
        }
        assert!(parse_guest_flag(Some(" true ")));
        assert!(parse_guest_flag(Some(" YES ")));
    }

    #[test]
    fn admin_stats_flag_and_log_retention_days_parse_cleanly() {
        assert!(parse_admin_stats_flag(Some("1")));
        assert!(parse_admin_stats_flag(Some(" true ")));
        assert!(!parse_admin_stats_flag(Some("0")));
        assert_eq!(parse_log_retention_days(Some("7")), 7);
        assert_eq!(parse_log_retention_days(Some(" 7 ")), 7);
        assert_eq!(parse_log_retention_days(None), 0);
        assert_eq!(parse_log_retention_days(Some("0")), 0);
        assert_eq!(parse_log_retention_days(Some("30")), 30);
    }

    #[test]
    fn only_login_logout_and_config_are_public() {
        assert!(is_public_api_path("/api/login"));
        assert!(is_public_api_path("/api/logout"));
        assert!(is_public_api_path("/api/config"));
        for path in [
            "/api/me",
            "/api/users",
            "/api/stations",
            "/api/popular",
            "/api/search",
            "/api/station/x",
            "/api/countries",
            "/api/favorites",
            "/api/Login",
            "/api/login/",
            "/api/login/extra",
            "/health",
            "/",
        ] {
            assert!(!is_public_api_path(path), "{path} must require auth");
        }
    }
}
