// HTTP client for radio-browser.info API
use crate::domain::{Country, Language, Station, Tag};
use crate::error::{AppError, Result};
use reqwest::Client;
use tracing::debug;

#[derive(Clone)]
pub struct RadioBrowserClient {
    client: Client,
    base_url: String,
}

impl RadioBrowserClient {
    pub fn new(base_url: Option<String>) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url
                .unwrap_or_else(|| "https://de1.api.radio-browser.info/json".to_string()),
        }
    }

    /// Fetch all stations
    pub async fn get_all_stations(&self, limit: u32) -> Result<Vec<Station>> {
        debug!("Fetching all stations with limit {}", limit);
        let url = format!(
            "{}/stations?hidebroken=true{}",
            self.base_url,
            limit_query(limit)
        );
        self.fetch_json_array(&url).await
    }

    /// Fetch popular stations sorted by click count
    pub async fn get_popular_stations(&self, limit: u32) -> Result<Vec<Station>> {
        debug!("Fetching popular stations with limit {}", limit);
        let url = format!(
            "{}/stations?hidebroken=true&order=clickcount&reverse=true{}",
            self.base_url,
            limit_query(limit)
        );
        self.fetch_json_array(&url).await
    }

    /// Search stations by name
    pub async fn search_stations(&self, query: &str, limit: u32) -> Result<Vec<Station>> {
        debug!("Searching stations with query: {}", query);
        let url = format!(
            "{}/stations/search?hidebroken=true&name={}{}",
            self.base_url,
            urlencoding::encode(query),
            limit_query(limit)
        );
        self.fetch_json_array(&url).await
    }

    /// Get stations by country
    pub async fn get_stations_by_country(&self, country: &str, limit: u32) -> Result<Vec<Station>> {
        debug!("Fetching stations for country: {}", country);
        let url = format!(
            "{}/stations/bycountry/{}?hidebroken=true{}",
            self.base_url,
            urlencoding::encode(country),
            limit_query(limit)
        );
        self.fetch_json_array(&url).await
    }

    /// Get stations by language
    pub async fn get_stations_by_language(
        &self,
        language: &str,
        limit: u32,
    ) -> Result<Vec<Station>> {
        debug!("Fetching stations for language: {}", language);
        let url = format!(
            "{}/stations/bylanguage/{}?hidebroken=true{}",
            self.base_url,
            urlencoding::encode(language),
            limit_query(limit)
        );
        self.fetch_json_array(&url).await
    }

    /// Get stations by tag/genre
    pub async fn get_stations_by_tag(&self, tag: &str, limit: u32) -> Result<Vec<Station>> {
        debug!("Fetching stations for tag: {}", tag);
        let url = format!(
            "{}/stations/bytag/{}?hidebroken=true{}",
            self.base_url,
            urlencoding::encode(tag),
            limit_query(limit)
        );
        self.fetch_json_array(&url).await
    }

    /// Get station by UUID
    pub async fn get_station_by_id(&self, station_id: &str) -> Result<Station> {
        debug!("Fetching station with id: {}", station_id);
        let url = format!(
            "{}/stations/byuuid/{}?hidebroken=true",
            self.base_url,
            urlencoding::encode(station_id)
        );
        let stations: Vec<Station> = self.fetch_json_array(&url).await?;
        stations
            .into_iter()
            .next()
            .ok_or_else(|| AppError::NotFound(format!("Station {} not found", station_id)))
    }

    /// Get all countries
    pub async fn get_countries(&self) -> Result<Vec<Country>> {
        debug!("Fetching countries");
        let url = format!("{}/countries?limit=200", self.base_url);
        self.fetch_json_array(&url).await
    }

    /// Get all languages
    pub async fn get_languages(&self) -> Result<Vec<Language>> {
        debug!("Fetching languages");
        let url = format!("{}/languages?limit=200", self.base_url);
        self.fetch_json_array(&url).await
    }

    /// Get all tags/genres
    pub async fn get_tags(&self) -> Result<Vec<Tag>> {
        debug!("Fetching tags");
        let url = format!("{}/tags?limit=200", self.base_url);
        self.fetch_json_array(&url).await
    }

    // Private helper method
    async fn fetch_json_array<T: serde::de::DeserializeOwned>(&self, url: &str) -> Result<Vec<T>> {
        let response = self
            .client
            .get(url)
            .timeout(std::time::Duration::from_secs(20))
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(AppError::ExternalServiceError(format!(
                "Radio browser API error: {}",
                response.status()
            )));
        }

        let body_text = Self::read_capped_text(response).await?;

        let data = serde_json::from_str::<Vec<T>>(&body_text).map_err(|e| {
            tracing::error!("Failed to deserialize response: {}", e);
            AppError::SerializationError(e)
        })?;
        Ok(data)
    }

    /// Read a response body with an upper bound so a misbehaving upstream
    /// cannot exhaust server memory, no matter the Content-Length it claims.
    async fn read_capped_text(response: reqwest::Response) -> Result<String> {
        /// Max accepted upstream body: comfortably above the largest capped
        /// station list, far below memory-exhaustion territory.
        const MAX_BODY_BYTES: usize = 32 * 1024 * 1024;
        if let Some(len) = response.content_length() {
            if len > MAX_BODY_BYTES as u64 {
                return Err(AppError::ExternalServiceError(format!(
                    "Radio browser API response too large: {len} bytes"
                )));
            }
        }
        let mut response = response;
        let mut buf = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            if buf.len() + chunk.len() > MAX_BODY_BYTES {
                return Err(AppError::ExternalServiceError(
                    "Radio browser API response too large".to_string(),
                ));
            }
            buf.extend_from_slice(&chunk);
        }
        String::from_utf8(buf).map_err(|e| {
            AppError::ExternalServiceError(format!("Radio browser API returned invalid UTF-8: {e}"))
        })
    }
}

fn limit_query(limit: u32) -> String {
    if limit == 0 {
        String::new()
    } else {
        format!("&limit={}", limit)
    }
}
