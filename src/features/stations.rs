// Stations feature handlers
use crate::domain::Station;
use crate::error::Result;
use crate::infra::RadioBrowserClient;
use axum::{extract::Query, Json};
use serde::Deserialize;
use tracing::debug;

#[derive(Clone)]
pub struct StationsHandlers {
    client: RadioBrowserClient,
    default_limit: u32,
}

impl StationsHandlers {
    pub fn new(client: RadioBrowserClient) -> Self {
        let default_limit = std::env::var("RADIO_BROWSER_STATION_LIMIT")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(1000);

        Self {
            client,
            default_limit,
        }
    }

    /// GET /api/stations - Get all stations
    pub async fn list_all(&self, params: Query<PaginationParams>) -> Result<Json<Vec<Station>>> {
        debug!("Listing all stations");
        let limit = params.limit.unwrap_or(self.default_limit);
        let stations = self.client.get_all_stations(limit).await?;
        Ok(Json(stations))
    }

    /// GET /api/popular - Get popular stations
    pub async fn list_popular(
        &self,
        params: Query<PaginationParams>,
    ) -> Result<Json<Vec<Station>>> {
        debug!("Listing popular stations");
        let limit = params.limit.unwrap_or(self.default_limit);
        let stations = self.client.get_popular_stations(limit).await?;
        Ok(Json(stations))
    }

    /// GET /api/search - Search stations
    pub async fn search(
        &self,
        Query(params): Query<SearchParams>,
    ) -> Result<Json<Vec<Station>>> {
        if params.q.chars().count() > 200 {
            return Err(crate::error::AppError::BadRequest("Search query is too long".to_string()));
        }
        debug!("Searching stations with query: {}", params.q);
        let limit = params.limit.unwrap_or(self.default_limit);
        let stations = self.client.search_stations(&params.q, limit).await?;
        Ok(Json(stations))
    }

    /// GET /api/station/:id - Get station by ID
    pub async fn get_by_id(&self, station_id: String) -> Result<Json<Station>> {
        debug!("Getting station by id: {}", station_id);
        let station = self.client.get_station_by_id(&station_id).await?;
        Ok(Json(station))
    }

    /// GET /api/stations?country=... - Get stations by country
    pub async fn by_country(
        &self,
        Query(params): Query<CountryFilterParams>,
    ) -> Result<Json<Vec<Station>>> {
        if let Some(country) = params.country {
            debug!("Getting stations by country: {}", country);
            let limit = params.limit.unwrap_or(self.default_limit);
            let stations = self.client.get_stations_by_country(&country, limit).await?;
            return Ok(Json(stations));
        }
        Ok(Json(Vec::new()))
    }

    /// GET /api/stations?language=... - Get stations by language
    pub async fn by_language(
        &self,
        Query(params): Query<LanguageFilterParams>,
    ) -> Result<Json<Vec<Station>>> {
        if let Some(language) = params.language {
            debug!("Getting stations by language: {}", language);
            let limit = params.limit.unwrap_or(self.default_limit);
            let stations = self.client.get_stations_by_language(&language, limit).await?;
            return Ok(Json(stations));
        }
        Ok(Json(Vec::new()))
    }

    /// GET /api/stations?tag=... - Get stations by tag
    pub async fn by_tag(&self, Query(params): Query<TagFilterParams>) -> Result<Json<Vec<Station>>> {
        if let Some(tag) = params.tag {
            debug!("Getting stations by tag: {}", tag);
            let limit = params.limit.unwrap_or(self.default_limit);
            let stations = self.client.get_stations_by_tag(&tag, limit).await?;
            return Ok(Json(stations));
        }
        Ok(Json(Vec::new()))
    }

    /// Unified handler for all /api/stations filters
    pub async fn get_stations(
        &self,
        params: Query<StationsQueryParams>,
    ) -> Result<Json<Vec<Station>>> {
        let limit = params.limit.unwrap_or(self.default_limit);
        
        if let Some(country) = &params.country {
            debug!("Getting stations by country: {}", country);
            return self.client.get_stations_by_country(country, limit).await.map(Json);
        }
        
        if let Some(language) = &params.language {
            debug!("Getting stations by language: {}", language);
            return self.client.get_stations_by_language(language, limit).await.map(Json);
        }
        
        if let Some(tag) = &params.tag {
            debug!("Getting stations by tag: {}", tag);
            return self.client.get_stations_by_tag(tag, limit).await.map(Json);
        }
        
        debug!("Listing all stations");
        self.client.get_all_stations(limit).await.map(Json)
    }
}

#[derive(Deserialize)]
pub struct StationsQueryParams {
    pub country: Option<String>,
    pub language: Option<String>,
    pub tag: Option<String>,
    pub limit: Option<u32>,
}

#[derive(Deserialize)]
pub struct PaginationParams {
    pub limit: Option<u32>,
}

#[derive(Deserialize)]
pub struct SearchParams {
    pub q: String,
    pub limit: Option<u32>,
}

#[derive(Deserialize)]
pub struct CountryFilterParams {
    pub country: Option<String>,
    pub limit: Option<u32>,
}

#[derive(Deserialize)]
pub struct LanguageFilterParams {
    pub language: Option<String>,
    pub limit: Option<u32>,
}

#[derive(Deserialize)]
pub struct TagFilterParams {
    pub tag: Option<String>,
    pub limit: Option<u32>,
}
