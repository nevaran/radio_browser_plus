//! In-app modal dialog replacing the browser's `alert()`/`confirm()`.
//!
//! Reads use the `Copy` dialog signal; actions go through a
//! `StoredValue<AppState>`. Renders above the auth modals so error feedback
//! stays visible even with the login dialog open.

use leptos::prelude::*;

use crate::state::{AppDialog, AppState};

#[component]
pub fn Dialog() -> impl IntoView {
    let st = use_context::<AppState>().expect("AppState provided");
    let store = StoredValue::new(st.clone());
    let dialog = st.dialog;

    view! {
        <Show when=move || dialog.get().is_some()>
            {move || match dialog.get() {
                Some(AppDialog::Alert { title, message }) => view! {
                    <div id="app-dialog" class="auth-modal app-dialog" aria-hidden="false">
                        <div
                            class="auth-modal-backdrop"
                            on:click=move |_| {
                                store.with_value(|s| s.close_dialog());
                            }
                        ></div>
                        <div
                            class="auth-modal-content"
                            role="alertdialog"
                            aria-modal="true"
                            aria-labelledby="app-dialog-title"
                            aria-describedby="app-dialog-message"
                        >
                            <div class="auth-modal-header">
                                <h3 id="app-dialog-title">{title}</h3>
                            </div>
                            <p class="dialog-message" id="app-dialog-message">{message}</p>
                            <div class="dialog-actions">
                                <button
                                    id="app-dialog-ok"
                                    class="primary-button"
                                    autofocus
                                    on:click=move |_| {
                                        store.with_value(|s| s.close_dialog());
                                    }
                                >
                                    "OK"
                                </button>
                            </div>
                        </div>
                    </div>
                }
                    .into_any(),
                Some(AppDialog::Confirm {
                    title,
                    message,
                    confirm_label,
                    ..
                }) => view! {
                    <div id="app-dialog" class="auth-modal app-dialog" aria-hidden="false">
                        <div
                            class="auth-modal-backdrop"
                            on:click=move |_| {
                                store.with_value(|s| s.close_dialog());
                            }
                        ></div>
                        <div
                            class="auth-modal-content"
                            role="alertdialog"
                            aria-modal="true"
                            aria-labelledby="app-dialog-title"
                            aria-describedby="app-dialog-message"
                        >
                            <div class="auth-modal-header">
                                <h3 id="app-dialog-title">{title}</h3>
                            </div>
                            <p class="dialog-message" id="app-dialog-message">{message}</p>
                            <div class="dialog-actions">
                                <button
                                    id="app-dialog-cancel"
                                    class="secondary-button"
                                    on:click=move |_| {
                                        store.with_value(|s| s.close_dialog());
                                    }
                                >
                                    "Cancel"
                                </button>
                                <button
                                    id="app-dialog-confirm"
                                    class="primary-button danger"
                                    autofocus
                                    on:click=move |_| {
                                        store.with_value(|s| s.confirm_dialog());
                                    }
                                >
                                    {confirm_label}
                                </button>
                            </div>
                        </div>
                    </div>
                }
                    .into_any(),
                None => view! { <div></div> }.into_any(),
            }}
        </Show>
    }
}
