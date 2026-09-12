//! Sidebar: navigation, search box, account + admin actions.
//!
//! View closures only capture `Copy` signal handles (a `Show`'s children must
//! stay `Fn`); multi-signal actions go through a `StoredValue<AppState>`.

use leptos::html::Input;
use leptos::prelude::*;

use crate::state::AppState;

#[component]
pub fn Sidebar() -> impl IntoView {
    let st = use_context::<AppState>().expect("AppState provided");
    let store = StoredValue::new(st.clone());
    let view = st.view;
    let user = st.user;
    let sidebar_open = st.sidebar_open;
    let login_open = st.login_open;
    let change_password_open = st.change_password_open;
    let create_user_open = st.create_user_open;
    let search_ref = NodeRef::<Input>::new();

    let nav = |view_name: &'static str, label: &'static str| {
        view! {
            <button
                class="nav"
                class:active=move || view.get() == view_name
                data-view=view_name
                on:click=move |_| {
                    sidebar_open.set(false);
                    store.with_value(|s| s.navigate(view_name, None));
                }
            >
                {label}
            </button>
        }
    };

    let on_search_input = move |_| {
        if let Some(input) = search_ref.get() {
            let query = input.value();
            store.with_value(|s| s.apply_search(query));
        }
    };

    let on_search_click = move |_| {
        if let Some(input) = search_ref.get() {
            let query = input.value();
            store.with_value(|s| s.apply_search(query));
        }
    };

    view! {
        <aside class="sidebar" id="sidebar" class:open=move || sidebar_open.get()>
            <h1>"Radio Browser Plus"</h1>
            <div class="nav-group">
                {nav("all", "All stations")}
                {nav("popular", "Popular")}
                {nav("favorites", "Favorites")}
                {nav("countries", "Countries")}
                {nav("languages", "Languages")}
                {nav("genres", "Genres")}
            </div>

            <div class="search-box">
                <div class="section-divider">"Search"</div>
                <input
                    id="search"
                    type="search"
                    placeholder="Search stations..."
                    node_ref=search_ref
                    on:input=on_search_input
                />
                <button id="search-btn" on:click=on_search_click>"Search"</button>
            </div>

            <div class="sidebar-separator"></div>

            <div class="side-actions-section">
                <div class="section-divider">"Account"</div>
                <div id="account-actions" class="account-actions">
                    <Show when=move || user.with(|u| u.is_none())>
                        <button
                            id="login-button"
                            class="side-action"
                            on:click=move |_| login_open.set(true)
                        >
                            "Login"
                        </button>
                    </Show>
                    <Show when=move || user.with(|u| u.is_some())>
                        <button
                            id="logout-button"
                            class="side-action"
                            on:click=move |_| {
                                store.with_value(|s| {
                                    let s = s.clone();
                                    leptos::task::spawn_local(async move {
                                        let _ = crate::api::logout().await;
                                        s.user.set(None);
                                        s.favorites.set(Default::default());
                                        s.favorites_loaded.set(false);
                                        s.login_open.set(true);
                                    });
                                });
                            }
                        >
                            {move || {
                                user
                                    .with(|u| {
                                        u.as_ref().map(|x| format!("Logout ({})", x.username))
                                    })
                                    .unwrap_or_else(|| "Logout".to_string())
                            }}
                        </button>
                        <button
                            id="change-password-button"
                            class="side-action"
                            on:click=move |_| change_password_open.set(true)
                        >
                            "Change Password"
                        </button>
                    </Show>
                </div>
            </div>

            <Show when=move || {
                user.with(|u| u.as_ref().is_some_and(|x| x.role == "admin"))
            }>
                <div id="admin-actions" class="side-actions-section">
                    <div class="section-divider">"Admin"</div>
                    <button
                        id="create-user-button"
                        class="side-action side-action-half admin-side-action"
                        on:click=move |_| create_user_open.set(true)
                    >
                        "Create User"
                    </button>
                </div>
            </Show>
        </aside>
    }
}
