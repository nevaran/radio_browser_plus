// Favorites repository - persists to JSON file
use crate::domain::{Favorite, FavoritesData};
use crate::error::Result;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs;
use tokio::sync::RwLock;
use tracing::debug;

pub struct FavoritesRepository {
    path: PathBuf,
    /// In-memory cache to avoid constant disk reads
    cache: Arc<RwLock<FavoritesData>>,
}

impl FavoritesRepository {
    /// Create a new repository with file path
    pub async fn new(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).await?;
            }
        }

        let mut data = FavoritesData::new();

        // Load existing file if it exists
        if path.exists() {
            debug!("Loading favorites from {}", path.display());
            let content = fs::read_to_string(&path).await?;
            data = serde_json::from_str(&content).unwrap_or_default();
        } else {
            debug!("Creating new favorites file at {}", path.display());
            Self::write_to_file(&path, &data).await?;
        }

        Ok(Self {
            path,
            cache: Arc::new(RwLock::new(data)),
        })
    }

    /// Load all favorites
    pub async fn load_all(&self) -> Result<FavoritesData> {
        let cache = self.cache.read().await;
        Ok(cache.clone())
    }

    /// Toggle a favorite (add if not present, remove if present)
    pub async fn toggle(&self, favorite: Favorite) -> Result<FavoritesData> {
        let mut cache = self.cache.write().await;
        cache.toggle(favorite);
        Self::write_to_file(&self.path, &cache).await?;
        Ok(cache.clone())
    }

    /// Update a favorite's metadata
    pub async fn update(&self, station_id: &str, favorite: Favorite) -> Result<FavoritesData> {
        let mut cache = self.cache.write().await;
        cache.update(station_id, favorite);
        Self::write_to_file(&self.path, &cache).await?;
        Ok(cache.clone())
    }

    /// Check if a station is favorited
    pub async fn is_favorite(&self, station_id: &str) -> bool {
        let cache = self.cache.read().await;
        cache.is_favorite(station_id)
    }

    /// Get a specific favorite
    pub async fn get(&self, station_id: &str) -> Result<Option<Favorite>> {
        let cache = self.cache.read().await;
        Ok(cache.get(station_id).cloned())
    }

    // Private helper to write to disk
    async fn write_to_file(path: &PathBuf, data: &FavoritesData) -> Result<()> {
        debug!("Persisting favorites to {}", path.display());
        let content = serde_json::to_string_pretty(&data.as_map())?;
        fs::write(path, content).await?;
        Ok(())
    }

    /// Reload from disk (useful after external changes)
    pub async fn reload(&self) -> Result<()> {
        if self.path.exists() {
            debug!("Reloading favorites from disk");
            let content = fs::read_to_string(&self.path).await?;
            let data: FavoritesData = serde_json::from_str(&content).unwrap_or_default();
            let mut cache = self.cache.write().await;
            *cache = data;
        }
        Ok(())
    }
}

impl Clone for FavoritesRepository {
    fn clone(&self) -> Self {
        Self {
            path: self.path.clone(),
            cache: self.cache.clone(),
        }
    }
}
