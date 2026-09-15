mod api;
mod app;
mod components;
mod media;
mod models;
mod player;
mod state;
mod utils;

use app::App;
use leptos::prelude::*;

fn main() {
    console_error_panic_hook::set_once();
    mount_to_body(|| view! { <App /> });
}
