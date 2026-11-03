//! Share window: read-only deep-link field plus a copy-to-clipboard action.
//!
//! Opened by [`crate::state::AppState::share_station`]; the URL travels in
//! `AppState::share_open` (`None` = closed). Reads use the `Copy` signal
//! handle; the copy action reaches for `navigator.clipboard` first and falls
//! back to a selection copy for plain-HTTP contexts where the API is absent.

use leptos::html::Input;
use leptos::prelude::*;

use crate::state::AppState;

#[component]
pub fn ShareModal() -> impl IntoView {
    let st = use_context::<AppState>().expect("AppState provided");
    let share_open = st.share_open;
    let link_ref = NodeRef::<Input>::new();
    // "Copy" feedback persists until the window closes or reopens.
    let copied = RwSignal::new(false);
    let failed = RwSignal::new(false);

    // Every opening starts with fresh feedback.
    Effect::new(move |_| {
        if share_open.get().is_some() {
            copied.set(false);
            failed.set(false);
        }
    });

    view! {
        <Show when=move || share_open.get().is_some()>
            <div id="share-modal" class="auth-modal" aria-hidden="false">
                <div
                    class="auth-modal-backdrop"
                    on:click=move |_| share_open.set(None)
                ></div>
                <div
                    class="auth-modal-content"
                    role="dialog"
                    aria-modal="true"
                    aria-labelledby="share-modal-title"
                >
                    <div class="auth-modal-header">
                        <h3 id="share-modal-title">"Share station"</h3>
                        <button
                            type="button"
                            class="auth-close"
                            aria-label="Close"
                            on:click=move |_| share_open.set(None)
                        >
                            "×"
                        </button>
                    </div>
                    <input
                        id="share-link"
                        class="share-link-input"
                        type="text"
                        name="share-link"
                        node_ref=link_ref
                        prop:value=move || share_open.get().unwrap_or_default()
                        prop:readOnly=true
                        spellcheck="false"
                        // Selecting the whole link on focus keeps the copy
                        // button the only thing that actually needs clicking.
                        on:focus=move |_| {
                            if let Some(link) = link_ref.get() {
                                link.select();
                            }
                        }
                    />
                    <div class="dialog-actions">
                        <button
                            type="button"
                            class="secondary-button"
                            on:click=move |_| share_open.set(None)
                        >
                            "Close"
                        </button>
                        <button
                            type="button"
                            id="share-copy"
                            class="primary-button"
                            on:click=move |_| {
                                let Some(url) = share_open.get_untracked() else {
                                    return;
                                };
                                if url.is_empty() {
                                    failed.set(true);
                                    return;
                                }
                                leptos::task::spawn_local(async move {
                                    if crate::media::clipboard_write_text(&url).await {
                                        failed.set(false);
                                        copied.set(true);
                                        return;
                                    }
                                    // Plain-HTTP serving has no clipboard API:
                                    // select the field and use the legacy copy
                                    // command within this user gesture.
                                    let fallback = link_ref
                                        .get()
                                        .map(|link| {
                                            let _ = link.focus();
                                            link.select();
                                            crate::media::copy_selection()
                                        })
                                        .unwrap_or(false);
                                    copied.set(fallback);
                                    failed.set(!fallback);
                                });
                            }
                        >
                            {move || if copied.get() { "Copied!" } else { "Copy" }}
                        </button>
                    </div>
                    <Show when=move || failed.get()>
                        <p class="share-error">
                            "Copying failed — select the link and copy it manually."
                        </p>
                    </Show>
                </div>
            </div>
        </Show>
    }
}
