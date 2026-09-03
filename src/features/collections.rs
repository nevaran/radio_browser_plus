// Collections feature handlers (countries, languages, tags)
use crate::domain::{Country, Language, Tag};
use crate::error::Result;
use crate::infra::RadioBrowserClient;
use axum::Json;
use tracing::debug;

#[derive(Clone)]
pub struct CollectionsHandlers {
    client: RadioBrowserClient,
}

impl CollectionsHandlers {
    pub fn new(client: RadioBrowserClient) -> Self {
        Self { client }
    }

    /// GET /api/countries - Get all countries
    pub async fn countries(&self) -> Result<Json<Vec<Country>>> {
        debug!("Fetching countries");
        let countries = self.client.get_countries().await?;
        Ok(Json(countries))
    }

    /// GET /api/languages - Get all languages
    pub async fn languages(&self) -> Result<Json<Vec<Language>>> {
        debug!("Fetching languages");
        let languages = self.client.get_languages().await?;
        Ok(Json(languages))
    }

    /// GET /api/tags - Get all tags/genres
    pub async fn tags(&self) -> Result<Json<Vec<Tag>>> {
        debug!("Fetching tags");
        let tags = self.client.get_tags().await?;
        Ok(Json(tags))
    }
}
