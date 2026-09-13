//! Login / create-user / change-password dialogs.
//!
//! Reads use `Copy` signal handles; submissions go through a
//! `StoredValue<AppState>`.

use leptos::html::{Input, Select};
use leptos::prelude::*;

use crate::state::AppState;

/// Shared dialog shell: backdrop, centered content, titled header. The form
/// body differs per dialog and arrives as children. Dismissal (backdrop and
/// ×) stays blocked while logged out so the login dialog cannot be bypassed —
/// except in guest mode, where a voluntarily opened dialog must stay closable.
#[component]
fn ModalShell(
    modal_id: &'static str,
    title: &'static str,
    open: RwSignal<bool>,
    user: RwSignal<Option<crate::models::User>>,
    allow_guest: RwSignal<bool>,
    children: ChildrenFn,
) -> impl IntoView {
    let title_id = format!("{modal_id}-title");
    let close_title_id = title_id.clone();
    view! {
        <div id=modal_id class="auth-modal" aria-hidden="false">
            <div
                class="auth-modal-backdrop"
                on:click=move |_| {
                    if user.with(|u| u.is_some()) || allow_guest.get() {
                        open.set(false);
                    }
                }
            ></div>
            <div
                class="auth-modal-content"
                role="dialog"
                aria-modal="true"
                aria-labelledby=title_id
            >
                <div class="auth-modal-header">
                    <h3 id=close_title_id>{title}</h3>
                    <button
                        type="button"
                        class="auth-close"
                        data-close=modal_id
                        aria-label="Close"
                        on:click=move |_| {
                            if user.with(|u| u.is_some()) || allow_guest.get() {
                                open.set(false);
                            }
                        }
                    >
                        "×"
                    </button>
                </div>
                {children()}
            </div>
        </div>
    }
}

#[component]
pub fn LoginModal() -> impl IntoView {
    let st = use_context::<AppState>().expect("AppState provided");
    let store = StoredValue::new(st.clone());
    let login_open = st.login_open;
    let user = st.user;
    let allow_guest = st.allow_guest;
    let user_ref = NodeRef::<Input>::new();
    let pass_ref = NodeRef::<Input>::new();

    view! {
        <Show when=move || login_open.get()>
            <ModalShell
                modal_id="login-modal"
                title="Sign in"
                open=login_open
                user=user
                allow_guest=allow_guest
            >
                    <form
                        id="login-form"
                        class="auth-form"
                        on:submit=move |ev| {
                            ev.prevent_default();
                            let username = user_ref
                                .get()
                                .map(|el| el.value().trim().to_string())
                                .unwrap_or_default();
                            let password = pass_ref.get().map(|el| el.value()).unwrap_or_default();
                            if username.is_empty() || password.is_empty() {
                                return;
                            }
                            store.with_value(|s| {
                                let s = s.clone();
                                leptos::task::spawn_local(async move {
                                    match crate::api::login(&username, &password).await {
                                        Ok(user) => {
                                            s.session_checked.set(true);
                                            s.user.set(Some(user));
                                            s.login_open.set(false);
                                            s.ensure_favorites().await;
                                            s.load_view();
                                        }
                                        Err(e) => s.show_alert("Sign in", e),
                                    }
                                });
                            });
                        }
                    >
                        <label>
                            <span>"Username"</span>
                            <input
                                id="login-username"
                                name="username"
                                type="text"
                                autocomplete="username"
                                required
                                node_ref=user_ref
                            />
                        </label>
                        <label>
                            <span>"Password"</span>
                            <input
                                id="login-password"
                                name="password"
                                type="password"
                                autocomplete="current-password"
                                required
                                node_ref=pass_ref
                            />
                        </label>
                        <button type="submit" class="primary-button">"Login"</button>
                    </form>
            </ModalShell>
        </Show>
    }
}

#[component]
pub fn CreateUserModal() -> impl IntoView {
    let st = use_context::<AppState>().expect("AppState provided");
    let store = StoredValue::new(st.clone());
    let create_user_open = st.create_user_open;
    let user = st.user;
    let allow_guest = st.allow_guest;
    let user_ref = NodeRef::<Input>::new();
    let pass_ref = NodeRef::<Input>::new();
    let role_ref = NodeRef::<Select>::new();

    view! {
        <Show when=move || {
            create_user_open.get()
                && user.with(|u| u.as_ref().is_some_and(|x| x.role == "admin"))
        }>
            <ModalShell
                modal_id="create-user-modal"
                title="Create User"
                open=create_user_open
                user=user
                allow_guest=allow_guest
            >
                    <form
                        id="create-user-form"
                        class="auth-form"
                        on:submit=move |ev| {
                            ev.prevent_default();
                            let is_admin = store
                                .with_value(|s| s.is_admin());
                            if !is_admin {
                                return;
                            }
                            let username = user_ref
                                .get()
                                .map(|el| el.value().trim().to_string())
                                .unwrap_or_default();
                            let password = pass_ref.get().map(|el| el.value()).unwrap_or_default();
                            let role = role_ref
                                .get()
                                .map(|el| el.value())
                                .unwrap_or_else(|| "listener".to_string());
                            if username.is_empty() || password.is_empty() {
                                return;
                            }
                            store.with_value(|s| {
                                let s = s.clone();
                                leptos::task::spawn_local(async move {
                                    match crate::api::create_user(&username, &password, &role).await
                                    {
                                        Ok(_) => {
                                            s.create_user_open.set(false);
                                            s.show_alert(
                                                "Create User",
                                                format!(
                                                    "User \"{username}\" created successfully."
                                                ),
                                            );
                                        }
                                        Err(e) => s.show_alert("Create User", e),
                                    }
                                });
                            });
                        }
                    >
                        <label>
                            <span>"Username"</span>
                            <input
                                id="create-user-username"
                                name="username"
                                type="text"
                                autocomplete="username"
                                required
                                node_ref=user_ref
                            />
                        </label>
                        <label>
                            <span>"Password"</span>
                            <input
                                id="create-user-password"
                                name="password"
                                type="password"
                                autocomplete="new-password"
                                required
                                node_ref=pass_ref
                            />
                        </label>
                        <label>
                            <span>"Role"</span>
                            <select id="create-user-role" name="role" node_ref=role_ref>
                                <option value="listener">"Listener"</option>
                                <option value="admin">"Admin"</option>
                            </select>
                        </label>
                        <button type="submit" class="primary-button">"Create User"</button>
                    </form>
            </ModalShell>
        </Show>
    }
}

#[component]
pub fn ChangePasswordModal() -> impl IntoView {
    let st = use_context::<AppState>().expect("AppState provided");
    let store = StoredValue::new(st.clone());
    let change_password_open = st.change_password_open;
    let user = st.user;
    let allow_guest = st.allow_guest;
    let current_ref = NodeRef::<Input>::new();
    let new_ref = NodeRef::<Input>::new();

    view! {
        <Show when=move || change_password_open.get() && user.with(|u| u.is_some())>
            <ModalShell
                modal_id="change-password-modal"
                title="Change Password"
                open=change_password_open
                user=user
                allow_guest=allow_guest
            >
                    <form
                        id="change-password-form"
                        class="auth-form"
                        on:submit=move |ev| {
                            ev.prevent_default();
                            let logged_in = store.with_value(|s| s.is_logged_in());
                            if !logged_in {
                                return;
                            }
                            let old = current_ref.get().map(|el| el.value()).unwrap_or_default();
                            let new = new_ref.get().map(|el| el.value()).unwrap_or_default();
                            if old.is_empty() || new.is_empty() {
                                return;
                            }
                            store.with_value(|s| {
                                let s = s.clone();
                                leptos::task::spawn_local(async move {
                                    match crate::api::change_password(&old, &new).await {
                                        Ok(_) => {
                                            s.change_password_open.set(false);
                                            s.show_alert(
                                                "Change Password",
                                                "Password updated successfully.",
                                            );
                                        }
                                        Err(e) => s.show_alert("Change Password", e),
                                    }
                                });
                            });
                        }
                    >
                        <label>
                            <span>"Current Password"</span>
                            <input
                                id="change-current-password"
                                name="old_password"
                                type="password"
                                autocomplete="current-password"
                                required
                                node_ref=current_ref
                            />
                        </label>
                        <label>
                            <span>"New Password"</span>
                            <input
                                id="change-new-password"
                                name="new_password"
                                type="password"
                                autocomplete="new-password"
                                required
                                node_ref=new_ref
                            />
                        </label>
                        <button type="submit" class="primary-button">"Update Password"</button>
                    </form>
            </ModalShell>
        </Show>
    }
}
