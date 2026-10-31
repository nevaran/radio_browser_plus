//! Admin-only "Active sessions" overlay.
//!
//! Rendered as a self-contained dialog instead of inline content: the admin
//! panel must never squeeze the station grid or overlap the now-playing bar,
//! especially on narrow screens. The list area scrolls internally, so the
//! dialog always stays inside the viewport.

use leptos::prelude::*;

use crate::models::PresenceEntry;
use crate::state::AppState;

fn format_elapsed_seconds(total: u64) -> String {
    let hours = total / 3_600;
    let minutes = (total % 3_600) / 60;
    let seconds = total % 60;

    match hours {
        0 if minutes == 0 => format!("{seconds}s"),
        0 => format!("{minutes}m {seconds}s"),
        hours => format!("{hours}h {minutes}m {seconds}s"),
    }
}

/// One presence row; station names and IPs may be long, so both this markup
/// and its CSS allow wrapping instead of spilling out of the dialog.
fn session_row(entry: PresenceEntry, fallback_name: &str) -> AnyView {
    let name = entry.username.unwrap_or_else(|| fallback_name.to_string());
    let station = entry.station.unwrap_or_else(|| "No station".to_string());
    let elapsed = format_elapsed_seconds(entry.active_seconds);

    view! {
        <li class="stats-entry">
            <div class="stats-entry-head">
                <strong>{name}</strong>
                <span class="stats-entry-time">{elapsed}</span>
            </div>
            <div class="stats-entry-meta">{station}</div>
            <div class="stats-entry-meta">{entry.ip}</div>
        </li>
    }
    .into_any()
}

#[component]
pub fn ActiveSessionsModal() -> impl IntoView {
    let st = use_context::<AppState>().expect("AppState provided");
    let open = st.admin_stats_open;
    let admin_stats = st.admin_stats;
    let user = st.user;

    view! {
        <Show when=move || {
            open.get() && user.with(|u| u.as_ref().is_some_and(|x| x.role == "admin"))
        }>
            <div id="active-sessions-modal" class="auth-modal admin-stats-modal">
                <div
                    class="auth-modal-backdrop"
                    on:click=move |_| open.set(false)
                ></div>
                <section
                    class="auth-modal-content stats-modal"
                    role="dialog"
                    aria-modal="true"
                    aria-labelledby="active-sessions-title"
                >
                    <div class="auth-modal-header admin-stats-header">
                        <div class="admin-stats-heading">
                            <h3 id="active-sessions-title">"Active sessions"</h3>
                            <p class="admin-stats-subtitle">"Live listeners on this server"</p>
                        </div>
                        <button
                            type="button"
                            class="auth-close"
                            aria-label="Close active sessions"
                            on:click=move |_| open.set(false)
                        >
                            "×"
                        </button>
                    </div>

                    <div class="stats-summary" aria-live="polite">
                        {move || {
                            match admin_stats.get().as_ref() {
                                Some(stats) => view! {
                                    <span>{format!("{} active users", stats.active_users.len())}</span>
                                    <span>{format!("{} guests", stats.guests.len())}</span>
                                }
                                    .into_any(),
                                None => view! { <span>"Loading…"</span> }.into_any(),
                            }
                        }}
                    </div>

                    <div class="admin-stats-body">
                        <div class="stats-columns">
                            <section class="stats-panel" aria-labelledby="stats-users-heading">
                                <h4 id="stats-users-heading">"Users"</h4>
                                <ul class="stats-list">
                                    {move || {
                                        let entries = admin_stats
                                            .get()
                                            .map(|stats| stats.active_users)
                                            .unwrap_or_default();
                                        if entries.is_empty() {
                                            return view! {
                                                <li class="stats-empty">"No active users"</li>
                                            }
                                            .into_any();
                                        }
                                        view! {
                                            <>
                                                {entries
                                                    .into_iter()
                                                    .map(|entry| session_row(entry, "Unknown"))
                                                    .collect::<Vec<_>>()}
                                            </>
                                        }
                                        .into_any()
                                    }}
                                </ul>
                            </section>

                            <section class="stats-panel" aria-labelledby="stats-guests-heading">
                                <h4 id="stats-guests-heading">"Guests"</h4>
                                <ul class="stats-list">
                                    {move || {
                                        let entries = admin_stats
                                            .get()
                                            .map(|stats| stats.guests)
                                            .unwrap_or_default();
                                        if entries.is_empty() {
                                            return view! {
                                                <li class="stats-empty">"No active guests"</li>
                                            }
                                            .into_any();
                                        }
                                        view! {
                                            <>
                                                {entries
                                                    .into_iter()
                                                    .map(|entry| session_row(entry, "Guest"))
                                                    .collect::<Vec<_>>()}
                                            </>
                                        }
                                        .into_any()
                                    }}
                                </ul>
                            </section>
                        </div>
                    </div>
                </section>
            </div>
        </Show>
    }
}
