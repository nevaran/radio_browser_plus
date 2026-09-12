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

        // Ensure parent directory exists with owner-only access
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).await?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    if let Err(err) =
                        fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700)).await
                    {
                        tracing::warn!(path = %parent.display(), error = %err, "Failed to restrict directory permissions");
                    }
                }
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

    /// Number of saved favorites
    pub async fn count(&self) -> usize {
        let cache = self.cache.read().await;
        cache.as_map().len()
    }

    // Private helper to write to disk atomically: write to a temp file, then
    // rename over the target, so a crash can never leave a truncated
    // favorites file behind (which would otherwise load back as empty).
    async fn write_to_file(path: &PathBuf, data: &FavoritesData) -> Result<()> {
        debug!("Persisting favorites to {}", path.display());
        let content = serde_json::to_string_pretty(&data.as_map())?;
        let tmp_path = path.with_extension("json.tmp");
        if let Err(err) = fs::write(&tmp_path, content).await {
            tracing::warn!(path = %tmp_path.display(), error = %err, "Failed to write favorites file");
            return Err(err.into());
        }
        if let Err(err) = fs::rename(&tmp_path, path).await {
            tracing::warn!(path = %path.display(), error = %err, "Failed to persist favorites file");
            let _ = fs::remove_file(&tmp_path).await;
            return Err(err.into());
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
