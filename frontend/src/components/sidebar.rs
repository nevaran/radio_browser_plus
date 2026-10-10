//! Sidebar: brand, navigation, search box, account + admin actions.
//!
//! View closures only capture `Copy` signal handles (a `Show`'s children must
//! stay `Fn`); multi-signal actions go through a `StoredValue<AppState>`.

use leptos::html::Input;
use leptos::prelude::*;

use crate::state::AppState;

/// Small stroke icon for a nav entry; falls back to a dot for unknown views.
fn nav_icon(view_name: &str) -> AnyView {
    let path = match view_name {
        "all" => "M4 5h6v6H4V5Zm10 0h6v4h-6V5ZM4 15h6v4H4v-4Zm10-2h6v6h-6v-6Z",
        "popular" => "M12 3c1.5 3.5 4 5.2 7 5.5-.4 6.4-3.4 10.3-7 12.5-3.6-2.2-6.6-6.1-7-12.5 3-.3 5.5-2 7-5.5Z",
        "favorites" => "M12 20.5S3.8 15.4 3.8 9.6A4.6 4.6 0 0 1 12 7.1a4.6 4.6 0 0 1 8.2 2.5c0 5.8-8.2 10.9-8.2 10.9Z",
        "countries" => "M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18Zm0 0c2.7-2.4 4-5.4 4-9s-1.3-6.6-4-9m0 18c-2.7-2.4-4-5.4-4-9s1.3-6.6 4-9M3.5 9h17m-17 6h17",
        "languages" => "M4 6h9M8.5 4v2m-3 0c.6 3.3 2.3 5.9 5 7.5m0 0c-1 1.9-2.6 3.3-5 4m5-4c1 1.7 2.4 3 4.5 3.8M14.5 20l3.6-9 3.6 9m-6.2-3h5.2",
        "genres" => "M9 18V6l10-2v12M9 18a2.5 2.5 0 1 1-5 0 2.5 2.5 0 0 1 5 0Zm10-2a2.5 2.5 0 1 1-5 0 2.5 2.5 0 0 1 5 0Z",
        _ => "M12 13a1 1 0 1 0 0-2 1 1 0 0 0 0 2Z",
    };
    view! {
        <svg class="nav-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
            <path d=path />
        </svg>
    }
    .into_any()
}

#[component]
fn NavButton(view_name: &'static str, label: &'static str) -> impl IntoView {
    let st = use_context::<AppState>().expect("AppState provided");
    let store = StoredValue::new(st.clone());
    let view = st.view;
    let sidebar_open = st.sidebar_open;
    let favorites = st.favorites;
    let icon = nav_icon(view_name);
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
            {icon}
            <span class="nav-label">{label}</span>
            {move || {
                if view_name == "favorites" {
                    let count = favorites.with(|m| m.len());
                    if count > 0 {
                        return view! { <span class="nav-badge">{count}</span> }.into_any();
                    }
                }
                view! { <span class="nav-spacer"></span> }.into_any()
            }}
        </button>
    }
}

#[component]
pub fn Sidebar() -> impl IntoView {
    let st = use_context::<AppState>().expect("AppState provided");
    let store = StoredValue::new(st.clone());
    let user = st.user;
    let sidebar_open = st.sidebar_open;
    let login_open = st.login_open;
    let allow_guest = st.allow_guest;
    let change_password_open = st.change_password_open;
    let create_user_open = st.create_user_open;
    let search_ref = NodeRef::<Input>::new();
    let query = st.search_query;

    let on_search_input = move |_| {
        if let Some(input) = search_ref.get() {
            let query = input.value();
            store.with_value(|s| s.apply_search(query));
        }
    };

    let clear_search = move |_| {
        if let Some(input) = search_ref.get() {
            input.set_value("");
        }
        store.with_value(|s| s.apply_search(String::new()));
    };

    view! {
        <aside class="sidebar" id="sidebar" class:open=move || sidebar_open.get()>
            <div class="brand">
                <span class="brand-mark" aria-hidden="true">
                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">
                        <path d="M4 9.5 18.5 4a1 1 0 0 1 1.3.9v10.9" />
                        <path d="M4 9.5v8a1.5 1.5 0 0 0 1.5 1.5H14" />
                        <circle cx="8.5" cy="14.5" r="2.5" />
                        <path d="M17 8.2a5 5 0 0 1 0 5.6" />
                    </svg>
                </span>
                <span class="brand-text">
                    <span class="brand-name">"Radio Browser"</span>
                    <span class="brand-tag">"Plus"</span>
                </span>
            </div>

            <nav class="nav-group" aria-label="Browse stations">
                <NavButton view_name="all" label="All stations" />
                <NavButton view_name="popular" label="Popular" />
                <Show when=move || user.with(|u| u.is_some())>
                    <NavButton view_name="favorites" label="Favorites" />
                </Show>
                <NavButton view_name="countries" label="Countries" />
                <NavButton view_name="languages" label="Languages" />
                <NavButton view_name="genres" label="Genres" />
            </nav>

            <div class="search-box">
                <div class="search-field">
                    <svg class="search-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true">
                        <circle cx="11" cy="11" r="7" />
                        <path d="m20 20-3.2-3.2" />
                    </svg>
                    <input
                        id="search"
                        type="search"
                        placeholder="Search stations…"
                        aria-label="Search stations"
                        node_ref=search_ref
                        on:input=on_search_input
                    />
                    <button
                        type="button"
                        class="search-clear"
                        class:hidden=move || query.get().is_empty()
                        aria-label="Clear search"
                        on:click=clear_search
                    >
                        "×"
                    </button>
                </div>
                <p class="search-hint">
                    "Press "
                    <kbd>"/"</kbd>
                    " to search"
                </p>
            </div>

            <div class="sidebar-spacer"></div>

            <div class="side-actions-section">
                <div class="section-divider">"Account"</div>
                <div id="account-actions" class="account-actions">
                    <Show when=move || user.with(|u| u.is_none())>
                        <button
                            id="login-button"
                            class="side-action primary"
                            on:click=move |_| login_open.set(true)
                        >
                            "Sign in"
                        </button>
                        <Show when=move || allow_guest.get()>
                            <p class="guest-note">"Browsing as a guest — sign in to save favorites."</p>
                        </Show>
                    </Show>
                    <Show when=move || user.with(|u| u.is_some())>
                        <div class="account-chip">
                            {move || {
                                let name = user
                                    .with(|u| u.as_ref().map(|x| x.username.clone()))
                                    .unwrap_or_default();
                                let initial = name.chars().next().unwrap_or('?').to_uppercase().to_string();
                                view! { <span class="avatar">{initial}</span> }
                            }}
                            <span class="account-meta">
                                <span class="account-name">
                                    {move || user.with(|u| u.as_ref().map(|x| x.username.clone())).unwrap_or_default()}
                                </span>
                                <span class="account-role">
                                    {move || user.with(|u| u.as_ref().map(|x| x.role.clone())).unwrap_or_default()}
                                </span>
                            </span>
                        </div>
                        <button
                            id="change-password-button"
                            class="side-action ghost"
                            on:click=move |_| change_password_open.set(true)
                        >
                            "Change password"
                        </button>
                        <button
                            id="logout-button"
                            class="side-action ghost"
                            on:click=move |_| {
                                store.with_value(|s| {
                                    let s = s.clone();
                                    leptos::task::spawn_local(async move {
                                        let _ = crate::api::logout().await;
                                        s.user.set(None);
                                        s.favorites.set(Default::default());
                                        s.favorites_loaded.set(false);
                                        if s.allow_guest.get_untracked() {
                                            // Stay in the app as a guest.
                                            s.load_view();
                                        } else {
                                            s.login_open.set(true);
                                        }
                                    });
                                });
                            }
                        >
                            {move || {
                                user
                                    .with(|u| {
                                        u.as_ref().map(|x| format!("Sign out ({})", x.username))
                                    })
                                    .unwrap_or_else(|| "Sign out".to_string())
                            }}
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
                        class="side-action admin-side-action"
                        on:click=move |_| create_user_open.set(true)
                    >
                        "Create user"
                    </button>
                </div>
            </Show>
        </aside>
    }
}
