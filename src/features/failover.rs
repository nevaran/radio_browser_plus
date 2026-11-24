// Server-side failover proxy for station streams and artwork.
//
// The browser talks to station origins directly; when the ISP, a local
// network or a regional restriction blocks that origin, the client falls back
// to these same-origin endpoints and the server makes the connection instead.
// Both endpoints take only a station id: URLs come from the radio-browser
// record or the requesting user's saved favorite, never from a proxy URL
// supplied on the request.
use axum::{
    body::Body,
    http::{header, HeaderMap, HeaderValue},
    response::Response,
};
use futures_util::stream;
use std::sync::Arc;
use tracing::debug;

use crate::domain::Favorite;
use crate::error::{AppError, Result};
use crate::features::FavoritesHandlers;
use crate::infra::RadioBrowserClient;

/// Same bound as the station detail endpoint; upstream ids are UUIDs.
const MAX_STATION_ID_LEN: usize = 128;
const DEFAULT_AUDIO_CONTENT_TYPE: &str = "audio/mpeg";
const DEFAULT_IMAGE_CONTENT_TYPE: &str = "image/png";

#[derive(Clone)]
pub struct FailoverHandlers {
    client: RadioBrowserClient,
    favorites: Arc<FavoritesHandlers>,
}

impl FailoverHandlers {
    pub fn new(client: RadioBrowserClient, favorites: Arc<FavoritesHandlers>) -> Self {
        Self { client, favorites }
    }

    /// GET /api/stations/{id}/stream - relay the station's audio through the
    /// server. The upstream body is streamed chunk by chunk: audio never
    /// fits (or should wait for) a capped response body.
    pub async fn stream(&self, headers: HeaderMap, station_id: String) -> Result<Response> {
        check_station_id(&station_id)?;
        let target = match self.client.station_stream_url(&station_id).await {
            Ok(url) => url.to_string(),
            Err(err) => {
                let Some(favorite) = self.favorites.saved_favorite(&headers, &station_id).await?
                else {
                    return Err(err);
                };
                favorite_stream_url(&favorite).ok_or(err)?.to_string()
            }
        };
        let upstream = self.client.open_stream(&target).await?;
        let content_type = upstream
            .headers()
            .get(header::CONTENT_TYPE)
            .cloned()
            .unwrap_or_else(|| HeaderValue::from_static(DEFAULT_AUDIO_CONTENT_TYPE));

        let body = Body::from_stream(stream::unfold(Some(upstream), |state| async move {
            let mut upstream = state?;
            match upstream.chunk().await {
                Ok(Some(chunk)) => Some((Ok::<_, reqwest::Error>(chunk), Some(upstream))),
                Ok(None) => None,
                Err(err) => Some((Err(err), None)),
            }
        }));

        debug!(station_id = %station_id, "Relaying station stream");
        let mut response = Response::new(body);
        response
            .headers_mut()
            .insert(header::CONTENT_TYPE, content_type);
        response
            .headers_mut()
            .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
        // Nginx/openresty buffers proxied responses by default; for a live
        // audio stream that adds latency and bursty delivery.
        response
            .headers_mut()
            .insert("x-accel-buffering", HeaderValue::from_static("no"));
        Ok(response)
    }

    /// GET /api/stations/{id}/art - relay the station's artwork. The usual
    /// cause of a missing cover is the same unreachable origin the stream
    /// proxy handles.
    pub async fn art(&self, headers: HeaderMap, station_id: String) -> Result<Response> {
        check_station_id(&station_id)?;
        let target = match self.client.station_art_url(&station_id).await {
            Ok(url) => url.to_string(),
            Err(err) => {
                let Some(favorite) = self.favorites.saved_favorite(&headers, &station_id).await?
                else {
                    return Err(err);
                };
                favorite_art_url(&favorite).ok_or(err)?.to_string()
            }
        };
        let (bytes, content_type) = self.client.fetch_art(&target).await?;
        let content_type = content_type
            .and_then(|value| HeaderValue::from_str(&value).ok())
            .unwrap_or_else(|| HeaderValue::from_static(DEFAULT_IMAGE_CONTENT_TYPE));

        debug!(station_id = %station_id, bytes = bytes.len(), "Relaying station artwork");
        let mut response = Response::new(Body::from(bytes));
        response
            .headers_mut()
            .insert(header::CONTENT_TYPE, content_type);
        response.headers_mut().insert(
            header::CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=86400"),
        );
        Ok(response)
    }
}

fn favorite_stream_url(favorite: &Favorite) -> Option<&str> {
    [favorite.url_resolved.as_deref(), favorite.url.as_deref()]
        .into_iter()
        .flatten()
        .map(str::trim)
        .find(|url| !url.is_empty())
}

fn favorite_art_url(favorite: &Favorite) -> Option<&str> {
    favorite
        .favicon
        .as_deref()
        .map(str::trim)
        .filter(|url| !url.is_empty())
}

fn check_station_id(station_id: &str) -> Result<()> {
    if station_id.is_empty() || station_id.len() > MAX_STATION_ID_LEN {
        return Err(AppError::BadRequest("Station id is invalid".to_string()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Favorite;

    #[test]
    fn station_id_length_is_bounded() {
        assert!(check_station_id("abc-123").is_ok());
        assert!(check_station_id("x".repeat(MAX_STATION_ID_LEN).as_str()).is_ok());
        assert!(check_station_id("").is_err());
        assert!(check_station_id("x".repeat(MAX_STATION_ID_LEN + 1).as_str()).is_err());
    }

    #[test]
    fn favorite_urls_are_available_when_directory_entry_is_gone() {
        let favorite = Favorite {
            station_id: "s1".to_string(),
            name: "Station".to_string(),
            url: Some("http://raw.example/live".to_string()),
            url_resolved: Some(" https://resolved.example/live ".to_string()),
            favicon: Some(" https://art.example/cover.png ".to_string()),
            country: None,
            countrycode: None,
            bitrate: None,
            genre: None,
            tags: None,
        };

        assert_eq!(
            favorite_stream_url(&favorite),
            Some("https://resolved.example/live")
        );
        assert_eq!(
            favorite_art_url(&favorite),
            Some("https://art.example/cover.png")
        );
    }
}
