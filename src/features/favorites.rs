// Favorites feature handlers
use crate::auth::AuthService;
use crate::domain::{Favorite, FavoritesResponse, ToggleFavoriteRequest, UpdateFavoriteRequest};
use crate::error::{AppError, Result};
use crate::infra::FavoritesRepository;
use axum::{extract::Json, http::HeaderMap};
use std::path::PathBuf;
use std::sync::Arc;
use tracing::debug;

#[derive(Clone)]
pub struct FavoritesHandlers {
    base_dir: PathBuf,
    auth: Arc<AuthService>,
}

impl FavoritesHandlers {
    pub fn new(base_dir: impl Into<PathBuf>, auth: Arc<AuthService>) -> Self {
        Self {
            base_dir: base_dir.into(),
            auth,
        }
    }

    async fn repo_for_headers(&self, headers: &HeaderMap) -> Result<FavoritesRepository> {
        let session_id = AuthService::extract_session_id(headers)
            .ok_or_else(|| AppError::Unauthorized("Not authenticated".to_string()))?;
        let user = self
            .auth
            .current_user_from_session(&session_id)
            .ok_or_else(|| AppError::Unauthorized("Session expired".to_string()))?;

        let path = self
            .base_dir
            .join(format!("user-{}", user.username))
            .join("favorites.json");
        let repo = FavoritesRepository::new(path).await?;
        Ok(repo)
    }

    pub async fn saved_favorite(
        &self,
        headers: &HeaderMap,
        station_id: &str,
    ) -> Result<Option<Favorite>> {
        let repo = match self.repo_for_headers(headers).await {
            Ok(repo) => repo,
            Err(AppError::Unauthorized(_)) => return Ok(None),
            Err(err) => return Err(err),
        };
        Ok(repo.load_all().await?.as_map().get(station_id).cloned())
    }

    /// GET /api/favorites - Get all favorites for the current user
    pub async fn list(&self, headers: HeaderMap) -> Result<axum::Json<FavoritesResponse>> {
        debug!("Listing favorites for authenticated user");
        let repo = self.repo_for_headers(&headers).await?;
        let data = repo.load_all().await?;
        Ok(axum::Json(FavoritesResponse {
            favorites: data.as_map().clone(),
        }))
    }

    /// Max saved favorites per user: bounds the size of each favorites file
    /// and the cost of loading it on every favorites request.
    const MAX_FAVORITES_PER_USER: usize = 2000;

    fn check_favorite(favorite: &Favorite) -> Result<()> {
        for raw_url in [
            favorite.url.as_deref(),
            favorite.url_resolved.as_deref(),
            favorite.favicon.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            if raw_url.trim().is_empty() {
                continue;
            }
            let url = reqwest::Url::parse(raw_url)
                .map_err(|_| AppError::BadRequest("Favorite URL is invalid".to_string()))?;
            if !matches!(url.scheme(), "http" | "https")
                || url.username() != ""
                || url.password().is_some()
            {
                return Err(AppError::BadRequest(
                    "Favorite URLs must be HTTP(S) URLs without credentials".to_string(),
                ));
            }
        }
        let invalid = favorite.station_id.is_empty()
            || favorite.station_id.len() > 128
            || favorite.name.chars().count() > 500
            || favorite.url.as_deref().is_some_and(|u| u.len() > 4096)
            || favorite
                .url_resolved
                .as_deref()
                .is_some_and(|u| u.len() > 4096)
            || favorite.favicon.as_deref().is_some_and(|u| u.len() > 4096)
            || favorite
                .country
                .as_deref()
                .is_some_and(|c| c.chars().count() > 200)
            || favorite
                .countrycode
                .as_deref()
                .is_some_and(|code| code.len() > 16)
            || favorite
                .genre
                .as_deref()
                .is_some_and(|g| g.chars().count() > 500)
            || favorite
                .tags
                .as_deref()
                .is_some_and(|t| t.chars().count() > 2000);
        if invalid {
            return Err(AppError::BadRequest(
                "Favorite payload is invalid".to_string(),
            ));
        }
        Ok(())
    }

    /// POST /api/favorites/toggle - Toggle favorite
    pub async fn toggle(
        &self,
        headers: HeaderMap,
        Json(payload): Json<ToggleFavoriteRequest>,
    ) -> Result<axum::Json<FavoritesResponse>> {
        debug!("Toggling favorite for station: {}", payload.station_id);
        let favorite = Favorite::from(payload);
        Self::check_favorite(&favorite)?;
        let repo = self.repo_for_headers(&headers).await?;

        if !repo.is_favorite(&favorite.station_id).await
            && repo.count().await >= Self::MAX_FAVORITES_PER_USER
        {
            return Err(AppError::BadRequest("Too many favorites".to_string()));
        }

        let data = repo.toggle(favorite).await?;
        Ok(axum::Json(FavoritesResponse {
            favorites: data.as_map().clone(),
        }))
    }

    /// POST /api/favorites/update - Update favorite metadata
    pub async fn update(
        &self,
        headers: HeaderMap,
        Json(payload): Json<UpdateFavoriteRequest>,
    ) -> Result<axum::Json<FavoritesResponse>> {
        debug!("Updating favorite for station: {}", payload.station_id);
        let favorite = Favorite::from(payload);
        Self::check_favorite(&favorite)?;
        let repo = self.repo_for_headers(&headers).await?;

        let station_id = favorite.station_id.clone();
        let data = repo.update(&station_id, favorite).await?;
        Ok(axum::Json(FavoritesResponse {
            favorites: data.as_map().clone(),
        }))
    }
}
