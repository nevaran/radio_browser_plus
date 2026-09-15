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

    // Only `Copy` handles cross the `Show` boundary below (a `Show`'s
    // children must stay `Fn`, so nothing non-`Copy` may move out of them):
    // signals/StoredValue for reactive reads, plain owned values at top level.
    let store = StoredValue::new(state.clone());
    let user = state.user;
    let favorites = state.favorites;
    let play_station = station.clone();
    let image_src = primary_image(&station);
    let station_store = StoredValue::new(station);
    let fav_id = StoredValue::new(id.clone());
    let article_id = id.clone();
    let button_id = id;

    view! {
        <article
            class="station-card"
            class:favorite=move || fav_id.with_value(|fid| favorites.with(|m| m.contains_key(fid)))
            data-station=article_id
            on:click=move |_| {
                store.with_value(|s| {
                    s.play_from_visible_list(play_station.clone());
                });
            }
        >
            <Show when=move || user.with(|u| u.is_some())>
            <button
                class="favorite-button"
                data-favorite=button_id.clone()
                aria-label="Toggle favorite"
                on:click=move |ev| {
                    ev.stop_propagation();
                    let station = station_store.get_value();
                    store.with_value(|s| s.toggle_favorite(station));
                }
            >
                {move || if fav_id.with_value(|fid| favorites.with(|m| m.contains_key(fid))) { "★" } else { "☆" }}
            </button>
            </Show>
            <div class="card-art">
                <img
                    src=image_src
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
