//! Bottom now-playing bar: artwork, health indicator, transport, volume.
//!
//! Reads use `Copy` signal handles; playback actions go through a
//! `StoredValue<AppState>`. Nested closures only borrow per-call locals.

use leptos::prelude::*;
use wasm_bindgen::{closure::Closure, JsCast};

use crate::models::station_id;
use crate::player::Player;
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
                            {move || if current_is_favorite(current, favorites) { "★" } else { "☆" }}
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
                            "⏮"
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
                            {move || if is_playing.get() { "■" } else { "▶" }}
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
                            "⏭"
                        </button>
                    </div>

                    <div class="now-playing-controls">
                        <div class="volume-control">
                            <button
                                id="volume-icon"
                                class="volume-icon"
                                title="Mute/Unmute"
                                style="background: none; border: none; cursor: pointer; font-size: 1.2rem; padding: 0.3rem;"
                                on:click=move |_| {
                                    store.with_value(|s| s.player.toggle_mute());
                                }
                            >
                                {move || Player::volume_icon(muted.get(), volume.get())}
                            </button>
                            <input
                                id="volume-slider"
                                type="range"
                                min="0"
                                max="1"
                                step="0.01"
                                class="volume-slider"
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
