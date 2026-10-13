//! Station card grid (All / Popular / Favorites / filtered / search views).

use leptos::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::HtmlImageElement;

use crate::models::{station_id, Station};
use crate::state::AppState;
use crate::utils::{
    card_playback, primary_image, station_flag_url, station_genre, truncate_name,
    CardPlayback, PLACEHOLDER_SVG,
};

/// Stroke icon used by empty-state blocks.
pub(crate) fn empty_icon(kind: &str) -> AnyView {
    let inner = match kind {
        "search" => view! {
            <><circle cx="11" cy="11" r="7" /><path d="m20 20-3.2-3.2" /></>
        }
        .into_any(),
        "globe" => view! {
            <><circle cx="12" cy="12" r="9" /><path d="M3 12h18M12 3c2.6 2.6 3.9 5.6 3.9 9s-1.3 6.4-3.9 9c-2.6-2.6-3.9-5.6-3.9-9S9.4 5.6 12 3Z" /></>
        }
        .into_any(),
        "warn" => view! {
            <><path d="M12 4 3 20h18L12 4Z" /><path d="M12 10v4.5m0 3v.5" /></>
        }
        .into_any(),
        _ => view! {
            <><path d="M4 9.5 18.5 4a1 1 0 0 1 1.3.9v10.9" /><path d="M4 9.5v8A1.5 1.5 0 0 0 5.5 19H14" /><circle cx="8.5" cy="14.5" r="2.5" /><path d="M17 8.2a5 5 0 0 1 0 5.6" /></>
        }
        .into_any(),
    };
    view! {
        <svg class="empty-glyph" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
            {inner}
        </svg>
    }
    .into_any()
}

/// Shared empty-state block for the station and collection grids.
pub(crate) fn empty_state(
    icon: AnyView,
    title: String,
    message: String,
    action: Option<(&'static str, StoredValue<AppState>)>,
) -> AnyView {
    view! {
        <div class="empty-state">
            <span class="empty-icon">{icon}</span>
            <h3>{title}</h3>
            <p>{message}</p>
            {action.map(|(label, store)| {
                view! {
                    <button
                        type="button"
                        class="secondary-button"
                        on:click=move |_| store.with_value(|s| s.navigate("popular", None))
                    >
                        {label}
                    </button>
                }
            })}
        </div>
    }
    .into_any()
}

/// Load-failure block with a Retry action, shared by both grids. Reached
/// when a view has no rows to show at all; with stale rows on screen the
/// banner in the app shell carries the error instead.
pub(crate) fn error_state(message: String, retry: StoredValue<AppState>) -> AnyView {
    view! {
        <div class="empty-state">
            <span class="empty-icon error">{empty_icon("warn")}</span>
            <h3>"Couldn't load this view"</h3>
            <p>{message}</p>
            <button
                type="button"
                class="secondary-button"
                on:click=move |_| retry.with_value(|s| s.load_view())
            >
                "Try again"
            </button>
        </div>
    }
    .into_any()
}

/// Placeholder grid shown while a view is loading for the first time.
#[component]
pub fn GridSkeleton() -> impl IntoView {
    view! {
        <div class="skeleton-grid" aria-hidden="true">
            {(0..12)
                .map(|_| {
                    view! {
                        <div class="skeleton-card">
                            <div class="skeleton-art"></div>
                            <div class="skeleton-line w-70"></div>
                            <div class="skeleton-line w-40"></div>
                        </div>
                    }
                })
                .collect_view()}
        </div>
    }
}

#[component]
fn StationCard(station: Station) -> impl IntoView {
    let state = use_context::<AppState>().expect("AppState provided");
    let id = station_id(&station).unwrap_or_default();
    let country = station
        .country
        .clone()
        .unwrap_or_else(|| "Unknown".to_string());
    let name = truncate_name(&station.name, 50);
    let flag = station_flag_url(&station);
    // Bitrate lives in the art badge, so the meta line carries genre instead.
    let genre = truncate_name(&station_genre(&station), 26);
    let meta = format!("{country} · {genre}");
    let bitrate_badge = station
        .bitrate
        .filter(|b| *b > 0)
        .map(|b| format!("{b} kbps"));
    let secure = station.has_https == Some(true);
    let accessible_name = format!("Play {name} — {genre}");

    // Only `Copy` handles cross the `Show` boundary below (a `Show`'s
    // children must stay `Fn`, so nothing non-`Copy` may move out of them):
    // signals/StoredValue for reactive reads, plain owned values at top level.
    let store = StoredValue::new(state.clone());
    let user = state.user;
    let favorites = state.favorites;
    let current = state.player.current;
    let is_playing = state.player.is_playing;
    let loading = state.player.loading;
    let play_station = station.clone();
    let play_station_key = station.clone();
    let image_src = primary_image(&station);
    let station_store = StoredValue::new(station);
    let fav_id = StoredValue::new(id.clone());
    let id_store = StoredValue::new(id.clone());
    let article_id = id.clone();
    let button_id = id;

    let is_current = move || {
        id_store.with_value(|id| {
            current.with(|c| c.as_ref().and_then(station_id).as_deref() == Some(id.as_str()))
        })
    };

    // Playback badge follows the player, not the selection: only a station
    // that is actually sounding (or audibly buffering) reads "Playing".
    let status = move || card_playback(is_current(), is_playing.get(), loading.get());

    view! {
        <article
            class="station-card"
            class:favorite=move || fav_id.with_value(|fid| favorites.with(|m| m.contains_key(fid)))
            class:current=move || !matches!(status(), CardPlayback::Idle)
            class:playing=move || matches!(status(), CardPlayback::Playing)
            data-station=article_id
            role="button"
            tabindex="0"
            aria-label=accessible_name
            on:click=move |_| {
                store.with_value(|s| {
                    s.play_from_visible_list(play_station.clone());
                });
            }
            on:keydown=move |ev| {
                if ev.key() == "Enter" || ev.code() == "Space" {
                    ev.prevent_default();
                    store.with_value(|s| {
                        s.play_from_visible_list(play_station_key.clone());
                    });
                }
            }
        >
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
                <Show when=move || !matches!(status(), CardPlayback::Idle)>
                    {move || match status() {
                        CardPlayback::Loading => view! {
                            <span class="playing-pill loading">
                                <span class="eq" aria-hidden="true">
                                    <i></i><i></i><i></i>
                                </span>
                                "Loading"
                            </span>
                        }
                            .into_any(),
                        CardPlayback::Playing => view! {
                            <span class="playing-pill">
                                <span class="eq" aria-hidden="true">
                                    <i></i><i></i><i></i>
                                </span>
                                "Playing"
                            </span>
                        }
                            .into_any(),
                        _ => view! {
                            <span class="playing-pill paused">
                                <span class="eq" aria-hidden="true">
                                    <i></i><i></i><i></i>
                                </span>
                                "Paused"
                            </span>
                        }
                            .into_any(),
                    }}
                </Show>
                <div class="card-badges">
                    {bitrate_badge.map(|b| view! { <span class="badge">{b}</span> })}
                    {secure.then(|| view! { <span class="badge badge-secure">"HTTPS"</span> })}
                </div>
                <Show when=move || user.with(|u| u.is_some())>
                    <button
                        class="favorite-button"
                        data-favorite=button_id.clone()
                        aria-label="Toggle favorite"
                        title="Toggle favorite"
                        on:click=move |ev| {
                            ev.stop_propagation();
                            let station = station_store.get_value();
                            store.with_value(|s| s.toggle_favorite(station));
                        }
                    >
                        {move || if fav_id.with_value(|fid| favorites.with(|m| m.contains_key(fid))) { "★" } else { "☆" }}
                    </button>
                </Show>
            </div>
            <div class="card-body">
                <div class="station-name">{name.clone()}</div>
                <div class="station-meta">
                    {flag.map(|url| view! { <img class="flag" src=url alt="" loading="lazy" /> })}
                    <span class="station-meta-text">{meta}</span>
                </div>
            </div>
        </article>
    }
}

#[component]
pub fn StationGrid() -> impl IntoView {
    let state = use_context::<AppState>().expect("AppState provided");
    let visible = state.clone();
    let listed = state.clone();
    let totals = state.clone();
    let store = StoredValue::new(state.clone());
    let foot_store = StoredValue::new(state.clone());
    let err_store = StoredValue::new(state.clone());
    let spacer_state = state.clone();
    let spacer_px = state.clone();

    // Keep the scroll reservation (stable scrollbar from the first page)
    // and the load threshold in sync with the mounted window, the totals,
    // and the viewport. `update_spacer` measures the committed rows, so it
    // runs here — after render — rather than during it.
    Effect::new(move |_| {
        spacer_state.grid_limit.get();
        let _ = spacer_state.stations.with(|rows| rows.len());
        spacer_state.resize_tick.get();
        spacer_state.update_spacer();
    });

    // Display order = fetched order with the user's sort mode applied,
    // windowed to the mounted page: only the first `grid_limit` rows mount,
    // so images, DOM nodes and reactive scopes stay bounded no matter how
    // many rows the fetch returned. Infinite scroll grows the window.
    let stations = Memo::new(move |_| {
        let mut list = listed.display_stations();
        list.truncate(listed.grid_limit.get());
        list
    });

    let empty = move || {
        let kind = match visible.view.get().as_str() {
            "favorites" => "radio",
            "search" => "search",
            _ => "globe",
        };
        let icon = empty_icon(kind);
        let store = store;
        match visible.view.get().as_str() {
            "favorites" => empty_state(
                icon,
                "No favorites yet".to_string(),
                "Star any station to keep it in your collection.".to_string(),
                Some(("Browse popular", store)),
            ),
            "search" => {
                let query = visible.search_query.get();
                let message = if query.is_empty() {
                    "Try a different search term.".to_string()
                } else {
                    format!("Nothing matched \u{201C}{query}\u{201D}. Try another term.")
                };
                empty_state(
                    icon,
                    "No stations found".to_string(),
                    message,
                    Some(("Browse popular", store)),
                )
            }
            _ => {
                let title = if visible.filter.get().is_some() {
                    "Nothing in this selection".to_string()
                } else {
                    "No stations to show".to_string()
                };
                empty_state(
                    icon,
                    title,
                    "The catalog did not return anything here — try Popular or run a search."
                        .to_string(),
                    Some(("Browse popular", store)),
                )
            }
        }
    };

    view! {
        <Show
            // Stale-while-revalidate: previously loaded rows stay on screen
            // while the next view loads (the toolbar progress line marks the
            // load). Skeletons appear only when there is nothing to show
            // yet, so a slow tab switch never blanks the grid.
            when=move || !stations.get().is_empty()
            fallback=move || {
                if visible.loading.get() {
                    view! { <GridSkeleton /> }.into_any()
                } else if let Some(message) = visible.view_error.get() {
                    error_state(message, err_store)
                } else {
                    empty()
                }
            }
        >
            {view! {
                // Only the mounted window renders: every fetched row stays
                // reachable (infinite scroll + Load more), but off-window
                // rows never mount cards, images or scopes. Nothing is cut —
                // the tail after display sorting is disproportionately
                // regional or non-Latin-script stations, so it must stay
                // one scroll away, not dropped.
                <For
                    each=move || stations.get()
                    key=|s| station_id(s).unwrap_or_default()
                    let:station
                >
                    <StationCard station=station.clone() />
                </For>
                {move || {
                    let shown = stations.get().len();
                    let total = totals.stations.get().len();
                    if shown < total {
                        view! {
                            <div class="grid-footer">
                                <span class="grid-count">
                                    {format!("Showing {shown} of {total}")}
                                </span>
                                <button
                                    type="button"
                                    class="secondary-button"
                                    on:click=move |_| {
                                        foot_store.with_value(|s| s.show_more());
                                    }
                                >
                                    "Load more"
                                </button>
                            </div>
                        }
                            .into_any()
                    } else {
                        ().into_any()
                    }
                }}
                // Empty reserved height for the not-yet-mounted rows: the
                // scrollbar reflects the full list immediately, while only
                // the window above mounts cards and loads images.
                {move || {
                    let px = spacer_px.spacer_px.get();
                    if px > 0 {
                        view! {
                            <div
                                class="grid-spacer"
                                style=format!("height:{px}px")
                                aria-hidden="true"
                            ></div>
                        }
                            .into_any()
                    } else {
                        ().into_any()
                    }
                }}
            }.into_any()}
        </Show>
    }
}
