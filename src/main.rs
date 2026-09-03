// Main application entrypoint and HTTP routing setup
use axum::{
    extract::{Json, Path},
    http::HeaderMap,
    routing::{get, post},
    Router,
};
use std::net::SocketAddr;
use std::sync::Arc;
use std::env;
use tokio::net::TcpListener;
use tower_http::services::{ServeDir, ServeFile};
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
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(false)
        .with_writer(std::io::stdout)
        .init();

    info!("Starting Radio Browser Plus server");

    // Initialize infrastructure
    let radio_client = RadioBrowserClient::new(None);
    let data_dir = env::var("DATA_DIR").unwrap_or_else(|_| "data".to_string());
    let auth = Arc::new(AuthService::new(&data_dir));
    auth.ensure_default_admin("admin", "admin");

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
    let _stations_5 = stations.clone();

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
    let app = Router::new()
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
        // Health check
        .route(
            "/health",
            get(move || {
                let h = health_1.clone();
                async move { h.check().await }
            }),
        )
        .fallback_service(
            ServeDir::new("public")
                .fallback(ServeFile::new("public/index.html"))
        );

    // Start server
    let addr = SocketAddr::from(([0, 0, 0, 0], 8000));
    let listener = TcpListener::bind(&addr)
        .await
        .unwrap_or_else(|err| {
            error!("Failed to bind to {}: {}", addr, err);
            std::process::exit(1);
        });

    info!("Server listening on http://{}", addr);

    axum::serve(listener, app)
        .await
        .unwrap_or_else(|err| {
            error!("Server error: {}", err);
            std::process::exit(1);
        });
}
