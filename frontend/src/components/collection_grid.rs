//! Collection grid (Countries / Languages / Genres).

use leptos::prelude::*;

use crate::models::CollectionItem;
use crate::state::AppState;
use crate::utils::format_count;

use super::station_grid::{empty_icon, empty_state, error_state, GridSkeleton};

#[component]
fn CollectionCard(item: CollectionItem) -> impl IntoView {
    let state = use_context::<AppState>().expect("AppState provided");
    let kind = state.collection_kind.get_untracked();
    let label = item.label.clone();
    let meta = item
        .count
        .map(|c| format_count(c as usize, "station"))
        .unwrap_or_else(|| kind.clone());
    let is_flag = kind == "countries";
    let accessible_name = format!("Browse {label}");
    let icon_url = item.icon_url.clone();
    let filter_value = item.filter_value.clone();

    // Each handler needs its own owned copies (closures must stay `Fn`).
    let click = {
        let state = state.clone();
        let kind = kind.clone();
        let filter = filter_value.clone();
        move || state.navigate(&kind, Some(filter.clone()))
    };
    let click_key = {
        let state = state.clone();
        let kind = kind.clone();
        move || state.navigate(&kind, Some(filter_value.clone()))
    };

    view! {
        <article
            class="collection-card"
            class:is-flag=is_flag
            data-collection=kind.clone()
            role="button"
            tabindex="0"
            aria-label=accessible_name
            on:click=move |_| click()
            on:keydown=move |ev| {
                if ev.key() == "Enter" || ev.code() == "Space" {
                    ev.prevent_default();
                    click_key();
                }
            }
        >
            <div class="card-art">
                <img src=icon_url alt="" loading="lazy" decoding="async" />
                <div class="card-overlay" aria-hidden="true">
                    <span class="card-play chevron">
                        <svg viewBox="0 0 24 24" aria-hidden="true">
                            <path d="m9.5 5.5 7 6.5-7 6.5" />
                        </svg>
                    </span>
                </div>
            </div>
            <div class="card-body">
                <div class="station-name">{label}</div>
                <div class="station-meta">{meta}</div>
            </div>
        </article>
    }
}

#[component]
pub fn CollectionGrid() -> impl IntoView {
    let state = use_context::<AppState>().expect("AppState provided");
    let visible = state.clone();
    let has_items = state.clone();
    let listed = state.clone();
    let grid_limit = state.grid_limit;
    let err_store = StoredValue::new(state.clone());
    let spacer_state = state.clone();
    let spacer_px = state.clone();

    // Same scroll reservation as the station grid (see StationGrid).
    Effect::new(move |_| {
        spacer_state.grid_limit.get();
        let _ = spacer_state.collections.with(|rows| rows.len());
        let _ = spacer_state.search_query.get();
        spacer_state.resize_tick.get();
        spacer_state.update_spacer();
    });

    // Same windowing as the station grid: mount only the visible page.
    let collection_list = Memo::new(move |_| listed.display_collections());
    let items = Memo::new(move |_| {
        let mut list = collection_list.get();
        list.truncate(grid_limit.get());
        list
    });

    view! {
        <Show
            // Same stale-while-revalidate contract as the station grid.
            when=move || {
                has_items.visible_collection_count() > 0
            }
            fallback=move || {
                if visible.loading.get() {
                    view! { <GridSkeleton /> }.into_any()
                } else if let Some(message) = visible.view_error.get() {
                    error_state(message, err_store)
                } else if !visible.search_query.get().is_empty() {
                    empty_state(
                        empty_icon("search"),
                        "No items matched".to_string(),
                        "Try another search or clear the search field to see the full list."
                            .to_string(),
                        None,
                    )
                } else {
                    empty_state(
                        empty_icon("globe"),
                        "Nothing to browse".to_string(),
                        "This list came back empty — try again in a moment.".to_string(),
                        None,
                    )
                }
            }
        >
            {view! {
                <For
                    each=move || items.get()
                    key=|item| format!("{}|{}", item.label, item.filter_value)
                    let:item
                >
                    <CollectionCard item=item.clone() />
                </For>
                {move || {
                    let shown = items.get().len();
                    let total = collection_list.get().len();
                    if shown < total {
                        view! {
                            <div class="grid-footer">
                                <span class="grid-count">
                                    {format!("Showing {shown} of {total}")}
                                </span>
                            </div>
                        }
                            .into_any()
                    } else {
                        ().into_any()
                    }
                }}
                // Empty reserved height for the not-yet-mounted rows (see
                // StationGrid): full-size scrollbar, unloaded off-screen.
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
