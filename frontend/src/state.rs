//! Central reactive state: view routing, fetched data, auth, favorites.
//!
//! Data loading mirrors `renderCurrentView` from the old UI. Stale responses
//! are discarded with a generation counter instead of `AbortController`.

use std::collections::{HashMap, HashSet};

use gloo_storage::{LocalStorage, Storage};
use gloo_timers::future::TimeoutFuture;
use leptos::prelude::*;

use crate::api;
use crate::models::{
    favorite_to_station, station_id, CollectionItem, Country, Favorite, Language, Station, Tag,
    User,
};
use crate::player::Player;
use crate::utils::{normalize_view_name, sort_by_name_asc, station_sort_key};

const VIEW_KEY: &str = "radio-browser-plus-last-view";
const SEARCH_DEBOUNCE_MS: u32 = 300;
pub const STATION_REFRESH_MS: u32 = 21_600_000; // 6 hours, as before

/// Action confirmed through the in-app confirm dialog. An enum (rather than a
/// stored closure) keeps the dialog signal `Send + Sync` for view closures.
#[derive(Clone)]
pub enum DialogAction {
    RemoveFavorite { station_id: String },
}

/// In-app modal dialog, replacing the browser's `alert()`/`confirm()`.
#[derive(Clone)]
pub enum AppDialog {
    Alert {
        title: String,
        message: String,
    },
    Confirm {
        title: String,
        message: String,
        confirm_label: String,
        action: DialogAction,
    },
}

#[derive(Clone)]
pub struct AppState {
    pub view: RwSignal<String>,
    pub filter: RwSignal<Option<String>>,
    pub search_query: RwSignal<String>,
    pub stations: RwSignal<Vec<Station>>,
    pub collections: RwSignal<Vec<CollectionItem>>,
    pub collection_kind: RwSignal<String>,
    pub view_title: RwSignal<String>,
    pub favorites: RwSignal<HashMap<String, Favorite>>,
    pub favorites_loaded: RwSignal<bool>,
    pub metadata_refreshed: RwSignal<HashSet<String>>,
    pub user: RwSignal<Option<User>>,
    /// True once the initial session restore (`/api/me` at startup) has
    /// resolved. Until then `user == None` means "unknown", not "logged out",
    /// and must not trigger the login gate.
    pub session_checked: RwSignal<bool>,
    /// Currently open in-app dialog, if any (replaces `alert`/`confirm`).
    pub dialog: RwSignal<Option<AppDialog>>,
    pub player: Player,
    pub login_open: RwSignal<bool>,
    pub create_user_open: RwSignal<bool>,
    pub change_password_open: RwSignal<bool>,
    pub sidebar_open: RwSignal<bool>,
    pub compact_bar: RwSignal<bool>,
    pub resize_tick: RwSignal<u64>,
    view_gen: RwSignal<u64>,
    search_gen: RwSignal<u64>,
}

impl AppState {
    pub fn new() -> Self {
        let player = Player::new();
        Self {
            view: RwSignal::new("all".to_string()),
            filter: RwSignal::new(None),
            search_query: RwSignal::new(String::new()),
            stations: RwSignal::new(Vec::new()),
            collections: RwSignal::new(Vec::new()),
            collection_kind: RwSignal::new(String::new()),
            view_title: RwSignal::new("All stations".to_string()),
            favorites: RwSignal::new(HashMap::new()),
            favorites_loaded: RwSignal::new(false),
            metadata_refreshed: RwSignal::new(HashSet::new()),
            user: RwSignal::new(None),
            session_checked: RwSignal::new(false),
            dialog: RwSignal::new(None),
            player,
            login_open: RwSignal::new(false),
            create_user_open: RwSignal::new(false),
            change_password_open: RwSignal::new(false),
            sidebar_open: RwSignal::new(false),
            compact_bar: RwSignal::new(false),
            resize_tick: RwSignal::new(0),
            view_gen: RwSignal::new(0),
            search_gen: RwSignal::new(0),
        }
    }

    // -- auth helpers ------------------------------------------------------

    pub fn is_logged_in(&self) -> bool {
        self.user.get().is_some()
    }

    pub fn is_admin(&self) -> bool {
        self.user.get().is_some_and(|u| u.role == "admin")
    }

    /// Central handler for authentication loss (rejected session, expiry):
    /// drop local identity and block the UI behind the login dialog.
    pub fn handle_unauthorized(&self) {
        self.session_checked.set(true);
        self.user.set(None);
        self.favorites.set(Default::default());
        self.favorites_loaded.set(false);
        self.stations.set(Vec::new());
        self.collections.set(Vec::new());
        self.login_open.set(true);
    }

    /// Route API failures: an auth rejection locks the UI, anything else is
    /// only logged (the backend already clamps/validates inputs).
    fn api_failed(&self, error: String) {
        if error == "unauthorized" {
            self.handle_unauthorized();
        } else {
            web_sys::console::error_1(&error.into());
        }
    }

    pub fn show_alert(&self, title: impl Into<String>, message: impl Into<String>) {
        self.dialog.set(Some(AppDialog::Alert {
            title: title.into(),
            message: message.into(),
        }));
    }

    pub fn close_dialog(&self) {
        self.dialog.set(None);
    }

    fn show_confirm_remove_favorite(&self, station_id: String, station_name: &str) {
        self.dialog.set(Some(AppDialog::Confirm {
            title: "Remove favorite".to_string(),
            message: format!("Remove \"{station_name}\" from favorites?"),
            confirm_label: "Remove".to_string(),
            action: DialogAction::RemoveFavorite { station_id },
        }));
    }

    /// Runs the confirmed dialog action, then closes the dialog.
    pub fn confirm_dialog(&self) {
        let Some(AppDialog::Confirm { action, .. }) = self.dialog.get_untracked() else {
            return;
        };
        self.dialog.set(None);
        match action {
            DialogAction::RemoveFavorite { station_id: id } => {
                let station = self
                    .stations
                    .get_untracked()
                    .into_iter()
                    .find(|s| station_id(s).as_deref() == Some(id.as_str()));
                if let Some(station) = station {
                    let this = self.clone();
                    leptos::task::spawn_local(async move {
                        this.toggle_favorite_now(station).await;
                    });
                }
            }
        }
    }

    // -- favorites ---------------------------------------------------------

    pub async fn ensure_favorites(&self) {
        if self.favorites_loaded.get_untracked() || !self.is_logged_in() {
            return;
        }
        match api::fetch_favorites().await {
            Ok(resp) => {
                self.favorites.set(resp.favorites);
                self.favorites_loaded.set(true);
            }
            Err(e) => {
                if e == "unauthorized" {
                    self.handle_unauthorized();
                }
            }
        }
    }

    pub fn is_favorite(&self, id: &str) -> bool {
        self.favorites.get().contains_key(id)
    }

    pub fn toggle_favorite(&self, station: Station) {
        if !self.is_logged_in() {
            self.login_open.set(true);
            return;
        }
        let Some(id) = station_id(&station) else {
            return;
        };
        if self.favorites.get_untracked().contains_key(&id) {
            // Removal needs confirmation; the in-app dialog continues in
            // `confirm_dialog` once the user confirms.
            self.show_confirm_remove_favorite(id, &station.name);
            return;
        }
        let this = self.clone();
        leptos::task::spawn_local(async move {
            this.toggle_favorite_now(station).await;
        });
    }

    async fn toggle_favorite_now(&self, station: Station) {
        let genre = crate::utils::station_genre(&station);
        let genre = (genre != "Unknown genre").then_some(genre);
        let Some(payload) = crate::models::FavoritePayload::from_station(&station, genre) else {
            return;
        };
        match api::toggle_favorite(&payload).await {
            Ok(resp) => {
                self.favorites.set(resp.favorites);
                self.favorites_loaded.set(true);
                if self.view.get_untracked() == "favorites" && self.filter.get_untracked().is_none()
                {
                    self.load_view();
                }
            }
            Err(e) => {
                if e == "unauthorized" {
                    self.handle_unauthorized();
                } else {
                    self.show_alert("Favorites", e);
                }
            }
        }
    }

    pub fn refresh_favorite_metadata(&self, station: Station) {
        let this = self.clone();
        leptos::task::spawn_local(async move {
            let Some(id) = station_id(&station) else {
                return;
            };
            if !this.favorites.get_untracked().contains_key(&id) {
                return;
            }
            if this.metadata_refreshed.get_untracked().contains(&id) {
                return;
            }
            this.metadata_refreshed.update(|s| {
                s.insert(id.clone());
            });
            let live = match api::fetch_station_by_id(&id).await {
                Ok(live) => live,
                Err(e) => {
                    this.api_failed(e);
                    return;
                }
            };
            let genre = crate::utils::station_genre(&live);
            if genre == "Unknown genre" {
                return;
            }
            let payload = crate::models::FavoritePayload {
                station_id: id.clone(),
                name: live.name.clone(),
                favicon: live.favicon.clone(),
                url: live.url_resolved.clone().or_else(|| live.url.clone()),
                url_resolved: live.url_resolved.clone().or_else(|| live.url.clone()),
                country: live.country.clone(),
                bitrate: live.bitrate,
                genre: Some(genre),
                tags: live.tags.clone(),
            };
            if let Ok(resp) = api::update_favorite(&payload).await {
                this.favorites.set(resp.favorites);
            }
        });
    }

    // -- routing -----------------------------------------------------------

    pub fn persist_view(&self, view: &str) {
        let _ = LocalStorage::set(VIEW_KEY, view);
    }

    pub fn navigate(&self, view: &str, filter: Option<String>) {
        let view = normalize_view_name(view);
        let mut hash = format!("#{view}");
        if let Some(f) = &filter {
            if view != "all" {
                hash.push('/');
                hash.push_str(&percent_encode(f));
            }
        }
        self.persist_view(&view);
        if let Some(window) = web_sys::window() {
            let current = window.location().hash().unwrap_or_default();
            if current != hash {
                let _ = window.location().set_hash(&hash);
                return; // hashchange listener will load the view
            }
        }
        // Hash unchanged (e.g. re-clicking the active nav): load directly.
        self.view.set(view);
        self.filter.set(filter);
        self.load_view();
    }

    pub fn apply_hash(&self) {
        let hash = web_sys::window()
            .and_then(|w| w.location().hash().ok())
            .unwrap_or_default();
        let hash = hash.strip_prefix('#').unwrap_or("");
        if hash.is_empty() {
            let saved: String = LocalStorage::get(VIEW_KEY).unwrap_or_else(|_| "all".to_string());
            self.view.set(normalize_view_name(&saved));
            self.filter.set(None);
            self.load_view();
            return;
        }
        let mut parts = hash.splitn(2, '/');
        let view = normalize_view_name(parts.next().unwrap_or(""));
        let mut filter = parts.next().map(percent_decode).filter(|f| !f.is_empty());
        if view == "all" {
            filter = None;
        }
        self.view.set(view);
        self.filter.set(filter);
        self.load_view();
    }

    // -- data loading ------------------------------------------------------

    /// Load the current view; stale in-flight responses are ignored.
    /// Without a session there is nothing to load: the backend rejects every
    /// data request, so clear the grids instead of firing doomed fetches.
    /// While the session restore is still in flight the login state is
    /// unknown, so skip quietly (the restore triggers a load when done)
    /// instead of flashing the login dialog on every page load.
    pub fn load_view(&self) {
        if !self.session_checked.get_untracked() {
            return;
        }
        if !self.is_logged_in() {
            self.stations.set(Vec::new());
            self.collections.set(Vec::new());
            self.login_open.set(true);
            return;
        }
        self.view_gen.update(|g| *g += 1);
        let gen = self.view_gen.get_untracked();
        let this = self.clone();
        leptos::task::spawn_local(async move {
            this.load_view_inner().await;
            let _ = gen;
        });
    }

    fn current_gen(&self, gen: u64) -> bool {
        self.view_gen.get_untracked() == gen
    }

    async fn load_view_inner(&self) {
        let gen = self.view_gen.get_untracked();
        let view = self.view.get_untracked();
        let filter = self.filter.get_untracked();

        if view == "search" {
            let query = self.search_query.get_untracked();
            if query.trim().is_empty() {
                self.view.set("popular".to_string());
                self.load_view();
                return;
            }
            match api::search_stations(&query).await {
                Ok(stations) => {
                    if self.current_gen(gen)
                        && self.view.get_untracked() == "search"
                        && self.search_query.get_untracked() == query
                    {
                        self.stations.set(stations);
                        self.view_title.set(format!("Search: {query}"));
                    }
                }
                Err(e) => self.api_failed(e),
            }
            return;
        }

        if let Some(filter_value) = filter {
            let result = match view.as_str() {
                "countries" => api::fetch_stations_by_country(&filter_value).await,
                "languages" => api::fetch_stations_by_language(&filter_value).await,
                // Raw tag deep links keep working; the Genres UI itself uses
                // curated buckets (canonical names + Variety).
                "tags" => api::fetch_stations_by_tag(&filter_value).await,
                "genres" => api::fetch_stations_by_genre(&filter_value).await,
                _ => Ok(Vec::new()),
            };
            match result {
                Ok(mut stations) => {
                    if view == "genres" {
                        sort_by_name_asc(&mut stations, station_sort_key);
                    }
                    if self.current_gen(gen) && self.filter.get_untracked().is_some() {
                        let title = match view.as_str() {
                            "genres" => "Genres".to_string(),
                            v => {
                                let mut c = v.chars();
                                format!("{}{}", c.next().unwrap_or('?').to_uppercase(), c.as_str())
                            }
                        };
                        self.stations.set(stations);
                        self.view_title.set(format!("{title}: {filter_value}"));
                    }
                }
                Err(e) => self.api_failed(e),
            }
            return;
        }

        match view.as_str() {
            "all" => {
                self.show_station_list(
                    gen,
                    Some("all"),
                    "All stations",
                    true,
                    api::fetch_all_stations(),
                )
                .await
            }
            "countries" => {
                self.show_collections(
                    gen,
                    Some("countries"),
                    "countries",
                    "Countries",
                    api::fetch_countries(),
                )
                .await
            }
            "languages" => {
                self.show_collections(
                    gen,
                    Some("languages"),
                    "languages",
                    "Languages",
                    api::fetch_languages(),
                )
                .await
            }
            "tags" | "genres" => {
                self.show_collections(gen, None, "genres", "Genres", api::fetch_genres())
                    .await
            }
            "favorites" => {
                self.ensure_favorites().await;
                if self.current_gen(gen) && self.view.get_untracked() == "favorites" {
                    let mut stations: Vec<Station> = self
                        .favorites
                        .get_untracked()
                        .iter()
                        .map(|(id, fav)| favorite_to_station(id, fav))
                        .collect();
                    sort_by_name_asc(&mut stations, station_sort_key);
                    self.stations.set(stations);
                    self.view_title.set("Favorites".to_string());
                }
            }
            _ => {
                self.show_station_list(gen, None, "Popular", false, api::fetch_popular())
                    .await
            }
        }
    }

    /// Fetch a station list for a top-level view and display it if still
    /// current. `expected_view` additionally requires the view to still match
    /// (`None` = generation check only, as before).
    async fn show_station_list(
        &self,
        gen: u64,
        expected_view: Option<&str>,
        title: &str,
        sort: bool,
        fetch: impl std::future::Future<Output = Result<Vec<Station>, String>>,
    ) {
        match fetch.await {
            Ok(mut stations) => {
                if sort {
                    sort_by_name_asc(&mut stations, station_sort_key);
                }
                let current = self.current_gen(gen)
                    && expected_view.is_none_or(|v| self.view.get_untracked() == v);
                if current {
                    self.stations.set(stations);
                    self.view_title.set(title.to_string());
                }
            }
            Err(e) => self.api_failed(e),
        }
    }

    /// Fetch a collection list (countries/languages/genres) and display it if
    /// still current. Same staleness contract as [`Self::show_station_list`].
    async fn show_collections<T: CollectionRow>(
        &self,
        gen: u64,
        expected_view: Option<&str>,
        kind: &str,
        title: &str,
        fetch: impl std::future::Future<Output = Result<Vec<T>, String>>,
    ) {
        match fetch.await {
            Ok(items) => {
                let current = self.current_gen(gen)
                    && expected_view.is_none_or(|v| self.view.get_untracked() == v);
                if current {
                    let kind = kind.to_string();
                    self.collections.set(to_collection(items, &kind));
                    self.collection_kind.set(kind);
                    self.view_title.set(title.to_string());
                }
            }
            Err(e) => self.api_failed(e),
        }
    }

    // -- search ------------------------------------------------------------

    pub fn apply_search(&self, query: String) {
        let trimmed = query.trim().to_string();
        self.search_query.set(trimmed.clone());
        if trimmed.is_empty() {
            self.view.set("popular".to_string());
            self.view_title.set("Popular".to_string());
            self.load_view();
            return;
        }
        self.view.set("search".to_string());
        self.search_gen.update(|g| *g += 1);
        let gen = self.search_gen.get_untracked();
        let this = self.clone();
        leptos::task::spawn_local(async move {
            TimeoutFuture::new(SEARCH_DEBOUNCE_MS).await;
            if this.search_gen.get_untracked() != gen {
                return;
            }
            match api::search_stations(&trimmed).await {
                Ok(stations) => {
                    if this.view.get_untracked() == "search"
                        && this.search_query.get_untracked() == trimmed
                    {
                        this.stations.set(stations);
                        this.view_title.set(format!("Search: {trimmed}"));
                    }
                }
                Err(e) => this.api_failed(e),
            }
        });
    }
}

trait CollectionRow {
    fn label(&self) -> String;
    fn count(&self) -> Option<u32>;
    fn iso(&self) -> Option<String>;
}

impl CollectionRow for Country {
    fn label(&self) -> String {
        self.name.clone()
    }
    fn count(&self) -> Option<u32> {
        self.stationcount
    }
    fn iso(&self) -> Option<String> {
        self.iso_3166_1.clone()
    }
}

impl CollectionRow for Language {
    fn label(&self) -> String {
        self.name.clone()
    }
    fn count(&self) -> Option<u32> {
        self.stationcount
    }
    fn iso(&self) -> Option<String> {
        None
    }
}

impl CollectionRow for Tag {
    fn label(&self) -> String {
        self.name.clone()
    }
    fn count(&self) -> Option<u32> {
        self.stationcount
    }
    fn iso(&self) -> Option<String> {
        None
    }
}

fn to_collection<T: CollectionRow>(mut items: Vec<T>, kind: &str) -> Vec<CollectionItem> {
    sort_by_name_asc(&mut items, |i| i.label());
    items
        .into_iter()
        .map(|item| {
            let label = if item.label().trim().is_empty() {
                "Unknown".to_string()
            } else {
                item.label()
            };
            let icon_url = if kind == "countries" {
                item.iso()
                    .map(|iso| format!("/flags/{}.svg", iso.to_lowercase()))
                    .unwrap_or_else(|| crate::utils::PLACEHOLDER_SVG.to_string())
            } else {
                crate::utils::PLACEHOLDER_SVG.to_string()
            };
            CollectionItem {
                filter_value: label.clone(),
                label,
                count: item.count(),
                icon_url,
            }
        })
        .collect()
}

fn percent_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for b in value.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                out.push(h << 4 | l);
                i += 3;
                continue;
            }
        }
        if bytes[i] == b'+' {
            out.push(b' ');
        } else {
            out.push(bytes[i]);
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}
