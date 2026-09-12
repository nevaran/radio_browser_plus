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

/// Absolute ceiling for station counts per request. Bounds response memory
/// and upstream load no matter what `?limit=` a client sends.
pub const MAX_STATION_LIMIT: u32 = 10_000;
/// Max length for free-text filters and ids.
const MAX_FILTER_LEN: usize = 200;
const MAX_STATION_ID_LEN: usize = 128;

fn check_filter_len(value: &str, what: &str) -> Result<()> {
    if value.chars().count() > MAX_FILTER_LEN {
        return Err(crate::error::AppError::BadRequest(format!(
            "{what} is too long"
        )));
    }
    Ok(())
}

impl StationsHandlers {
    pub fn new(client: RadioBrowserClient) -> Self {
        let default_limit = std::env::var("RADIO_BROWSER_STATION_LIMIT")
            .ok()
            .and_then(|value| value.parse::<u32>().ok())
            .map(|n| {
                if n == 0 {
                    1000
                } else {
                    n.min(MAX_STATION_LIMIT)
                }
            })
            .unwrap_or(1000);

        Self {
            client,
            default_limit,
        }
    }

    /// Resolve a client-supplied limit: missing/zero means the default,
    /// anything larger is clamped to the absolute ceiling.
    fn resolve_limit(&self, requested: Option<u32>) -> u32 {
        match requested {
            None | Some(0) => self.default_limit,
            Some(n) => n.min(MAX_STATION_LIMIT),
        }
    }

    /// GET /api/popular - Get popular stations
    pub async fn list_popular(
        &self,
        params: Query<PaginationParams>,
    ) -> Result<Json<Vec<Station>>> {
        debug!("Listing popular stations");
        let limit = self.resolve_limit(params.limit);
        let stations = self.client.get_popular_stations(limit).await?;
        Ok(Json(stations))
    }

    /// GET /api/search - Search stations
    pub async fn search(&self, Query(params): Query<SearchParams>) -> Result<Json<Vec<Station>>> {
        check_filter_len(&params.q, "Search query")?;
        debug!("Searching stations with query: {}", params.q);
        let limit = self.resolve_limit(params.limit);
        let stations = self.client.search_stations(&params.q, limit).await?;
        Ok(Json(stations))
    }

    /// GET /api/station/:id - Get station by ID
    pub async fn get_by_id(&self, station_id: String) -> Result<Json<Station>> {
        if station_id.len() > MAX_STATION_ID_LEN {
            return Err(crate::error::AppError::BadRequest(
                "Station id is too long".to_string(),
            ));
        }
        debug!("Getting station by id: {}", station_id);
        let station = self.client.get_station_by_id(&station_id).await?;
        Ok(Json(station))
    }

    /// Unified handler for all /api/stations filters
    pub async fn get_stations(
        &self,
        params: Query<StationsQueryParams>,
    ) -> Result<Json<Vec<Station>>> {
        let limit = self.resolve_limit(params.limit);

        if let Some(country) = &params.country {
            check_filter_len(country, "Country filter")?;
            debug!("Getting stations by country: {}", country);
            return self
                .client
                .get_stations_by_country(country, limit)
                .await
                .map(Json);
        }

        if let Some(language) = &params.language {
            check_filter_len(language, "Language filter")?;
            debug!("Getting stations by language: {}", language);
            return self
                .client
                .get_stations_by_language(language, limit)
                .await
                .map(Json);
        }

        if let Some(tag) = &params.tag {
            check_filter_len(tag, "Tag filter")?;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn handlers_with_default(default_limit: u32) -> StationsHandlers {
        StationsHandlers {
            client: crate::infra::RadioBrowserClient::new(None),
            default_limit,
        }
    }

    #[test]
    fn limits_are_clamped_to_the_ceiling() {
        let handlers = handlers_with_default(1000);
        assert_eq!(handlers.resolve_limit(None), 1000);
        assert_eq!(handlers.resolve_limit(Some(0)), 1000);
        assert_eq!(handlers.resolve_limit(Some(50)), 50);
        assert_eq!(handlers.resolve_limit(Some(10_000)), 10_000);
        assert_eq!(handlers.resolve_limit(Some(10_001)), 10_000);
        assert_eq!(handlers.resolve_limit(Some(u32::MAX)), 10_000);
    }

    #[test]
    fn oversized_filters_are_rejected() {
        assert!(check_filter_len(&"x".repeat(200), "q").is_ok());
        assert!(check_filter_len(&"x".repeat(201), "q").is_err());
    }
}
