// Feature modules
pub mod collections;
pub mod failover;
pub mod favorites;
pub mod health;
pub mod stations;

pub use collections::CollectionsHandlers;
pub use failover::FailoverHandlers;
pub use favorites::FavoritesHandlers;
pub use health::HealthHandlers;
pub use stations::StationsHandlers;
