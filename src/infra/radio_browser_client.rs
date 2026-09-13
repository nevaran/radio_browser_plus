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

    /// Get stations for a curated genre: fan out over its raw tags
    /// concurrently, then merge fairly (see [`merge_station_lists`]).
    /// Unknown names fall back to a plain tag query.
    pub async fn get_stations_by_genre(&self, genre: &str, limit: u32) -> Result<Vec<Station>> {
        if genre.eq_ignore_ascii_case(crate::genres::VARIETY_GENRE) {
            return self.get_variety_stations(limit).await;
        }
        let Some(canonical) = crate::genres::find_genre(genre) else {
            return self.get_stations_by_tag(genre, limit).await;
        };

        debug!("Fetching stations for curated genre: {}", canonical.name);
        // No upper clamp: an explicit limit is honored as-is (individual tag
        // fetches tolerate failures, and each response is body-capped).
        let per_tag = (limit / canonical.tags.len().max(1) as u32).max(50);
        let responses = futures_util::future::join_all(
            canonical
                .tags
                .iter()
                .map(|tag| self.get_stations_by_tag(tag, per_tag)),
        )
        .await;

        let mut per_tag_lists: Vec<Vec<Station>> = Vec::new();
        let mut first_error: Option<AppError> = None;
        for response in responses {
            match response {
                Ok(stations) => per_tag_lists.push(stations),
                Err(err) => {
                    tracing::warn!("Genre tag fetch failed: {}", err);
                    first_error.get_or_insert(err);
                }
            }
        }
        let merged = merge_station_lists(per_tag_lists, limit as usize);
        if merged.is_empty() {
            if let Some(err) = first_error {
                return Err(err);
            }
        }
        Ok(merged)
    }

    /// Stations outside the curated catalog: scan a bounded station set and
    /// keep entries with no canonical tag (or none at all).
    pub async fn get_variety_stations(&self, limit: u32) -> Result<Vec<Station>> {
        debug!("Fetching variety stations");
        // Uncapped scan factor like the requested limit itself; the response
        // body cap bounds worst-case memory either way.
        let scan_limit = limit.saturating_mul(5).max(limit);
        let stations = self.get_all_stations(scan_limit).await?;
        Ok(stations
            .into_iter()
            .filter(|station| !crate::genres::has_canonical_tag(&station.tags))
            .take(limit as usize)
            .collect())
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

    /// Get top tags/genres ordered by station count (the endpoint's default
    /// order is arbitrary and surfaces one-station noise first). The limit is
    /// deliberately generous: canonical genre synonyms live deep in the long
    /// tail (e.g. `kids` ranks past #250), and anything below the cutoff
    /// would wrongly aggregate to a zero count.
    pub async fn get_tags(&self) -> Result<Vec<Tag>> {
        debug!("Fetching tags");
        let url = format!(
            "{}/tags?order=stationcount&reverse=true&limit=2000",
            self.base_url
        );
        self.fetch_json_array(&url).await
    }

    // Private helper method
    async fn fetch_json_array<T: serde::de::DeserializeOwned>(&self, url: &str) -> Result<Vec<T>> {
        let started = std::time::Instant::now();
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
        tracing::info!(
            endpoint = %Self::endpoint_label(&self.base_url, url),
            items = data.len(),
            elapsed_ms = started.elapsed().as_millis(),
            "Upstream fetch completed"
        );
        Ok(data)
    }

    /// Short `/path` label for logs (drops the host and query string).
    fn endpoint_label<'a>(base_url: &str, url: &'a str) -> &'a str {
        let path = url.strip_prefix(base_url).unwrap_or(url);
        let end = path.find(['?', '#']).unwrap_or(path.len());
        &path[..end]
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

/// Identity of a station for dedupe: upstream UUID, falling back to
/// name+stream for records without one.
fn station_key(station: &Station) -> String {
    if station.stationuuid.is_empty() {
        format!(
            "{}|{}",
            station.name,
            station.url_resolved.as_deref().unwrap_or_default()
        )
    } else {
        station.stationuuid.clone()
    }
}

/// Merge per-tag station lists fairly: round-robin interleave so every raw
/// tag contributes proportionally, drop repeats (first occurrence wins),
/// then cap at `limit`.
///
/// Deliberately NO popularity ordering here: sorting by clickcount would push
/// low-clickcount — often regional or non-Latin-script — stations past the
/// cut and silently drop them. Display sorting happens client-side over the
/// already-fetched set.
fn merge_station_lists(lists: Vec<Vec<Station>>, limit: usize) -> Vec<Station> {
    let mut iters: Vec<_> = lists.into_iter().map(Vec::into_iter).collect();
    let mut merged = Vec::new();
    loop {
        let mut progressed = false;
        for it in iters.iter_mut() {
            if let Some(station) = it.next() {
                merged.push(station);
                progressed = true;
            }
        }
        if !progressed {
            break;
        }
    }

    let mut seen = std::collections::HashSet::new();
    merged.retain(|station| seen.insert(station_key(station)));
    merged.truncate(limit);
    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_station(uuid: &str, name: &str, clickcount: u32) -> Station {
        Station {
            changeuuid: None,
            stationuuid: uuid.to_string(),
            uuid: None,
            id: None,
            serveruuid: None,
            name: name.to_string(),
            url: None,
            url_resolved: None,
            homepage: None,
            favicon: None,
            tags: None,
            country: None,
            countrycode: None,
            iso_3166_1: None,
            iso_3166_2: None,
            language: None,
            lastcheckok: None,
            bitrate: None,
            clickcount: Some(clickcount),
            state: None,
            has_https: None,
            ssl_error: None,
            genre: None,
        }
    }

    #[test]
    fn merge_interleaves_fairly_across_tags() {
        let lists = vec![
            vec![
                test_station("a1", "A1", 10),
                test_station("a2", "A2", 20),
                test_station("a3", "A3", 30),
            ],
            vec![test_station("b1", "B1", 40)],
            vec![test_station("c1", "C1", 50), test_station("c2", "C2", 60)],
        ];
        let names: Vec<_> = merge_station_lists(lists, 10)
            .iter()
            .map(|s| s.name.clone())
            .collect();
        assert_eq!(names, vec!["A1", "B1", "C1", "A2", "C2", "A3"]);
    }

    #[test]
    fn merge_dedupes_and_respects_limit() {
        let lists = vec![
            vec![test_station("a1", "A1", 1), test_station("dup", "Dup", 2)],
            vec![test_station("dup", "Dup", 2), test_station("b1", "B1", 3)],
        ];
        let names: Vec<_> = merge_station_lists(lists, 2)
            .iter()
            .map(|s| s.name.clone())
            .collect();
        // Round-robin order A1, Dup, B1 — dupe dropped, cut at 2.
        assert_eq!(names, vec!["A1", "Dup"]);
    }

    #[test]
    fn merge_keeps_low_clickcount_regional_stations() {
        // Regression test: popularity-sorting the fetch dropped precisely
        // these stations past the limit cut.
        let lists = vec![
            vec![
                test_station("l1", "Mega Hits FM", 9999),
                test_station("l2", "Super Pop Radio", 9000),
                test_station("l3", "Top Music NL", 8000),
            ],
            vec![
                test_station("c1", "Радио Рекорд", 5),
                test_station("c2", "Наше Радио", 3),
            ],
        ];
        let names: Vec<_> = merge_station_lists(lists, 4)
            .iter()
            .map(|s| s.name.clone())
            .collect();
        assert!(names.contains(&"Радио Рекорд".to_string()));
        assert!(names.contains(&"Наше Радио".to_string()));
        assert_eq!(names.len(), 4);
    }
}
