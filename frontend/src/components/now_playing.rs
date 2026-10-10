//! Bottom now-playing bar: artwork, health indicator, transport, volume.
//!
//! Reads use `Copy` signal handles; playback actions go through a
//! `StoredValue<AppState>`. Nested closures only borrow per-call locals.

use leptos::prelude::*;
use wasm_bindgen::{closure::Closure, JsCast};

use crate::models::station_id;
use crate::state::AppState;
use crate::utils::{
    format_bitrate, primary_image, station_genre, status_score, truncate_name, PLACEHOLDER_SVG,
};

fn after_paint(callback: impl FnOnce() + 'static) {
    let closure = Closure::once(callback);
    if let Some(window) = web_sys::window() {
        let _ = window.request_animation_frame(closure.as_ref().unchecked_ref());
    }
    closure.forget();
}

/// Current favorite state without cloning the favorites map.
fn current_is_favorite(
    current: RwSignal<Option<crate::models::Station>>,
    favorites: RwSignal<std::collections::HashMap<String, crate::models::Favorite>>,
) -> bool {
    let id = current.with(|c| c.as_ref().and_then(station_id));
    match id {
        Some(id) => favorites.with(|m| m.contains_key(&id)),
        None => false,
    }
}

#[component]
pub fn NowPlaying() -> impl IntoView {
    let st = use_context::<AppState>().expect("AppState provided");
    let store = StoredValue::new(st.clone());
    let current = st.player.current;
    let is_playing = st.player.is_playing;
    let loading = st.player.loading;
    let volume = st.player.volume;
    let muted = st.player.muted;
    let compact_bar = st.compact_bar;
    let resize_tick = st.resize_tick;
    let favorites = st.favorites;
    let user = st.user;

    // Compact-mode detection: same overflow rule as the old UI. Re-runs when
    // the track changes or the viewport is resized.
    Effect::new(move |_| {
        current.get();
        resize_tick.get();
        store.with_value(|s| s.compact_bar.set(false));
        after_paint(move || {
            let overflow = web_sys::window()
                .and_then(|w| w.document())
                .and_then(|d| d.get_element_by_id("now-playing-details"))
                .is_some_and(|el| {
                    el.scroll_width() > el.client_width() + 1
                        || el.scroll_height() > el.client_height() + 1
                });
            store.with_value(|s| s.compact_bar.set(overflow));
        });
    });

    view! {
        <Show when=move || current.with(|c| c.is_some())>
            <div
                id="now-playing-bar"
                class="now-playing-bar"
                class:compact=move || compact_bar.get()
            >
                <div class="now-playing-main-row">
                    <div class="now-playing-left">
                        <Show when=move || user.with(|u| u.is_some())>
                        <button
                            id="favorite-current"
                            class="favorite-current"
                            class:is-favorited=move || current_is_favorite(current, favorites)
                            title=move || {
                                if current_is_favorite(current, favorites) {
                                    "Remove from favorites"
                                } else {
                                    "Add to favorites"
                                }
                            }
                            aria-label="Toggle favorite"
                            on:click=move |_| {
                                store.with_value(|s| {
                                    if let Some(station) = s.player.current.get_untracked() {
                                        s.toggle_favorite(station);
                                    }
                                });
                            }
                        >
                            {move || {
                                let favorited = current_is_favorite(current, favorites);
                                view! {
                                    <svg
                                        class="heart-icon"
                                        viewBox="0 0 24 24"
                                        fill=if favorited { "currentColor" } else { "none" }
                                        stroke="currentColor"
                                        stroke-width="1.8"
                                        stroke-linecap="round"
                                        stroke-linejoin="round"
                                        aria-hidden="true"
                                    >
                                        <path d="M12 20.5S3.8 15.4 3.8 9.6A4.6 4.6 0 0 1 12 7.1a4.6 4.6 0 0 1 8.2 2.5c0 5.8-8.2 10.9-8.2 10.9Z" />
                                    </svg>
                                }
                            }}
                        </button>
                        </Show>
                        <div class="now-playing-icon-section">
                            <img
                                id="now-playing-icon"
                                src=move || {
                                    current
                                        .with(|c| c.as_ref().map(primary_image))
                                        .unwrap_or_else(|| PLACEHOLDER_SVG.to_string())
                                }
                                alt="Station"
                                on:error=|ev| {
                                    if let Some(target) = ev.target() {
                                        if let Ok(img) = target
                                            .dyn_into::<web_sys::HtmlImageElement>()
                                        {
                                            if img.src() != PLACEHOLDER_SVG {
                                                img.set_src(PLACEHOLDER_SVG);
                                            }
                                        }
                                    }
                                }
                            />
                            <span
                                class="art-eq"
                                class:active=move || is_playing.get()
                                aria-hidden="true"
                            >
                                <i></i><i></i><i></i><i></i>
                            </span>
                        </div>
                        <div class="now-playing-status" aria-label="Station quality indicator">
                            <div
                                id="now-playing-status-bar"
                                class="now-playing-status-bar"
                                title=move || {
                                    current
                                        .with(|c| c.as_ref().map(|s| status_score(s).3))
                                        .unwrap_or_default()
                                }
                                style=move || {
                                    current
                                        .with(|c| {
                                            c.as_ref().map(|s| {
                                                let (height, hue, lightness, _) = status_score(s);
                                                format!(
                                                    "height:{height}%;background:hsl({hue} 85% {lightness}%);box-shadow:0 0 10px hsl({hue} 85% {lightness}% / 0.55)",
                                                )
                                            })
                                        })
                                        .unwrap_or_default()
                                }
                            ></div>
                        </div>
                        <div class="now-playing-meta">
                            <div class="now-playing-name" id="now-playing-name">
                                {move || {
                                    current
                                        .with(|c| {
                                            c.as_ref().map(|s| truncate_name(&s.name, 50))
                                        })
                                        .unwrap_or_else(|| "Nothing playing".to_string())
                                }}
                            </div>
                            <div class="now-playing-details" id="now-playing-details">
                                <span id="now-playing-country">
                                    {move || {
                                        current
                                            .with(|c| {
                                                c.as_ref().and_then(|s| s.country.clone())
                                            })
                                            .unwrap_or_else(|| "Unknown".to_string())
                                    }}
                                </span>
                                <span class="separator">"·"</span>
                                <span id="now-playing-genre">
                                    {move || {
                                        current
                                            .with(|c| c.as_ref().map(station_genre))
                                            .unwrap_or_else(|| "Unknown genre".to_string())
                                    }}
                                </span>
                                <span class="separator">"·"</span>
                                <span id="now-playing-bitrate">
                                    {move || {
                                        current
                                            .with(|c| c.as_ref().map(|s| format_bitrate(s.bitrate)))
                                            .unwrap_or_else(|| "Stream".to_string())
                                    }}
                                </span>
                            </div>
                        </div>
                    </div>

                    <div class="transport-controls">
                        <button
                            id="prev-btn"
                            class="skip-btn"
                            aria-label="Previous station"
                            title="Previous station"
                            prop:disabled=move || !store.with_value(|s| s.has_prev_next())
                            on:click=move |_| {
                                store.with_value(|s| s.play_previous());
                            }
                        >
                            <svg viewBox="0 0 24 24" aria-hidden="true">
                                <path d="M18 6.5v11L9.5 12 18 6.5ZM7 6h2v12H7V6Z" />
                            </svg>
                        </button>
                    <div class="play-stop-container">
                        <button
                            id="play-stop-btn"
                            class:is-playing=move || is_playing.get()
                            aria-label=move || {
                                if is_playing.get() { "Stop playback" } else { "Resume playback" }
                            }
                            title=move || if is_playing.get() { "Stop" } else { "Play" }
                            on:click=move |_| {
                                store.with_value(|s| {
                                    if s.player.is_playing.get_untracked() {
                                        s.player.pause();
                                    } else {
                                        s.player.resume();
                                    }
                                });
                            }
                        >
                            {move || {
                                if is_playing.get() {
                                    view! {
                                        <svg class="icon-stop" viewBox="0 0 24 24" aria-hidden="true">
                                            <rect x="7" y="7" width="10" height="10" rx="2" />
                                        </svg>
                                    }
                                } else {
                                    view! {
                                        <svg class="icon-play" viewBox="0 0 24 24" aria-hidden="true">
                                            <path d="M8.5 5.8v12.4L18.8 12 8.5 5.8Z" />
                                        </svg>
                                    }
                                }
                            }}
                        </button>
                        <Show when=move || loading.get()>
                            <div id="loading-spinner" class="loading-spinner"></div>
                        </Show>
                    </div>
                        <button
                            id="next-btn"
                            class="skip-btn"
                            aria-label="Next station"
                            title="Next station"
                            prop:disabled=move || !store.with_value(|s| s.has_prev_next())
                            on:click=move |_| {
                                store.with_value(|s| s.play_next());
                            }
                        >
                            <svg viewBox="0 0 24 24" aria-hidden="true">
                                <path d="M6 6.5v11L14.5 12 6 6.5ZM15 6h2v12h-2V6Z" />
                            </svg>
                        </button>
                    </div>

                    <div class="now-playing-controls">
                        <div class="volume-control">
                            <button
                                id="volume-icon"
                                class="volume-icon"
                                title="Mute/Unmute"
                                aria-label="Mute or unmute"
                                on:click=move |_| {
                                    store.with_value(|s| s.player.toggle_mute());
                                }
                            >
                                {move || {
                                    if muted.get() {
                                        view! {
                                            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                                                <path d="M11 5 6.5 9H3v6h3.5L11 19V5Z" />
                                                <path d="m16.5 9.5 5 5m0-5-5 5" />
                                            </svg>
                                        }
                                        .into_any()
                                    } else {
                                        view! {
                                            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                                                <path d="M11 5 6.5 9H3v6h3.5L11 19V5Z" />
                                                <path d="M15.2 9.3a4 4 0 0 1 0 5.4M18 6.8a8 8 0 0 1 0 10.4" />
                                            </svg>
                                        }
                                        .into_any()
                                    }
                                }}
                            </button>
                            <input
                                id="volume-slider"
                                type="range"
                                min="0"
                                max="1"
                                step="0.01"
                                class="volume-slider"
                                aria-label="Volume"
                                style=move || {
                                    let effective = if muted.get() { 0.0 } else { volume.get() };
                                    format!("--vol:{effective:.2}")
                                }
                                prop:value=move || format!("{:.2}", volume.get())
                                on:input=move |ev| {
                                    if let Ok(v) = event_target_value(&ev).parse::<f64>() {
                                        store.with_value(|s| s.player.set_volume(v));
                                    }
                                }
                            />
                            <span id="volume-display" class="volume-display">
                                {move || {
                                    let is_muted = muted.get();
                                    let vol = volume.get();
                                    let effective = if is_muted { 0.0 } else { vol };
                                    format!("{}%", (effective * 100.0).round() as u32)
                                }}
                            </span>
                        </div>
                    </div>
                </div>

                <div class="now-playing-meta-row">
                    <div class="now-playing-meta">
                        <div class="now-playing-name" id="now-playing-name-mobile">
                            {move || {
                                current
                                    .with(|c| c.as_ref().map(|s| truncate_name(&s.name, 50)))
                                    .unwrap_or_else(|| "Nothing playing".to_string())
                            }}
                        </div>
                        <div class="now-playing-details">
                            <span id="now-playing-country-mobile">
                                {move || {
                                    current
                                        .with(|c| c.as_ref().and_then(|s| s.country.clone()))
                                        .unwrap_or_else(|| "Unknown".to_string())
                                }}
                            </span>
                            <span class="separator">"·"</span>
                            <span id="now-playing-genre-mobile">
                                {move || {
                                    current
                                        .with(|c| c.as_ref().map(station_genre))
                                        .unwrap_or_else(|| "Unknown genre".to_string())
                                }}
                            </span>
                            <span class="separator">"·"</span>
                            <span id="now-playing-bitrate-mobile">
                                {move || {
                                    current
                                        .with(|c| c.as_ref().map(|s| format_bitrate(s.bitrate)))
                                        .unwrap_or_else(|| "Stream".to_string())
                                }}
                            </span>
                        </div>
                    </div>
                </div>
            </div>
        </Show>
    }
}
