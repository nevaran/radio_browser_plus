//! Root component: layout shell plus one-time browser wiring.

use gloo_timers::callback::Interval;
use leptos::prelude::*;
use wasm_bindgen::{closure::Closure, JsCast};

use crate::components::{
    ActiveSessionsModal, ChangePasswordModal, CollectionGrid, CreateUserModal, Dialog, LoginModal,
    NowPlaying, ShareModal, Sidebar, StationGrid,
};
use crate::state::{AppState, STATION_REFRESH_MS};
use crate::utils::{format_count, SortMode};

#[component]
pub fn App() -> impl IntoView {
    let state = AppState::new();
    provide_context(state.clone());
    init_app(state.clone());

    let title = state.clone();
    let menu = state.clone();
    let sub = state.clone();
    let sort_state = state.clone();
    let sort_store = StoredValue::new(state.clone());
    let scroller = state.clone();
    let err_show = state.clone();
    let err_text = state.clone();
    let err_retry = StoredValue::new(state.clone());
    let err_dismiss = StoredValue::new(state.clone());
    let grid_view = state.view;
    let grid_filter = state.filter;
    let loading = state.loading;

    // Collection views (Countries / Languages / Genres) list entries, not
    // stations, so the station sort control hides while they are visible.
    let is_collection = move || {
        matches!(
            grid_view.get().as_str(),
            "countries" | "languages" | "tags" | "genres"
        ) && grid_filter.get().is_none()
    };

    let subtitle = move || {
        // Collection views browse entries (countries, languages, genres);
        // every other view lists stations. Only count what is on screen.
        let browsing_entries = matches!(
            sub.view.get().as_str(),
            "countries" | "languages" | "tags" | "genres"
        ) && sub.filter.get().is_none();
        if browsing_entries {
            let entries = sub.collections.get().len();
            if entries > 0 {
                return format_count(entries, "item");
            }
        } else {
            let stations = sub.stations.get().len();
            if stations > 0 {
                return format_count(stations, "station");
            }
        }
        if sub.loading.get() {
            return "Tuning in…".to_string();
        }
        match sub.view.get().as_str() {
            "favorites" => "Star a station to keep it here".to_string(),
            "search" => "No stations matched that search".to_string(),
            "countries" | "languages" | "genres" => "Nothing to browse right now".to_string(),
            _ => "No stations to show right now".to_string(),
        }
    };

    view! {
        <div class="app-shell">
            <Sidebar />
            <main class="content">
                <header class="toolbar">
                    <button
                        id="menu-toggle"
                        class="menu-toggle"
                        aria-label="Toggle menu"
                        on:click=move |_| menu.sidebar_open.update(|open| *open = !*open)
                    >
                        <svg viewBox="0 0 24 24" aria-hidden="true">
                            <path d="M4 7h16M4 12h16M4 17h16" />
                        </svg>
                    </button>
                    <div class="toolbar-titles">
                        <h2 id="view-title">{move || title.view_title.get()}</h2>
                        <p class="toolbar-sub">{subtitle}</p>
                    </div>
                    <div class="toolbar-actions">
                        <Show when=move || !is_collection()>
                            <label class="sort-control">
                                <span class="sort-label">"Sort"</span>
                                <select
                                    class="sort-select"
                                    aria-label="Sort stations"
                                    prop:value=move || sort_state.sort.get().key()
                                    on:change=move |ev| {
                                        let mode = SortMode::from_key(&event_target_value(&ev));
                                        sort_store.with_value(|s| s.set_sort(mode));
                                    }
                                >
                                    {SortMode::ALL
                                        .into_iter()
                                        .map(|mode| {
                                            view! { <option value=mode.key()>{mode.label()}</option> }
                                        })
                                        .collect_view()}
                                </select>
                            </label>
                        </Show>
                    </div>
                    <div
                        class="toolbar-progress"
                        class:active=move || loading.get()
                        aria-hidden="true"
                    ></div>
                </header>
                // Load-failure banner: visible only while stale rows are on
                // screen (an empty grid renders the error block instead), so
                // a failed tab switch explains itself and offers a Retry
                // instead of silently re-showing the previous view.
                <Show when=move || {
                    err_show.view_error.get().is_some()
                        && !err_show.loading.get()
                        && (!err_show.stations.get().is_empty()
                            || !err_show.collections.get().is_empty())
                }>
                    <div class="view-error" role="alert">
                        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                            <path d="M12 4 3 20h18L12 4Z" />
                            <path d="M12 10v4.5m0 3v.5" />
                        </svg>
                        <span class="view-error-text">
                            {move || err_text.view_error.get().unwrap_or_default()}
                        </span>
                        <button
                            type="button"
                            class="view-error-retry"
                            on:click=move |_| err_retry.with_value(|s| s.load_view())
                        >
                            "Retry"
                        </button>
                        <button
                            type="button"
                            class="view-error-dismiss"
                            aria-label="Dismiss error"
                            on:click=move |_| err_dismiss.with_value(|s| s.dismiss_error())
                        >
                            "×"
                        </button>
                    </div>
                </Show>
                // Infinite scroll: the grids mount a window of rows with a
                // spacer reserving the full height, so grow the window as
                // the viewport nears the end of the *mounted* rows
                // (`load_more_at`, recomputed after every render). The
                // check is layout-free and `show_more` is a no-op once
                // every row is visible, so this stays quiet on scroll.
                <div
                    id="station-grid"
                    class="station-grid"
                    on:scroll=move |ev| {
                        let Some(target) = ev.target() else { return };
                        let Ok(el) = target.dyn_into::<web_sys::HtmlElement>() else {
                            return
                        };
                        if el.scroll_top() + el.client_height() + 1600
                            >= scroller.load_more_at.get_untracked()
                        {
                            scroller.show_more();
                        }
                    }
                >
                    {move || {
                        if is_collection() {
                            view! { <CollectionGrid /> }.into_any()
                        } else {
                            view! { <StationGrid /> }.into_any()
                        }
                    }}
                </div>
                <NowPlaying />
            </main>
        </div>
        <ActiveSessionsModal />
        <LoginModal />
        <CreateUserModal />
        <ChangePasswordModal />
        <ShareModal />
        <Dialog />
    }
}

/// One-time startup: viewport fix, initial route, session restore, listeners.
fn init_app(state: AppState) {
    update_viewport_height();
    state.apply_hash();

    // Runtime config first (public): it decides whether anonymous browsing
    // is allowed. A failed fetch keeps the fail-closed default (off).
    // Then restore the session. Without one the backend rejects every data
    // request, so a failed restore locks the UI behind the login dialog —
    // unless guest browsing is allowed, in which case the view loads
    // anonymously. A restored session always (re)loads the view because the
    // initial load is skipped while logged out.
    {
        let state = state.clone();
        leptos::task::spawn_local(async move {
            if let Ok(config) = crate::api::fetch_config().await {
                state.allow_guest.set(config.allow_guest);
            }
            match crate::api::fetch_me().await {
                Ok(user) => {
                    state.session_checked.set(true);
                    state.user.set(Some(user));
                    if state.is_admin() {
                        state.refresh_admin_stats();
                    }
                    state.ensure_favorites().await;
                    state.load_view();
                    // A share link opened before the session was known loads
                    // once the session is confirmed here.
                    state.load_pending_shared_station();
                }
                // A login submitted while the restore was in flight wins:
                // never wipe an established session with a stale failure.
                Err(_) => {
                    if state.is_logged_in() {
                        return;
                    }
                    state.handle_unauthorized();
                    if state.allow_guest.get_untracked() {
                        state.load_view();
                        // Same retry, now that guest browsing is confirmed.
                        state.load_pending_shared_station();
                    }
                }
            }
        });
    }

    // Hash routing (back/forward buttons + `#view/filter` deep links).
    {
        let state = state.clone();
        let on_hash = Closure::wrap(Box::new(move || {
            state.apply_hash();
        }) as Box<dyn Fn()>);
        if let Some(window) = web_sys::window() {
            window.set_onhashchange(Some(on_hash.as_ref().unchecked_ref()));
        }
        on_hash.forget();
    }

    // Viewport + compact-bar recompute on resize.
    {
        let state = state.clone();
        let on_resize = Closure::wrap(Box::new(move || {
            update_viewport_height();
            state.resize_tick.update(|t| *t += 1);
        }) as Box<dyn Fn()>);
        if let Some(window) = web_sys::window() {
            let _ = window
                .add_event_listener_with_callback("resize", on_resize.as_ref().unchecked_ref());
        }
        on_resize.forget();
    }

    // Close the mobile menu when tapping outside of it.
    {
        let state = state.clone();
        let on_doc_click = Closure::wrap(Box::new(move |ev: web_sys::MouseEvent| {
            if !state.sidebar_open.get_untracked() {
                return;
            }
            let Some(document) = web_sys::window().and_then(|w| w.document()) else {
                return;
            };
            let target = ev.target();
            let inside = |selector: &str| {
                document
                    .query_selector(selector)
                    .ok()
                    .flatten()
                    .zip(target.clone())
                    .is_some_and(|(el, t)| el.contains(Some(&t.unchecked_into())))
            };
            if !inside("#sidebar") && !inside("#menu-toggle") {
                state.sidebar_open.set(false);
            }
        }) as Box<dyn Fn(web_sys::MouseEvent)>);
        if let Some(document) = web_sys::window().and_then(|w| w.document()) {
            let _ = document
                .add_event_listener_with_callback("click", on_doc_click.as_ref().unchecked_ref());
        }
        on_doc_click.forget();
    }

    // Keyboard shortcuts: M mute, Space play/pause, arrows volume.
    {
        let state = state.clone();
        let on_key = Closure::wrap(Box::new(move |ev: web_sys::KeyboardEvent| {
            if let Some(target) = ev.target() {
                if let Ok(el) = target.dyn_into::<web_sys::HtmlElement>() {
                    match el.tag_name().to_uppercase().as_str() {
                        "INPUT" | "TEXTAREA" | "SELECT" => return,
                        _ => {}
                    }
                }
            }
            // "/" jumps to search (opening the mobile drawer first so the
            // field is actually reachable); Escape closes the drawer.
            if ev.key() == "/" {
                ev.prevent_default();
                focus_search_field(&state);
                return;
            }
            if ev.key() == "Escape" {
                // Close the topmost overlay first: the admin dialog should
                // not leave the mobile navigation drawer exposed behind it.
                if state.admin_stats_open.get_untracked() {
                    state.admin_stats_open.set(false);
                } else {
                    state.sidebar_open.set(false);
                }
                return;
            }
            // Media keys on keyboards that surface as key events (most OS
            // media keys arrive via the Media Session handlers below instead).
            let code = ev.code();
            let key = ev.key();
            let is = |name: &str| code == name || key == name;
            if is("MediaTrackNext") {
                ev.prevent_default();
                state.play_next();
            } else if is("MediaTrackPrevious") {
                ev.prevent_default();
                state.play_previous();
            } else if is("MediaPlayPause") {
                ev.prevent_default();
                if state.player.is_playing.get_untracked() {
                    state.player.pause();
                } else {
                    state.player.resume();
                }
            } else if is("MediaPlay") {
                ev.prevent_default();
                state.player.resume();
            } else if is("MediaPause") || is("MediaStop") {
                ev.prevent_default();
                state.player.pause();
            } else if ev.key().to_lowercase() == "m" {
                ev.prevent_default();
                state.player.toggle_mute();
            } else if ev.code() == "Space" {
                ev.prevent_default();
                if state.player.is_playing.get_untracked() {
                    state.player.pause();
                } else {
                    state.player.resume();
                }
            } else if ev.code() == "ArrowLeft" {
                ev.prevent_default();
                let v = state.player.volume.get_untracked();
                state.player.set_volume(v - 0.05);
            } else if ev.code() == "ArrowRight" {
                ev.prevent_default();
                let v = state.player.volume.get_untracked();
                state.player.set_volume(v + 0.05);
            }
        }) as Box<dyn Fn(web_sys::KeyboardEvent)>);
        if let Some(window) = web_sys::window() {
            let _ =
                window.add_event_listener_with_callback("keydown", on_key.as_ref().unchecked_ref());
        }
        on_key.forget();
    }

    // Periodic refresh of the visible view (every 6h, as before).
    {
        let state = state.clone();
        Interval::new(STATION_REFRESH_MS, move || state.load_view()).forget();
    }

    // Refresh admin session stats regularly while the admin is active.
    {
        let state = state.clone();
        Interval::new(60_000, move || {
            if state.is_admin() {
                state.refresh_admin_stats();
            }
        })
        .forget();
    }

    // Keep active presence fresh and record station changes for users/guests.
    {
        let state = state.clone();
        Effect::new(move || {
            let _ = state.player.current.get();
            let session_checked = state.session_checked.get();
            let logged_in = state.user.get().is_some();
            let allow_guest = state.allow_guest.get();
            if session_checked && (logged_in || allow_guest) {
                state.send_heartbeat();
            }
        });
    }
    {
        let state = state.clone();
        Interval::new(60_000, move || state.send_heartbeat()).forget();
    }

    // OS media controls (lock screen, headset / keyboard media keys): the
    // handlers step within the list the current station was started from.
    {
        let play = state.clone();
        let pause = state.clone();
        let prev = state.clone();
        let next = state.clone();
        crate::media::init_media_handlers(
            move || play.player.resume(),
            move || pause.player.pause(),
            move || prev.play_previous(),
            move || next.play_next(),
        );
    }

    // Mirror player state to the OS: track metadata on station change,
    // play/pause state on transport change. No-ops where unsupported.
    {
        let current = state.clone();
        Effect::new(move || match current.player.current.get() {
            Some(station) => crate::media::update_media_metadata(&station),
            None => crate::media::clear_media_metadata(),
        });
    }
    {
        let transport = state.clone();
        Effect::new(move || {
            crate::media::update_playback_state(transport.player.is_playing.get());
        });
    }

    // Reconnect immediately when connectivity returns (e.g. wifi -> mobile
    // handover): the stall watchdog would catch it eventually, but the OS
    // tells us exactly when the new network is up.
    {
        let state = state.clone();
        let on_online = Closure::wrap(Box::new(move || {
            state.reconnect();
        }) as Box<dyn Fn()>);
        if let Some(window) = web_sys::window() {
            let _ = window
                .add_event_listener_with_callback("online", on_online.as_ref().unchecked_ref());
        }
        on_online.forget();
    }
}

/// Focus the sidebar search box, pulling the mobile drawer open first so the
/// field is on screen. Bound to the `/` shortcut.
fn focus_search_field(state: &AppState) {
    let narrow = web_sys::window()
        .map(|w| {
            w.inner_width()
                .ok()
                .and_then(|v| v.as_f64())
                .unwrap_or(f64::MAX)
                <= 768.0
        })
        .unwrap_or(false);
    if narrow {
        state.sidebar_open.set(true);
    }
    let focus = || {
        if let Some(el) = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id("search"))
        {
            let _ = el.unchecked_into::<web_sys::HtmlElement>().focus();
        }
    };
    // Wait a frame on mobile so the drawer is mounted/visible before focus.
    if narrow {
        let closure = Closure::once(focus);
        if let Some(window) = web_sys::window() {
            let _ = window.request_animation_frame(closure.as_ref().unchecked_ref());
        }
        closure.forget();
    } else {
        focus();
    }
}

/// Mobile-browser-chrome workaround: pin layout heights to the real layout
/// viewport height (`documentElement.clientHeight`), so URL-bar show/hide
/// cycles can't stretch the app shell under the system UI.
/// System-bar insets (notch, gesture/navigation bar) are handled purely in
/// CSS via `env(safe-area-inset-*)` + `viewport-fit=cover` — the browser
/// reports the real numbers, no JS guesswork needed.
fn update_viewport_height() {
    let Some(window) = web_sys::window() else {
        return;
    };
    let Some(document) = window.document() else {
        return;
    };
    let Some(root) = document
        .document_element()
        .and_then(|el| el.dyn_into::<web_sys::HtmlElement>().ok())
    else {
        return;
    };
    let inner_w = window
        .inner_width()
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(1024.0);
    let client_h = f64::from(root.client_height()).max(1.0);

    let style = root.style();
    let _ = style.set_property("--viewport-height", &format!("{client_h}px"));

    if let Some(body) = document.body() {
        let body_style = body.style();
        if inner_w <= 768.0 {
            let _ = body_style.set_property("height", &format!("{client_h}px"));
            let _ = body_style.set_property("min-height", &format!("{client_h}px"));
        } else {
            let _ = body_style.remove_property("height");
            let _ = body_style.remove_property("min-height");
        }
    }
}
