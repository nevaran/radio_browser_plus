//! Root component: layout shell plus one-time browser wiring.

use gloo_timers::callback::Interval;
use leptos::prelude::*;
use wasm_bindgen::{closure::Closure, JsCast};

use crate::components::{
    ChangePasswordModal, CollectionGrid, CreateUserModal, Dialog, LoginModal, NowPlaying, Sidebar,
    StationGrid,
};
use crate::state::{AppState, STATION_REFRESH_MS};

#[component]
pub fn App() -> impl IntoView {
    let state = AppState::new();
    provide_context(state.clone());
    init_app(state.clone());

    let title = state.clone();
    let menu = state.clone();
    let grid_view = state.view;
    let grid_filter = state.filter;

    view! {
        <div class="app-shell">
            <Sidebar />
            <main class="content">
                <div class="toolbar">
                    <button
                        id="menu-toggle"
                        class="menu-toggle"
                        aria-label="Toggle menu"
                        on:click=move |_| menu.sidebar_open.update(|open| *open = !*open)
                    >
                        "☰"
                    </button>
                    <h2 id="view-title">{move || title.view_title.get()}</h2>
                </div>
                <div id="station-grid" class="station-grid">
                    {move || {
                        let collection = matches!(
                            grid_view.get().as_str(),
                            "countries" | "languages" | "tags" | "genres"
                        ) && grid_filter.get().is_none();
                        if collection {
                            view! { <CollectionGrid /> }.into_any()
                        } else {
                            view! { <StationGrid /> }.into_any()
                        }
                    }}
                </div>
                <NowPlaying />
            </main>
        </div>
        <LoginModal />
        <CreateUserModal />
        <ChangePasswordModal />
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
                    state.ensure_favorites().await;
                    state.load_view();
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
            let _ =
                window.add_event_listener_with_callback("online", on_online.as_ref().unchecked_ref());
        }
        on_online.forget();
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
