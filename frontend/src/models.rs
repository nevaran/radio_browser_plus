//! Shared data models matching the Axum backend JSON API.
//!
//! Deserializers are intentionally tolerant (numbers vs. strings, string vs.
//! array) so favorites persisted by older clients keep loading.

use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashMap;

fn opt_bool<'de, D>(d: D) -> Result<Option<bool>, D::Error>
where
    D: Deserializer<'de>,
{
    let v = serde_json::Value::deserialize(d)?;
    match v {
        serde_json::Value::Bool(b) => Ok(Some(b)),
        serde_json::Value::Number(n) => Ok(n.as_i64().map(|i| i != 0)),
        _ => Ok(None),
    }
}

fn opt_stringy_u32<'de, D>(d: D) -> Result<Option<u32>, D::Error>
where
    D: Deserializer<'de>,
{
    let v = serde_json::Value::deserialize(d)?;
    match v {
        serde_json::Value::Number(n) => Ok(n.as_u64().and_then(|x| u32::try_from(x).ok())),
        serde_json::Value::String(s) => {
            // Accept both plain numbers and display strings like "128 kbps".
            let digits: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
            Ok(digits.parse().ok())
        }
        _ => Ok(None),
    }
}

fn opt_string_or_array<'de, D>(d: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let v = serde_json::Value::deserialize(d)?;
    match v {
        serde_json::Value::String(s) => Ok(Some(s)),
        serde_json::Value::Array(items) => Ok(Some(
            items
                .into_iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect::<Vec<_>>()
                .join(","),
        )),
        _ => Ok(None),
    }
}

/// Station as returned by `/api/stations`, `/api/popular`, `/api/search`, ...
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Station {
    #[serde(default)]
    pub changeuuid: Option<String>,
    #[serde(default)]
    pub stationuuid: String,
    #[serde(default)]
    pub uuid: Option<String>,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub url_resolved: Option<String>,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub favicon: Option<String>,
    #[serde(default, deserialize_with = "opt_string_or_array")]
    pub tags: Option<String>,
    #[serde(default)]
    pub country: Option<String>,
    #[serde(default)]
    pub countrycode: Option<String>,
    #[serde(default)]
    pub iso_3166_1: Option<String>,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default, deserialize_with = "opt_bool")]
    pub lastcheckok: Option<bool>,
    #[serde(default, deserialize_with = "opt_stringy_u32")]
    pub bitrate: Option<u32>,
    #[serde(default, deserialize_with = "opt_stringy_u32")]
    pub clickcount: Option<u32>,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default, deserialize_with = "opt_bool")]
    pub has_https: Option<bool>,
    #[serde(default, deserialize_with = "opt_bool")]
    pub ssl_error: Option<bool>,
    #[serde(default)]
    pub genre: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Favorite {
    #[serde(default)]
    pub station_id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub url_resolved: Option<String>,
    #[serde(default)]
    pub favicon: Option<String>,
    #[serde(default)]
    pub country: Option<String>,
    #[serde(default, deserialize_with = "opt_stringy_u32")]
    pub bitrate: Option<u32>,
    #[serde(default)]
    pub genre: Option<String>,
    #[serde(default, deserialize_with = "opt_string_or_array")]
    pub tags: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Country {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub iso_3166_1: Option<String>,
    #[serde(default)]
    pub stationcount: Option<u32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Language {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub stationcount: Option<u32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Tag {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub stationcount: Option<u32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FavoritesResponse {
    #[serde(default)]
    pub favorites: HashMap<String, Favorite>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct User {
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub role: String,
}

/// Public runtime config from `/api/config`. Unknown fields are ignored so
/// older frontends keep working against newer backends and vice versa.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub allow_guest: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct FavoritePayload {
    pub station_id: String,
    pub name: String,
    pub favicon: Option<String>,
    pub url: Option<String>,
    pub url_resolved: Option<String>,
    pub country: Option<String>,
    pub bitrate: Option<u32>,
    pub genre: Option<String>,
    pub tags: Option<String>,
}

impl FavoritePayload {
    pub fn from_station(station: &Station, genre: Option<String>) -> Option<Self> {
        let id = station_id(station)?;
        let stream = station.url_resolved.clone().or_else(|| station.url.clone());
        Some(Self {
            station_id: id,
            name: station.name.clone(),
            favicon: station.favicon.clone(),
            url: stream.clone(),
            url_resolved: stream,
            country: station.country.clone(),
            bitrate: station.bitrate,
            genre,
            tags: station.tags.clone(),
        })
    }
}

/// Unified row for the Countries / Languages / Genres collection grids.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CollectionItem {
    pub label: String,
    pub filter_value: String,
    pub count: Option<u32>,
    pub icon_url: String,
}

pub fn station_id(station: &Station) -> Option<String> {
    if !station.stationuuid.is_empty() {
        return Some(station.stationuuid.clone());
    }
    if let Some(uuid) = &station.uuid {
        if !uuid.is_empty() {
            return Some(uuid.clone());
        }
    }
    if let Some(id) = &station.id {
        if !id.is_empty() {
            return Some(id.clone());
        }
    }
    None
}

pub fn station_stream_url(station: &Station) -> Option<String> {
    station
        .url_resolved
        .clone()
        .filter(|u| !u.is_empty())
        .or_else(|| station.url.clone().filter(|u| !u.is_empty()))
}

pub fn favorite_to_station(id: &str, fav: &Favorite) -> Station {
    Station {
        stationuuid: id.to_string(),
        name: fav.name.clone(),
        favicon: fav.favicon.clone(),
        country: fav.country.clone(),
        bitrate: fav.bitrate,
        genre: fav.genre.clone(),
        tags: fav.tags.clone(),
        url: fav.url.clone(),
        url_resolved: fav.url_resolved.clone(),
        ..Station::default()
    }
}
