//! Station card grid (All / Popular / Favorites / filtered / search views).

use leptos::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::HtmlImageElement;

use crate::models::{station_id, Station};
use crate::state::AppState;
use crate::utils::{format_bitrate, primary_image, truncate_name, PLACEHOLDER_SVG};

#[component]
fn StationCard(station: Station) -> impl IntoView {
    let state = use_context::<AppState>().expect("AppState provided");
    let id = station_id(&station).unwrap_or_default();
    let country = station
        .country
        .clone()
        .unwrap_or_else(|| "Unknown".to_string());
    let meta = format!("{country} · {}", format_bitrate(station.bitrate));
    let name = truncate_name(&station.name, 50);

    let fav_state = state.clone();
    let fav_id = id.clone();
    let play_state = state.clone();
    let play_station = station.clone();
    let toggle_state = state.clone();
    let toggle_station = station.clone();
    let article_id = id.clone();
    let button_id = id.clone();

    view! {
        <article
            class="station-card"
            class:favorite=move || fav_state.is_favorite(&fav_id)
            data-station=article_id
            on:click=move |_| {
                let current_id = station_id(&play_station);
                play_state.player.play(play_station.clone());
                if current_id.is_some_and(|id| play_state.favorites.get_untracked().contains_key(&id)) {
                    play_state.refresh_favorite_metadata(play_station.clone());
                }
            }
        >
            <button
                class="favorite-button"
                data-favorite=button_id
                aria-label="Toggle favorite"
                on:click=move |ev| {
                    ev.stop_propagation();
                    toggle_state.toggle_favorite(toggle_station.clone());
                }
            >
                {move || if state.is_favorite(&id) { "★" } else { "☆" }}
            </button>
            <div class="card-art">
                <img
                    src=primary_image(&station)
                    alt=name.clone()
                    loading="lazy"
                    decoding="async"
                    on:error=move |ev| {
                        if let Some(target) = ev.target() {
                            if let Ok(img) = target.dyn_into::<HtmlImageElement>() {
                                if img.src() != PLACEHOLDER_SVG {
                                    img.set_src(PLACEHOLDER_SVG);
                                }
                            }
                        }
                    }
                />
            </div>
            <div>
                <div class="station-name">{name}</div>
                <div class="station-meta">{meta}</div>
            </div>
        </article>
    }
}

#[component]
pub fn StationGrid() -> impl IntoView {
    let state = use_context::<AppState>().expect("AppState provided");
    let visible = state.clone();
    let listed = state.clone();

    view! {
        <Show
            when=move || !visible.stations.get().is_empty()
            fallback=|| {
                view! { <div class="station-card"><h3>"No stations found"</h3></div> }
            }
        >
            // Render everything fetched: the fetch limit already bounds the
            // list, and cutting the grid would silently drop stations (the
            // tail after display sorting is disproportionately regional or
            // non-Latin-script ones). Artwork stays cheap via lazy loading.
            <For
                each=move || listed.stations.get()
                key=|s| station_id(s).unwrap_or_default()
                let:station
            >
                <StationCard station=station.clone() />
            </For>
        </Show>
    }
}
