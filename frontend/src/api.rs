//! Typed client for the existing Axum JSON API (`/api/*`, `/health`).
//!
//! All requests use same-origin credentials so the `session_id` cookie set by
//! the backend is sent, exactly like the previous `fetch(..., { credentials:
//! 'same-origin' })` calls.

use gloo_net::http::Request;
use serde::{de::DeserializeOwned, Serialize};
use web_sys::RequestCredentials;

use crate::models::{Country, FavoritePayload, FavoritesResponse, Language, Station, Tag, User};

async fn get_json<T: DeserializeOwned>(path: &str) -> Result<T, String> {
    let response = Request::get(path)
        .credentials(RequestCredentials::SameOrigin)
        .send()
        .await
        .map_err(|e| format!("Request failed: {e}"))?;
    if !response.ok() {
        if response.status() == 401 || response.status() == 403 {
            return Err("unauthorized".to_string());
        }
        return Err(format!("Request failed: {}", response.status()));
    }
    response
        .json()
        .await
        .map_err(|e| format!("Bad response: {e}"))
}

async fn post_json<B: Serialize, T: DeserializeOwned>(path: &str, body: &B) -> Result<T, String> {
    let response = Request::post(path)
        .credentials(RequestCredentials::SameOrigin)
        .json(body)
        .map_err(|e| format!("Bad request: {e}"))?
        .send()
        .await
        .map_err(|e| format!("Request failed: {e}"))?;
    if !response.ok() {
        let status = response.status();
        let message = response
            .json::<serde_json::Value>()
            .await
            .ok()
            .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(str::to_string))
            .unwrap_or_else(|| format!("Request failed: {status}"));
        return Err(message);
    }
    response
        .json()
        .await
        .map_err(|e| format!("Bad response: {e}"))
}

async fn post_empty(path: &str) -> Result<serde_json::Value, String> {
    let response = Request::post(path)
        .credentials(RequestCredentials::SameOrigin)
        .send()
        .await
        .map_err(|e| format!("Request failed: {e}"))?;
    if !response.ok() {
        return Err(format!("Request failed: {}", response.status()));
    }
    response
        .json()
        .await
        .map_err(|e| format!("Bad response: {e}"))
}

fn encode(value: &str) -> String {
    // Minimal percent-encoding for query/path segments.
    let mut out = String::with_capacity(value.len());
    for b in value.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

pub async fn fetch_all_stations() -> Result<Vec<Station>, String> {
    get_json("/api/stations").await
}

pub async fn fetch_popular() -> Result<Vec<Station>, String> {
    get_json("/api/popular").await
}

pub async fn search_stations(query: &str) -> Result<Vec<Station>, String> {
    get_json(&format!("/api/search?q={}", encode(query))).await
}

pub async fn fetch_stations_by_country(country: &str) -> Result<Vec<Station>, String> {
    get_json(&format!("/api/stations?country={}", encode(country))).await
}

pub async fn fetch_stations_by_language(language: &str) -> Result<Vec<Station>, String> {
    get_json(&format!("/api/stations?language={}", encode(language))).await
}

pub async fn fetch_stations_by_tag(tag: &str) -> Result<Vec<Station>, String> {
    get_json(&format!(
        "/api/stations?tag={}",
        encode(&tag.to_lowercase())
    ))
    .await
}

pub async fn fetch_station_by_id(id: &str) -> Result<Station, String> {
    get_json(&format!("/api/station/{}", encode(id))).await
}

pub async fn fetch_countries() -> Result<Vec<Country>, String> {
    get_json("/api/countries").await
}

pub async fn fetch_languages() -> Result<Vec<Language>, String> {
    get_json("/api/languages").await
}

pub async fn fetch_tags() -> Result<Vec<Tag>, String> {
    get_json("/api/tags").await
}

pub async fn fetch_me() -> Result<User, String> {
    get_json("/api/me").await
}

pub async fn login(username: &str, password: &str) -> Result<User, String> {
    post_json(
        "/api/login",
        &serde_json::json!({ "username": username, "password": password }),
    )
    .await
}

pub async fn logout() -> Result<(), String> {
    let _ = post_empty("/api/logout").await;
    Ok(())
}

pub async fn create_user(username: &str, password: &str, role: &str) -> Result<User, String> {
    post_json(
        "/api/users",
        &serde_json::json!({ "username": username, "password": password, "role": role }),
    )
    .await
}

pub async fn change_password(old_password: &str, new_password: &str) -> Result<(), String> {
    let _: serde_json::Value = post_json(
        "/api/change-password",
        &serde_json::json!({ "old_password": old_password, "new_password": new_password }),
    )
    .await?;
    Ok(())
}

pub async fn fetch_favorites() -> Result<FavoritesResponse, String> {
    get_json("/api/favorites").await
}

pub async fn toggle_favorite(payload: &FavoritePayload) -> Result<FavoritesResponse, String> {
    post_json("/api/favorites/toggle", payload).await
}

pub async fn update_favorite(payload: &FavoritePayload) -> Result<FavoritesResponse, String> {
    post_json("/api/favorites/update", payload).await
}
