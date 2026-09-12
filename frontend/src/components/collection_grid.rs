//! Collection grid (Countries / Languages / Genres).

use leptos::prelude::*;

use crate::models::CollectionItem;
use crate::state::AppState;

#[component]
fn CollectionCard(item: CollectionItem) -> impl IntoView {
    let state = use_context::<AppState>().expect("AppState provided");
    let click_state = state.clone();
    let kind = state.collection_kind.get_untracked();
    let label = item.label.clone();
    let filter = item.filter_value.clone();
    let meta = item
        .count
        .map(|c| format!("{c} stations"))
        .unwrap_or_else(|| kind.clone());

    view! {
        <article
            class="station-card"
            data-collection=kind.clone()
            on:click=move |_| click_state.navigate(&kind, Some(filter.clone()))
        >
            <div class="card-art">
                <img src=item.icon_url.clone() alt=label.clone() loading="lazy" />
            </div>
            <div>
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
    let listed = state.clone();

    view! {
        <Show
            when=move || !visible.collections.get().is_empty()
            fallback=|| view! { <div class="station-card"><h3>"No items found"</h3></div> }
        >
            <For
                each=move || listed.collections.get()
                key=|item| format!("{}|{}", item.label, item.filter_value)
                let:item
            >
                <CollectionCard item=item.clone() />
            </For>
        </Show>
    }
}
