// Main application entrypoint and HTTP routing setup
use axum::{
    extract::{Json, Path, Request, State},
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
    auth::{AuthService, ChangePasswordRequest, CreateUserRequest, LoginRequest},
    features::{CollectionsHandlers, FavoritesHandlers, HealthHandlers, StationsHandlers},
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

    // Initialize infrastructure
    let radio_client = RadioBrowserClient::new(None);
    let data_dir = env::var("DATA_DIR").unwrap_or_else(|_| "data".to_string());
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

    // Clones for router
    let stations_1 = stations.clone();
    let stations_2 = stations.clone();
    let stations_3 = stations.clone();
    let stations_4 = stations.clone();

    let favorites_1 = favorites.clone();
    let favorites_2 = favorites.clone();
    let favorites_3 = favorites.clone();

    let collections_1 = collections.clone();
    let collections_2 = collections.clone();
    let collections_3 = collections.clone();

    let health_1 = health.clone();

    let auth_login = auth.clone();
    let auth_logout = auth.clone();
    let auth_me = auth.clone();
    let auth_users = auth.clone();
    let auth_change_password = auth.clone();

    // Build router
    let api = Router::new()
        // Stations endpoints
        .route(
            "/api/stations",
            get(move |params| {
                let s = stations_1.clone();
                async move {
                    s.get_stations(params).await
                }
            }),
        )
        .route(
            "/api/popular",
            get(move |params| {
                let s = stations_2.clone();
                async move { s.list_popular(params).await }
            }),
        )
        .route(
            "/api/search",
            get(move |params| {
                let s = stations_3.clone();
                async move { s.search(params).await }
            }),
        )
        .route(
            "/api/station/{id}",
            get(move |Path(id)| {
                let s = stations_4.clone();
                async move { s.get_by_id(id).await }
            }),
        )
        // Collections endpoints
        .route(
            "/api/countries",
            get(move || {
                let c = collections_1.clone();
                async move { c.countries().await }
            }),
        )
        .route(
            "/api/languages",
            get(move || {
                let c = collections_2.clone();
                async move { c.languages().await }
            }),
        )
        .route(
            "/api/tags",
            get(move || {
                let c = collections_3.clone();
                async move { c.tags().await }
            }),
        )
        // Auth endpoints
        .route(
            "/api/login",
            post(move |headers: HeaderMap, payload: Json<LoginRequest>| {
                let a = auth_login.clone();
                async move { a.login_response(headers, payload) }
            }),
        )
        .route(
            "/api/logout",
            post(move |headers: HeaderMap| {
                let a = auth_logout.clone();
                async move { a.logout_response(headers) }
            }),
        )
        .route(
            "/api/me",
            get(move |headers: HeaderMap| {
                let a = auth_me.clone();
                async move { a.me_response(headers) }
            }),
        )
        .route(
            "/api/users",
            post(move |headers: HeaderMap, payload: Json<CreateUserRequest>| {
                let a = auth_users.clone();
                async move { a.create_user_response(headers, payload) }
            }),
        )
        .route(
            "/api/change-password",
            post(move |headers: HeaderMap, payload: Json<ChangePasswordRequest>| {
                let a = auth_change_password.clone();
                async move { a.change_password_response(headers, payload) }
            }),
        )
        // Favorites endpoints
        .route(
            "/api/favorites",
            get(move |headers: HeaderMap| {
                let f = favorites_1.clone();
                async move { f.list(headers).await }
            }),
        )
        .route(
            "/api/favorites/toggle",
            post(move |headers: HeaderMap, body: Json<radio_browser_plus::domain::ToggleFavoriteRequest>| {
                let f = favorites_2.clone();
                async move { f.toggle(headers, body).await }
            }),
        )
        .route(
            "/api/favorites/update",
            post(move |headers: HeaderMap, body: Json<radio_browser_plus::domain::UpdateFavoriteRequest>| {
                let f = favorites_3.clone();
                async move { f.update(headers, body).await }
            }),
        )
        // Unknown API paths must not fall through to the SPA shell: answer
        // with JSON 404 so API clients (and scanners) get a clear signal.
        // (Also behind the auth layer below, like every other /api route.)
        .route(
            "/api/{*rest}",
            any(|| async {
                (
                    StatusCode::NOT_FOUND,
                    Json(json!({ "error": "Not found" })),
                )
            }),
        )
        .route_layer(middleware::from_fn_with_state(
            auth.clone(),
            require_auth,
        ));

    // Build router
    let app = Router::new()
        // Health check stays public so reverse proxies and orchestrators can
        // probe liveness without credentials.
        .route(
            "/health",
            get(move || {
                let h = health_1.clone();
                async move { h.check().await }
            }),
        )
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
        ));

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

// API routes that work without a session: sign-in itself and idempotent
// sign-out. Everything else under /api requires authentication, so the app
// cannot be used without logging in.
fn is_public_api_path(path: &str) -> bool {
    matches!(path, "/api/login" | "/api/logout")
}

async fn require_auth(
    State(auth): State<Arc<AuthService>>,
    headers: HeaderMap,
    req: Request,
    next: Next,
) -> Response {
    if is_public_api_path(req.uri().path()) {
        return next.run(req).await;
    }
    let authorized = AuthService::extract_session_id(&headers)
        .and_then(|id| auth.current_user_from_session(&id))
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
    use super::is_public_api_path;

    #[test]
    fn only_login_and_logout_are_public() {
        assert!(is_public_api_path("/api/login"));
        assert!(is_public_api_path("/api/logout"));
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
