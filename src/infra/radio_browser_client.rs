// HTTP client for radio-browser.info API
use crate::domain::{Country, Language, Station, Tag};
use crate::error::{AppError, Result};
use reqwest::{redirect, Client};
use std::net::{IpAddr, Ipv4Addr};
use tokio::net::lookup_host;
use tracing::debug;

/// Max accepted upstream JSON body: comfortably above the largest capped
/// station list, far below memory-exhaustion territory.
const MAX_BODY_BYTES: usize = 32 * 1024 * 1024;
/// Max accepted artwork body for the failover proxy: station covers are small.
const MAX_ART_BODY_BYTES: usize = 3 * 1024 * 1024;
const MAX_PROXY_REDIRECTS: usize = 10;
/// Some station servers reject requests without one.
const STREAM_USER_AGENT: &str = "RadioBrowserPlus/1.0";

#[derive(Clone)]
pub struct RadioBrowserClient {
    client: Client,
    proxy_client: Client,
    base_url: String,
}

impl RadioBrowserClient {
    pub fn new(base_url: Option<String>) -> Self {
        Self {
            client: Client::new(),
            proxy_client: Client::builder()
                .redirect(redirect::Policy::none())
                .build()
                .unwrap_or_else(|_| Client::new()),
            base_url: base_url
                .unwrap_or_else(|| "https://de1.api.radio-browser.info/json".to_string()),
        }
    }

    /// GET `{base}/{path}?hidebroken=true{extra}{limit}` and parse the array.
    /// `extra` holds endpoint-specific query segments (already encoded).
    async fn fetch_station_list(
        &self,
        path: &str,
        extra: &str,
        limit: u32,
    ) -> Result<Vec<Station>> {
        debug!("Fetching {path}{extra} with limit {limit}");
        let url = format!(
            "{}{}?hidebroken=true{}{}",
            self.base_url,
            path,
            extra,
            limit_query(limit)
        );
        self.fetch_json_array(&url).await
    }

    /// Fetch all stations
    pub async fn get_all_stations(&self, limit: u32) -> Result<Vec<Station>> {
        self.fetch_station_list("/stations", "", limit).await
    }

    /// Fetch popular stations sorted by click count
    pub async fn get_popular_stations(&self, limit: u32) -> Result<Vec<Station>> {
        self.fetch_station_list("/stations", "&order=clickcount&reverse=true", limit)
            .await
    }

    /// Search stations by name
    pub async fn search_stations(&self, query: &str, limit: u32) -> Result<Vec<Station>> {
        let extra = format!("&name={}", urlencoding::encode(query));
        self.fetch_station_list("/stations/search", &extra, limit)
            .await
    }

    /// Get stations by country
    pub async fn get_stations_by_country(&self, country: &str, limit: u32) -> Result<Vec<Station>> {
        let path = format!("/stations/bycountry/{}", urlencoding::encode(country));
        self.fetch_station_list(&path, "", limit).await
    }

    /// Get stations by language
    pub async fn get_stations_by_language(
        &self,
        language: &str,
        limit: u32,
    ) -> Result<Vec<Station>> {
        let path = format!("/stations/bylanguage/{}", urlencoding::encode(language));
        self.fetch_station_list(&path, "", limit).await
    }

    /// Get stations by tag/genre
    pub async fn get_stations_by_tag(&self, tag: &str, limit: u32) -> Result<Vec<Station>> {
        let path = format!("/stations/bytag/{}", urlencoding::encode(tag));
        self.fetch_station_list(&path, "", limit).await
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
        let url = format!("{}/countries?limit=1000", self.base_url);
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

    /// Resolve a station's stream target from the directory, without
    /// contacting the station.
    pub async fn station_stream_url(&self, station_id: &str) -> Result<reqwest::Url> {
        let station = self.get_station_by_id(station_id).await?;
        resolve_stream_target(&station)
    }

    /// Open a stream URL server-side for the failover proxy. Deliberately
    /// separate from the JSON helpers: the body is unbounded (a radio stream
    /// never "finishes") and no total timeout is applied after the request is
    /// established.
    pub async fn open_stream(&self, raw_url: &str) -> Result<reqwest::Response> {
        let url = http_url(raw_url)?;
        debug!(url = %url, "Proxying station stream");
        self.proxy_get(url, None).await
    }

    /// Resolve a station's artwork URL from the directory, without fetching
    /// it.
    pub async fn station_art_url(&self, station_id: &str) -> Result<reqwest::Url> {
        let station = self.get_station_by_id(station_id).await?;
        let raw = station
            .favicon
            .as_deref()
            .map(str::trim)
            .filter(|url| !url.is_empty())
            .ok_or_else(|| AppError::NotFound("Station has no artwork".to_string()))?;
        http_url(raw)
    }

    /// Fetch artwork server-side for the failover proxy. The caller supplies
    /// a URL resolved from the directory or a validated saved favorite.
    pub async fn fetch_art(&self, raw_url: &str) -> Result<(Vec<u8>, Option<String>)> {
        let url = http_url(raw_url)?;
        debug!(url = %url, "Proxying station artwork");
        let response = self
            .proxy_get(url, Some(std::time::Duration::from_secs(15)))
            .await?;
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .filter(|value| is_safe_image_type(value))
            .map(str::to_owned);
        let bytes = Self::read_capped_bytes(response, MAX_ART_BODY_BYTES).await?;
        Ok((bytes, content_type))
    }

    async fn proxy_get(
        &self,
        mut url: reqwest::Url,
        timeout: Option<std::time::Duration>,
    ) -> Result<reqwest::Response> {
        let mut redirects = 0;
        loop {
            validate_public_url(&url).await?;
            let mut request = self
                .proxy_client
                .get(url.clone())
                .header(reqwest::header::USER_AGENT, STREAM_USER_AGENT);
            if let Some(timeout) = timeout {
                request = request.timeout(timeout);
            }
            let response = request.send().await?;
            if response.status().is_redirection() {
                if redirects >= MAX_PROXY_REDIRECTS {
                    return Err(AppError::ExternalServiceError(
                        "Station server redirected too many times".to_string(),
                    ));
                }
                let location = response
                    .headers()
                    .get(reqwest::header::LOCATION)
                    .and_then(|value| value.to_str().ok())
                    .ok_or_else(|| {
                        AppError::ExternalServiceError(
                            "Station server returned an invalid redirect".to_string(),
                        )
                    })?;
                let next = redirect_target(response.url(), location)?;
                drop(response);
                url = http_url(next.as_str())?;
                redirects += 1;
                continue;
            }
            if !response.status().is_success() {
                return Err(AppError::ExternalServiceError(format!(
                    "Station server error: {}",
                    response.status()
                )));
            }
            return Ok(response);
        }
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

    /// Read a response body as UTF-8 under the JSON body cap.
    async fn read_capped_text(response: reqwest::Response) -> Result<String> {
        let buf = Self::read_capped_bytes(response, MAX_BODY_BYTES).await?;
        String::from_utf8(buf).map_err(|e| {
            AppError::ExternalServiceError(format!("Radio browser API returned invalid UTF-8: {e}"))
        })
    }

    /// Read a response body with an upper bound so a misbehaving upstream
    /// cannot exhaust server memory, no matter the Content-Length it claims.
    async fn read_capped_bytes(response: reqwest::Response, max_bytes: usize) -> Result<Vec<u8>> {
        if let Some(len) = response.content_length()
            && len > max_bytes as u64
        {
            return Err(AppError::ExternalServiceError(format!(
                "Upstream response too large: {len} bytes"
            )));
        }
        let mut response = response;
        let mut buf = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            if buf.len() + chunk.len() > max_bytes {
                return Err(AppError::ExternalServiceError(
                    "Upstream response too large".to_string(),
                ));
            }
            buf.extend_from_slice(&chunk);
        }
        Ok(buf)
    }
}

fn limit_query(limit: u32) -> String {
    if limit == 0 {
        String::new()
    } else {
        format!("&limit={}", limit)
    }
}

/// Only raster images are relayed: an upstream SVG opened directly could run
/// script on the app origin, so those fall back to the default image type.
fn is_safe_image_type(value: &str) -> bool {
    let value = value.to_ascii_lowercase();
    value.starts_with("image/") && !value.starts_with("image/svg")
}

/// Parse an upstream-provided station URL, refusing anything but HTTP(S) so
/// the proxy can never be pointed at `file:`/`data:` or similar schemes.
fn http_url(raw: &str) -> Result<reqwest::Url> {
    let url = reqwest::Url::parse(raw)
        .map_err(|_| AppError::BadRequest("Station URL is invalid".to_string()))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(AppError::BadRequest(
            "Only http and https station URLs can be proxied".to_string(),
        ));
    }
    if url.username() != "" || url.password().is_some() {
        return Err(AppError::BadRequest(
            "Station URLs must not contain credentials".to_string(),
        ));
    }
    Ok(url)
}

fn redirect_target(current: &reqwest::Url, location: &str) -> Result<reqwest::Url> {
    let next = current.join(location).map_err(|_| {
        AppError::ExternalServiceError("Station server returned an invalid redirect".to_string())
    })?;
    http_url(next.as_str()).map_err(|_| {
        AppError::ExternalServiceError("Station server returned an invalid redirect".to_string())
    })
}

async fn validate_public_url(url: &reqwest::Url) -> Result<()> {
    let host = url
        .host_str()
        .ok_or_else(|| AppError::BadRequest("Station URL has no host".to_string()))?;
    let port = url.port_or_known_default().unwrap_or(80);

    if let Ok(ip) = host.parse::<IpAddr>() {
        if is_non_public_ip(ip) {
            return Err(AppError::BadRequest(
                "Private or local station addresses are not allowed".to_string(),
            ));
        }
        return Ok(());
    }

    let addresses = lookup_host((host, port))
        .await
        .map_err(|_| AppError::BadRequest("Station host could not be resolved".to_string()))?;
    let mut resolved = false;
    for address in addresses {
        resolved = true;
        if is_non_public_ip(address.ip()) {
            return Err(AppError::BadRequest(
                "Private or local station addresses are not allowed".to_string(),
            ));
        }
    }
    if !resolved {
        return Err(AppError::BadRequest(
            "Station host could not be resolved".to_string(),
        ));
    }
    Ok(())
}

fn is_non_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => is_non_public_ipv4(ip),
        IpAddr::V6(ip) => {
            if let Some(ip) = ip.to_ipv4_mapped() {
                return is_non_public_ipv4(ip);
            }
            ip.is_unspecified()
                || ip.is_loopback()
                || ip.is_unique_local()
                || ip.is_unicast_link_local()
                || ip.is_multicast()
                || (ip.segments()[0] == 0x2001 && ip.segments()[1] == 0xdb8)
        }
    }
}

fn is_non_public_ipv4(ip: Ipv4Addr) -> bool {
    let octets = ip.octets();
    ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_unspecified()
        || ip.is_multicast()
        || octets[0] == 0
        || octets[0] == 100 && (64..=127).contains(&octets[1])
        || octets[0] == 192 && octets[1] == 0 && octets[2] == 0
        || (octets[0] == 192 && octets[1] == 0 && octets[2] == 2)
        || (octets[0] == 198 && octets[1] == 18)
        || (octets[0] == 198 && octets[1] == 51 && octets[2] == 100)
        || (octets[0] == 203 && octets[1] == 0 && octets[2] == 113)
        || (octets[0] >= 240)
}

/// Prefer the resolved stream URL (redirects already followed by the
/// directory) and fall back to the raw one.
fn resolve_stream_target(station: &Station) -> Result<reqwest::Url> {
    let raw = [station.url_resolved.as_deref(), station.url.as_deref()]
        .into_iter()
        .flatten()
        .map(str::trim)
        .find(|url| !url.is_empty())
        .ok_or_else(|| AppError::NotFound("Station has no stream URL".to_string()))?;
    http_url(raw)
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

    #[test]
    fn stream_target_prefers_resolved_url() {
        let mut station = test_station("s1", "S1", 0);
        station.url = Some("http://raw.example/stream".to_string());
        station.url_resolved = Some("  https://resolved.example/live  ".to_string());
        assert_eq!(
            resolve_stream_target(&station).unwrap().as_str(),
            "https://resolved.example/live"
        );

        station.url_resolved = Some("   ".to_string());
        assert_eq!(
            resolve_stream_target(&station).unwrap().as_str(),
            "http://raw.example/stream"
        );
    }

    #[test]
    fn stream_target_rejects_missing_and_non_http_urls() {
        let mut station = test_station("s2", "S2", 0);
        station.url = None;
        station.url_resolved = None;
        assert!(resolve_stream_target(&station).is_err());

        station.url = Some("file:///etc/passwd".to_string());
        assert!(resolve_stream_target(&station).is_err());

        station.url = Some("not a url".to_string());
        assert!(resolve_stream_target(&station).is_err());
    }

    #[test]
    fn proxy_redirects_resolve_relative_locations_and_reject_unsafe_urls() {
        let current = reqwest::Url::parse("http://radio.example/live").unwrap();
        assert_eq!(
            redirect_target(&current, "/stream").unwrap().as_str(),
            "http://radio.example/stream"
        );
        assert_eq!(
            redirect_target(&current, "https://cdn.example/live")
                .unwrap()
                .as_str(),
            "https://cdn.example/live"
        );
        assert!(redirect_target(&current, "file:///etc/passwd").is_err());
        assert!(redirect_target(&current, "http://user:pass@radio.example/").is_err());
    }

    #[test]
    fn artwork_relay_skips_svg_content_types() {
        assert!(is_safe_image_type("image/png"));
        assert!(is_safe_image_type("IMAGE/JPEG"));
        assert!(!is_safe_image_type("image/svg+xml"));
        assert!(!is_safe_image_type("text/html"));
    }

    #[test]
    fn proxy_rejects_private_and_reserved_ip_addresses() {
        assert!(is_non_public_ip("127.0.0.1".parse().unwrap()));
        assert!(is_non_public_ip("10.0.0.1".parse().unwrap()));
        assert!(is_non_public_ip("172.16.0.1".parse().unwrap()));
        assert!(is_non_public_ip("192.168.1.1".parse().unwrap()));
        assert!(is_non_public_ip("169.254.169.254".parse().unwrap()));
        assert!(is_non_public_ip("100.64.0.1".parse().unwrap()));
        assert!(is_non_public_ip("::1".parse().unwrap()));
        assert!(is_non_public_ip("fc00::1".parse().unwrap()));
        assert!(is_non_public_ip("fe80::1".parse().unwrap()));
        assert!(!is_non_public_ip("8.8.8.8".parse().unwrap()));
        assert!(!is_non_public_ip("2606:4700:4700::1111".parse().unwrap()));
    }
}
