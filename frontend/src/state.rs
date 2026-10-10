//! Central reactive state: view routing, fetched data, auth, favorites.
//!
//! Data loading mirrors `renderCurrentView` from the old UI. Stale responses
//! are discarded with a generation counter instead of `AbortController`.

use std::collections::{HashMap, HashSet};

use gloo_storage::{LocalStorage, Storage};
use gloo_timers::future::TimeoutFuture;
use leptos::prelude::*;
use wasm_bindgen::JsCast;

use crate::api;
use crate::models::{
    favorite_to_station, station_id, CollectionItem, Country, Favorite, Language, Station, Tag,
    User,
};
use crate::player::Player;
use crate::utils::{
    normalize_view_name, sort_by_name_asc, sort_stations, station_sort_key, title_for_view,
    SortMode,
};

const VIEW_KEY: &str = "radio-browser-plus-last-view";
const SORT_KEY: &str = "radio-browser-plus-sort";
const SEARCH_DEBOUNCE_MS: u32 = 300;
pub const STATION_REFRESH_MS: u32 = 21_600_000; // 6 hours, as before
/// Automatic retries for a failed view load before the error is surfaced.
/// Big catalog fetches occasionally stall or fail upstream; retrying keeps a
/// flaky response from stranding the grid until the user hammers the tabs.
const LOAD_RETRIES: u8 = 2;
const RETRY_DELAY_MS: u32 = 2000;
/// Grid rows mounted at once. The backend can return thousands of stations;
/// rendering them all mounts thousands of cards, images and reactive scopes,
/// which blows up tab memory and scroll performance. The grids render at
/// most this many and grow it as the user scrolls (infinite scroll).
pub const GRID_PAGE: usize = 60;

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
    /// List the current station was started from, in display order. Previous /
    /// next (UI buttons, keyboard, OS media keys) step within this list, so a
    /// station started from Favorites stays in Favorites even after the
    /// visible view changes. Captured on every direct (card) play.
    pub queue: RwSignal<Vec<Station>>,
    pub collections: RwSignal<Vec<CollectionItem>>,
    pub collection_kind: RwSignal<String>,
    pub view_title: RwSignal<String>,
    /// True while a view/search fetch is in flight (skeletons + toolbar
    /// progress indicator).
    pub loading: RwSignal<bool>,
    /// Last view-load failure, if the current view failed and retries ran
    /// out. The grids render it with a Retry action instead of failing
    /// silently (failures used to be console-only, leaving a stale grid
    /// under a stale title that looked like the tab switch never happened).
    pub view_error: RwSignal<Option<String>>,
    /// Display order applied on top of the fetched list; persisted locally.
    pub sort: RwSignal<SortMode>,
    /// Cap on mounted grid rows; grows via [`Self::show_more`], resets on
    /// navigation (not on idle reloads, which keep the user's place).
    pub grid_limit: RwSignal<usize>,
    /// Bottom spacer height (px) reserving the not-yet-mounted rows, so the
    /// scrollbar reflects the full list from the first page instead of
    /// growing while scrolling. Recomputed by [`Self::update_spacer`].
    pub spacer_px: RwSignal<usize>,
    /// Scroll-bottom position (px) at which the next page mounts. Tracks the
    /// end of the mounted rows rather than the scroll bottom, because the
    /// spacer below them is empty space that must not trigger loads.
    pub load_more_at: RwSignal<i32>,
    pub favorites: RwSignal<HashMap<String, Favorite>>,
    pub favorites_loaded: RwSignal<bool>,
    pub metadata_refreshed: RwSignal<HashSet<String>>,
    pub user: RwSignal<Option<User>>,
    /// True once the initial session restore (`/api/me` at startup) has
    /// resolved. Until then `user == None` means "unknown", not "logged out",
    /// and must not trigger the login gate.
    pub session_checked: RwSignal<bool>,
    /// True when the server allows anonymous browsing (`ALLOW_GUEST_ACCESS`).
    /// Fetched once at startup from the public `/api/config`.
    pub allow_guest: RwSignal<bool>,
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
        let sort = LocalStorage::get::<String>(SORT_KEY)
            .map(|key| SortMode::from_key(&key))
            .unwrap_or(SortMode::Featured);
        Self {
            view: RwSignal::new("all".to_string()),
            filter: RwSignal::new(None),
            search_query: RwSignal::new(String::new()),
            stations: RwSignal::new(Vec::new()),
            queue: RwSignal::new(Vec::new()),
            collections: RwSignal::new(Vec::new()),
            collection_kind: RwSignal::new(String::new()),
            view_title: RwSignal::new("All stations".to_string()),
            loading: RwSignal::new(false),
            view_error: RwSignal::new(None),
            sort: RwSignal::new(sort),
            grid_limit: RwSignal::new(GRID_PAGE),
            spacer_px: RwSignal::new(0),
            load_more_at: RwSignal::new(i32::MAX),
            favorites: RwSignal::new(HashMap::new()),
            favorites_loaded: RwSignal::new(false),
            metadata_refreshed: RwSignal::new(HashSet::new()),
            user: RwSignal::new(None),
            session_checked: RwSignal::new(false),
            allow_guest: RwSignal::new(false),
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
    /// drop local identity and block the UI behind the login dialog — unless
    /// guest browsing is allowed, in which case continue anonymously.
    pub fn handle_unauthorized(&self) {
        self.session_checked.set(true);
        self.user.set(None);
        self.favorites.set(Default::default());
        self.favorites_loaded.set(false);
        self.stations.set(Vec::new());
        self.queue.set(Vec::new());
        self.collections.set(Vec::new());
        self.loading.set(false);
        self.view_error.set(None);
        // Guest mode degrades to anonymous browsing instead of forcing sign-in.
        self.login_open.set(!self.allow_guest.get_untracked());
    }

    /// Route API failures: an auth rejection locks the UI, anything else is
    /// only logged (the backend already clamps/validates inputs). Used for
    /// background work; view loads go through [`Self::note_view_error`] so
    /// failures surface in the UI with a Retry action.
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

    // -- playback queue (previous / next) ----------------------------------

    /// Direct play from a station card: capture the visible list as the
    /// navigation context, then start playback. The queue preserves display
    /// order, so previous / next follow what the user saw.
    pub fn play_from_visible_list(&self, station: Station) {
        let visible = self.stations.get_untracked();
        if !visible.is_empty() {
            self.queue.set(visible);
        }
        self.play_queued(station);
    }

    /// Step to the next station in the originating list (wraps around).
    pub fn play_next(&self) {
        self.step_queue(1);
    }

    /// Step to the previous station in the originating list (wraps around).
    pub fn play_previous(&self) {
        self.step_queue(-1);
    }

    /// Fresh connection on the current stream: used when the OS reports
    /// connectivity back after an outage. No-op when idle.
    pub fn reconnect(&self) {
        self.player.reconnect();
    }

    /// Whether a previous / next step has anywhere to go (2+ stations in the
    /// effective list). Uses tracked reads so views re-render on change.
    pub fn has_prev_next(&self) -> bool {
        let queued = self.queue.get();
        let current_id = self.player.current.get().as_ref().and_then(station_id);
        if let Some(ref id) = current_id {
            if queued.iter().any(|s| station_id(s).as_deref() == Some(id)) {
                return queued.len() > 1;
            }
            let visible = self.stations.get();
            if visible.iter().any(|s| station_id(s).as_deref() == Some(id)) {
                return visible.len() > 1;
            }
        }
        if !queued.is_empty() {
            return queued.len() > 1;
        }
        self.stations.get().len() > 1
    }

    /// The list a prev / next step would walk: the captured queue when it
    /// still contains the current station, otherwise the visible list (the
    /// view may have changed or refreshed since playback started).
    fn effective_queue(&self) -> Vec<Station> {
        let queued = self.queue.get_untracked();
        let current_id = self
            .player
            .current
            .get_untracked()
            .as_ref()
            .and_then(station_id);
        if let Some(ref id) = current_id {
            if queued.iter().any(|s| station_id(s).as_deref() == Some(id)) {
                return queued;
            }
            let visible = self.stations.get_untracked();
            if visible.iter().any(|s| station_id(s).as_deref() == Some(id)) {
                return visible;
            }
        }
        if !queued.is_empty() {
            return queued;
        }
        self.stations.get_untracked()
    }

    fn step_queue(&self, delta: isize) {
        let list = self.effective_queue();
        let Some(next) = pick_neighbor(&list, self.current_station_id().as_deref(), delta) else {
            return;
        };
        // A view change may have re-resolved the queue: keep the adopted list
        // so repeated steps stay in the same context.
        if self.queue.get_untracked().is_empty()
            || self
                .current_station_id()
                .is_none_or(|id| !queue_contains(&self.queue.get_untracked(), &id))
        {
            self.queue.set(list);
        }
        self.play_queued(next);
    }

    fn current_station_id(&self) -> Option<String> {
        self.player.current.get_untracked().as_ref().and_then(station_id)
    }

    fn play_queued(&self, station: Station) {
        let id = station_id(&station);
        self.player.play(station.clone());
        if id.is_some_and(|sid| self.favorites.get_untracked().contains_key(&sid)) {
            self.refresh_favorite_metadata(station);
        }
    }

    // -- routing -----------------------------------------------------------

    pub fn persist_view(&self, view: &str) {
        let _ = LocalStorage::set(VIEW_KEY, view);
    }

    /// Change the display sort mode and remember it across sessions.
    pub fn set_sort(&self, mode: SortMode) {
        self.sort.set(mode);
        let _ = LocalStorage::set(SORT_KEY, mode.key());
    }

    /// Fetched stations with the user's sort mode applied, for display.
    pub fn display_stations(&self) -> Vec<Station> {
        let mut stations = self.stations.get();
        sort_stations(&mut stations, self.sort.get());
        stations
    }

    /// Rows the active grid displays: collections for the entry views
    /// (Countries / Languages / Genres), stations for everything else.
    fn grid_total(&self) -> usize {
        let collection = matches!(
            self.view.get_untracked().as_str(),
            "countries" | "languages" | "tags" | "genres"
        ) && self.filter.get_untracked().is_none();
        if collection {
            self.collections.get_untracked().len()
        } else {
            self.stations.get_untracked().len()
        }
    }

    /// Render the next grid page, if any rows are still hidden. A no-op once
    /// everything is visible, so scroll handlers can call it on every scroll
    /// event without churning signals or re-rendering the grid.
    pub fn show_more(&self) {
        let total = self.grid_total();
        let limit = self.grid_limit.get_untracked();
        if limit < total {
            self.grid_limit.set((limit + GRID_PAGE).min(total));
        }
    }

    /// Recompute the bottom spacer and the load threshold after renders that
    /// change the window, the totals, or the viewport geometry. Only mounted
    /// rows load images and cost layout; the spacer below them is empty
    /// reserved height, so off-screen content stays unloaded while the
    /// scrollbar is stable from the first paint.
    ///
    /// Safe to call from render effects: both signals are change-guarded,
    /// and the catch-up chain below strictly grows `grid_limit` toward
    /// `total`, so it always terminates.
    pub(crate) fn update_spacer(&self) {
        let total = self.grid_total();
        let shown = self.grid_limit.get_untracked().min(total);
        let Some(document) = web_sys::window().and_then(|w| w.document()) else {
            return;
        };
        let Some(grid) = document.get_element_by_id("station-grid") else {
            return;
        };
        if shown >= total {
            if self.spacer_px.get_untracked() != 0 {
                self.spacer_px.set(0);
            }
            if self.load_more_at.get_untracked() != i32::MAX {
                self.load_more_at.set(i32::MAX);
            }
            return;
        }
        // Geometry from the mounted rows: column count from the grid width
        // and the first card, row height from the tallest sampled card (an
        // overestimate only shrinks the track once, at the very end).
        let gap = element_gap_px(&grid);
        let grid_w = grid.client_width() as f64;
        let cards = grid
            .query_selector_all(".station-card, .collection-card")
            .ok();
        let mut card_w = 0.0f64;
        let mut card_h = 0.0f64;
        if let Some(list) = cards {
            for i in 0..list.length().min(12) {
                if let Some(node) = list.item(i) {
                    if let Ok(card) = node.dyn_into::<web_sys::HtmlElement>() {
                        card_w = card_w.max(card.offset_width() as f64);
                        card_h = card_h.max(card.offset_height() as f64);
                    }
                }
            }
        }
        if card_w <= 0.0 || card_h <= 0.0 || grid_w <= 0.0 {
            return; // Nothing measurable yet; keep the previous reservation.
        }
        let cols = ((grid_w + gap) / (card_w + gap)).round().max(1.0) as usize;
        let row_h = card_h + gap;
        let rows_remaining = total.saturating_sub(shown).div_ceil(cols);
        let spacer = (rows_remaining as f64 * row_h) as usize;
        // Exact end of the mounted rows: the footer sits directly below
        // them, so its far edge in grid coordinates is the load threshold.
        // (Derived from the footer — never from the scroll height, which
        // already contains the previous spacer and would feed back into
        // runaway growth.) `.station-grid` is positioned so the grid is the
        // footer's offset parent and the coordinates match the scroll ones.
        let threshold = grid
            .query_selector(".grid-footer")
            .ok()
            .flatten()
            .and_then(|el| el.dyn_into::<web_sys::HtmlElement>().ok())
            .map(|footer| footer.offset_top() + footer.offset_height())
            .unwrap_or(i32::MAX);
        if self.spacer_px.get_untracked() != spacer {
            self.spacer_px.set(spacer);
        }
        if self.load_more_at.get_untracked() != threshold {
            self.load_more_at.set(threshold);
        }
        // Dragging the scrollbar deep into the spacer must fill the window
        // instead of parking on empty space: keep growing while the viewport
        // is already past the mounted rows. Each step re-renders and returns
        // here, so a long jump streams pages in over a few frames.
        if grid.scroll_top() + grid.client_height() + 1600 >= threshold {
            self.show_more();
        }
    }

    /// Reset the grid window for a new result set: remount from the first
    /// page and scroll back to the top. The scroll container survives view
    /// switches, so without the scroll reset a new view would open mid-list
    /// (and the infinite-scroll handler would immediately regrow the
    /// window). Only navigation calls this — background reloads keep the
    /// user's scroll position.
    fn reset_grid_window(&self) {
        // Idempotent: navigation entry points overlap (navigate + the
        // hashchange it triggers) and search runs per keystroke, so never
        // write signals or touch the DOM when already at the first page.
        if self.grid_limit.get_untracked() != GRID_PAGE {
            self.grid_limit.set(GRID_PAGE);
        }
        if let Some(grid) = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id("station-grid"))
        {
            if grid.scroll_top() != 0 {
                grid.set_scroll_top(0);
            }
        }
    }

    pub fn navigate(&self, view: &str, filter: Option<String>) {
        self.reset_grid_window();
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
        self.reset_grid_window();
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
    /// Without a session there is nothing to load (unless guest browsing is
    /// allowed): the backend rejects every data request, so clear the grids
    /// instead of firing doomed fetches.
    /// While the session restore is still in flight the login state is
    /// unknown, so skip quietly (the restore triggers a load when done)
    /// instead of flashing the login dialog on every page load.
    pub fn load_view(&self) {
        if !self.session_checked.get_untracked() {
            return;
        }
        if !self.is_logged_in() && !self.allow_guest.get_untracked() {
            self.stations.set(Vec::new());
            self.collections.set(Vec::new());
            self.loading.set(false);
            self.view_error.set(None);
            self.login_open.set(true);
            return;
        }
        self.start_view_load(LOAD_RETRIES);
    }

    /// (Re)load the current view with up to `retries` automatic retries on
    /// failure. The title and error slot reset up-front: the header tracks
    /// the requested view instantly, and a previous failure never lingers
    /// under a new attempt.
    fn start_view_load(&self, retries: u8) {
        let view = self.view.get_untracked();
        let filter = self.filter.get_untracked();
        let query = self.search_query.get_untracked();
        self.view_title
            .set(title_for_view(&view, filter.as_deref(), &query));
        self.view_error.set(None);
        self.loading.set(true);
        self.view_gen.update(|g| *g += 1);
        let gen = self.view_gen.get_untracked();
        let this = self.clone();
        leptos::task::spawn_local(async move {
            this.load_view_inner().await;
            // Only the newest load clears the spinner; a superseded one
            // leaves it to its successor.
            if this.current_gen(gen) {
                this.loading.set(false);
                // A failed attempt used to strand the grid with nothing but
                // a console message: retry while the user is still on this
                // view (a newer navigation abandons the retry below), else
                // the error slot stays set and the grids offer a Retry.
                if this.view_error.get_untracked().is_some() && retries > 0 {
                    let retry = this.clone();
                    leptos::task::spawn_local(async move {
                        TimeoutFuture::new(RETRY_DELAY_MS).await;
                        if retry.current_gen(gen) {
                            retry.start_view_load(retries - 1);
                        }
                    });
                }
            }
        });
    }

    /// Record a view-load failure for the UI (grid error block / banner).
    /// Auth rejections always lock the UI; anything else is retryable, but
    /// only the current attempt owns the error slot — a superseded attempt
    /// must not paint errors over its successor's grid (`current` is the
    /// generation check of the attempt that failed).
    fn note_view_error(&self, error: String, current: bool) {
        if error == "unauthorized" {
            self.handle_unauthorized();
            return;
        }
        web_sys::console::error_1(&error.clone().into());
        if current {
            self.view_error.set(Some(friendly_load_error(&error)));
        }
    }

    /// Dismiss the load-error banner; the stale rows stay until the next
    /// load replaces them.
    pub fn dismiss_error(&self) {
        self.view_error.set(None);
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
                Err(e) => self.note_view_error(e, self.current_gen(gen)),
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
                Err(e) => self.note_view_error(e, self.current_gen(gen)),
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
            Err(e) => self.note_view_error(e, self.current_gen(gen)),
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
            Err(e) => self.note_view_error(e, self.current_gen(gen)),
        }
    }

    // -- search ------------------------------------------------------------

    pub fn apply_search(&self, query: String) {
        self.reset_grid_window();
        // A new keystroke is a new attempt: drop any previous search error.
        if self.view_error.get_untracked().is_some() {
            self.view_error.set(None);
        }
        let trimmed = query.trim().to_string();
        self.search_query.set(trimmed.clone());
        if trimmed.is_empty() {
            self.view.set("popular".to_string());
            self.view_title.set("Popular".to_string());
            self.load_view();
            return;
        }
        self.view.set("search".to_string());
        self.view_title
            .set(title_for_view("search", None, &trimmed));
        self.loading.set(true);
        // Abandon any in-flight view load: the debounced search below owns
        // the spinner and the error slot from here on.
        self.view_gen.update(|g| *g += 1);
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
                    if this.search_gen.get_untracked() == gen {
                        this.loading.set(false);
                    }
                }
                Err(e) => {
                    if this.search_gen.get_untracked() == gen {
                        this.loading.set(false);
                        this.note_view_error(e, true);
                    } else {
                        // A superseded keystroke owns nothing, except auth
                        // loss which must always lock the UI.
                        this.api_failed(e);
                    }
                }
            }
        });
    }
}

/// Computed `gap` of the station grid in px (falls back to the desktop
/// token when unreadable).
fn element_gap_px(el: &web_sys::Element) -> f64 {
    web_sys::window()
        .and_then(|w| w.get_computed_style(el).ok().flatten())
        .and_then(|s| s.get_property_value("gap").ok())
        .and_then(|v| {
            v.split_whitespace()
                .next()
                .and_then(|t| t.trim_end_matches("px").parse::<f64>().ok())
        })
        .unwrap_or(16.0)
}

/// Human-readable load failure for the error banner / error block. Raw API
/// errors ("Request failed: 408") mean nothing to most users.
fn friendly_load_error(error: &str) -> String {
    if error.contains("408") {
        "The radio catalog took too long to respond.".to_string()
    } else if error.starts_with("Request failed") {
        "Couldn't reach the radio catalog. Check your connection.".to_string()
    } else if error.starts_with("Bad response") {
        "The radio catalog returned an unexpected response.".to_string()
    } else {
        error.to_string()
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

/// Previous / next pick within a station list, with wraparound. Returns
/// `None` for an empty list or a single-station list already playing (stepping
/// would only reconnect the same stream).
fn pick_neighbor(
    list: &[Station],
    current_id: Option<&str>,
    delta: isize,
) -> Option<Station> {
    if list.is_empty() {
        return None;
    }
    let len = list.len() as isize;
    match current_id.and_then(|id| list.iter().position(|s| station_id(s).as_deref() == Some(id))) {
        Some(index) => {
            if list.len() == 1 {
                return None;
            }
            let next = (index as isize + delta).rem_euclid(len) as usize;
            list.get(next).cloned()
        }
        // Current station unknown (stale queue, cleared player): start at the
        // head for next, at the tail for previous.
        None => {
            let next = if delta >= 0 { 0 } else { (len - 1) as usize };
            list.get(next).cloned()
        }
    }
}

fn queue_contains(list: &[Station], id: &str) -> bool {
    list.iter().any(|s| station_id(s).as_deref() == Some(id))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn station_with_id(id: &str) -> Station {
        Station {
            stationuuid: id.to_string(),
            name: id.to_string(),
            ..Station::default()
        }
    }

    fn list(ids: &[&str]) -> Vec<Station> {
        ids.iter().map(|id| station_with_id(id)).collect()
    }

    #[test]
    fn neighbor_steps_with_wraparound() {
        let stations = list(&["a", "b", "c"]);
        assert_eq!(pick_neighbor(&stations, Some("a"), 1).unwrap().name, "b");
        assert_eq!(pick_neighbor(&stations, Some("c"), 1).unwrap().name, "a");
        assert_eq!(pick_neighbor(&stations, Some("a"), -1).unwrap().name, "c");
        assert_eq!(pick_neighbor(&stations, Some("b"), -1).unwrap().name, "a");
    }

    #[test]
    fn neighbor_without_current_starts_at_head_or_tail() {
        let stations = list(&["a", "b"]);
        assert_eq!(pick_neighbor(&stations, None, 1).unwrap().name, "a");
        assert_eq!(pick_neighbor(&stations, None, -1).unwrap().name, "b");
        assert_eq!(pick_neighbor(&stations, Some("missing"), 1).unwrap().name, "a");
    }

    #[test]
    fn neighbor_empty_or_singleton() {
        assert!(pick_neighbor(&[], Some("a"), 1).is_none());
        let one = list(&["a"]);
        assert!(pick_neighbor(&one, Some("a"), 1).is_none());
        assert!(pick_neighbor(&one, Some("a"), -1).is_none());
        // Unknown current still resolves into the only entry.
        assert_eq!(pick_neighbor(&one, None, 1).unwrap().name, "a");
    }
}
