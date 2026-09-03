// Domain models and value types for radio browser

use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashMap;

/// Helper to deserialize boolean from integer or boolean value
fn deserialize_bool<'de, D>(deserializer: D) -> Result<Option<bool>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    match value {
        serde_json::Value::Bool(b) => Ok(Some(b)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(Some(i != 0))
            } else {
                Ok(None)
            }
        }
        serde_json::Value::Null => Ok(None),
        _ => Ok(None),
    }
}

fn deserialize_optional_string_or_array<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    match value {
        serde_json::Value::String(value) => Ok(Some(value)),
        serde_json::Value::Array(values) => Ok(Some(
            values
                .into_iter()
                .filter_map(|value| value.as_str().map(str::to_owned))
                .collect::<Vec<_>>()
                .join(","),
        )),
        serde_json::Value::Null => Ok(None),
        _ => Ok(None),
    }
}

fn deserialize_optional_u32<'de, D>(deserializer: D) -> Result<Option<u32>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    match value {
        serde_json::Value::Number(value) => Ok(value.as_u64().and_then(|value| u32::try_from(value).ok())),
        serde_json::Value::String(value) => Ok(value.parse().ok()),
        serde_json::Value::Null => Ok(None),
        _ => Ok(None),
    }
}

/// Station from radio-browser.info API
#[derive(Debug, Serialize, Deserialize, Clone)]
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
    pub serveruuid: Option<String>,
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
    #[serde(default)]
    pub tags: Option<String>,
    #[serde(default)]
    pub country: Option<String>,
    #[serde(default)]
    pub countrycode: Option<String>,
    #[serde(default)]
    pub iso_3166_1: Option<String>,
    #[serde(default)]
    pub iso_3166_2: Option<String>,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default, deserialize_with = "deserialize_bool")]
    pub lastcheckok: Option<bool>,
    #[serde(default)]
    pub bitrate: Option<u32>,
    #[serde(default)]
    pub clickcount: Option<u32>,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default, deserialize_with = "deserialize_bool")]
    pub has_https: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_bool")]
    pub ssl_error: Option<bool>,
    #[serde(default)]
    pub genre: Option<String>,
}

/// Saved favorite station
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Favorite {
    pub station_id: String,
    pub name: String,
    pub url: Option<String>,
    pub url_resolved: Option<String>,
    pub favicon: Option<String>,
    pub country: Option<String>,
    pub bitrate: Option<u32>,
    pub genre: Option<String>,
    pub tags: Option<String>,
}

/// Country info
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Country {
    pub name: String,
    pub iso_3166_1: Option<String>,
    pub stationcount: Option<u32>,
}

/// Language info
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Language {
    pub name: String,
    pub stationcount: Option<u32>,
}

/// Tag/Genre info
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Tag {
    pub name: String,
    pub stationcount: Option<u32>,
}

/// Favorites collection wrapper
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct FavoritesData {
    #[serde(flatten)]
    pub favorites: HashMap<String, Favorite>,
}

impl FavoritesData {
    pub fn new() -> Self {
        Self {
            favorites: HashMap::new(),
        }
    }

    pub fn toggle(&mut self, favorite: Favorite) {
        if self.favorites.contains_key(&favorite.station_id) {
            self.favorites.remove(&favorite.station_id);
        } else {
            self.favorites.insert(favorite.station_id.clone(), favorite);
        }
    }

    pub fn update(&mut self, station_id: &str, favorite: Favorite) {
        self.favorites.insert(station_id.to_string(), favorite);
    }

    pub fn get(&self, station_id: &str) -> Option<&Favorite> {
        self.favorites.get(station_id)
    }

    pub fn is_favorite(&self, station_id: &str) -> bool {
        self.favorites.contains_key(station_id)
    }

    pub fn as_map(&self) -> &HashMap<String, Favorite> {
        &self.favorites
    }

    pub fn as_map_mut(&mut self) -> &mut HashMap<String, Favorite> {
        &mut self.favorites
    }
}

/// Toggle favorite request payload
#[derive(Debug, Serialize, Deserialize)]
pub struct ToggleFavoriteRequest {
    #[serde(alias = "stationuuid")]
    pub station_id: String,
    pub name: String,
    pub favicon: Option<String>,
    pub url: Option<String>,
    pub url_resolved: Option<String>,
    pub country: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_u32")]
    pub bitrate: Option<u32>,
    pub genre: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_string_or_array")]
    pub tags: Option<String>,
}

/// Update favorite metadata request
#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateFavoriteRequest {
    #[serde(alias = "stationuuid")]
    pub station_id: String,
    pub name: String,
    pub favicon: Option<String>,
    pub url: Option<String>,
    pub url_resolved: Option<String>,
    pub country: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_u32")]
    pub bitrate: Option<u32>,
    pub genre: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_string_or_array")]
    pub tags: Option<String>,
}

/// Favorites response wrapper
#[derive(Debug, Serialize, Deserialize)]
pub struct FavoritesResponse {
    pub favorites: HashMap<String, Favorite>,
}

/// Health check response
#[derive(Debug, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
}

impl HealthResponse {
    pub fn ok() -> Self {
        Self {
            status: "ok".to_string(),
        }
    }
}

/// API error response
#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
}

impl ErrorResponse {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            error: message.into(),
        }
    }
}
