// Infrastructure layer - external services and storage
pub mod radio_browser_client;
pub mod favorites_repository;

pub use radio_browser_client::RadioBrowserClient;
pub use favorites_repository::FavoritesRepository;
