//! OS media-controls integration via the Media Session API.
//!
//! Uses raw `js_sys::Reflect` interop instead of `web_sys`'s typed
//! `MediaSession` bindings so the build stays on stable web-sys (those
//! bindings still require `--cfg=web_sys_unstable_apis`). Every helper is a
//! silent no-op where the API is missing, so private-mode / old browsers
//! simply get no lock-screen controls instead of an error.

use wasm_bindgen::{closure::Closure, JsCast, JsValue};

use crate::models::Station;
use crate::utils::{primary_image, station_genre, PLACEHOLDER_SVG};

/// `navigator.mediaSession`, or `None` where unsupported.
fn media_session() -> Option<JsValue> {
    let window = web_sys::window()?;
    let navigator: JsValue = window.navigator().into();
    let session = js_sys::Reflect::get(&navigator, &JsValue::from_str("mediaSession")).ok()?;
    if session.is_undefined() || session.is_null() {
        return None;
    }
    Some(session)
}

/// Publish the current station to the OS (lock screen, headset display, ...).
pub fn update_media_metadata(station: &Station) {
    let Some(session) = media_session() else {
        return;
    };
    let Some(window) = web_sys::window() else {
        return;
    };
    let window: JsValue = window.into();
    let ctor = js_sys::Reflect::get(&window, &JsValue::from_str("MediaMetadata")).ok();
    let ctor = ctor.filter(|c| c.is_function());
    let Some(ctor) = ctor else {
        return;
    };
    let ctor: js_sys::Function = ctor.unchecked_into();

    let title = station.name.trim();
    let title = if title.is_empty() {
        "Unknown station".to_string()
    } else {
        title.to_string()
    };
    let artist = station_genre(station);
    let album = station
        .country
        .as_ref()
        .map(|c| c.trim())
        .filter(|c| !c.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| "Radio Browser Plus".to_string());

    let init = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&init, &JsValue::from_str("title"), &JsValue::from_str(&title));
    let _ = js_sys::Reflect::set(
        &init,
        &JsValue::from_str("artist"),
        &JsValue::from_str(&artist),
    );
    let _ = js_sys::Reflect::set(&init, &JsValue::from_str("album"), &JsValue::from_str(&album));

    // Data-URL placeholders must not reach the OS: they are large and render
    // as noise on the lock screen. Only real (http) artwork is advertised.
    let image = primary_image(station);
    if image != PLACEHOLDER_SVG && image.starts_with("http") {
        let art = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&art, &JsValue::from_str("src"), &JsValue::from_str(&image));
        let _ = js_sys::Reflect::set(
            &art,
            &JsValue::from_str("sizes"),
            &JsValue::from_str("512x512"),
        );
        let artwork = js_sys::Array::new();
        artwork.push(&art);
        let _ = js_sys::Reflect::set(&init, &JsValue::from_str("artwork"), &artwork);
    }

    let metadata = js_sys::Reflect::construct(&ctor, &js_sys::Array::of1(&init));
    if let Ok(metadata) = metadata {
        let _ = js_sys::Reflect::set(&session, &JsValue::from_str("metadata"), &metadata);
    }
}

/// Clear previously published metadata (e.g. after logout stops playback).
pub fn clear_media_metadata() {
    let Some(session) = media_session() else {
        return;
    };
    let _ = js_sys::Reflect::set(
        &session,
        &JsValue::from_str("metadata"),
        &JsValue::NULL,
    );
}

/// Mirror the player state so the OS shows play vs. pause correctly.
pub fn update_playback_state(is_playing: bool) {
    let Some(session) = media_session() else {
        return;
    };
    let _ = js_sys::Reflect::set(
        &session,
        &JsValue::from_str("playbackState"),
        &JsValue::from_str(if is_playing { "playing" } else { "paused" }),
    );
}

fn set_handler(session: &JsValue, action: &str, handler: &js_sys::Function) {
    let set = js_sys::Reflect::get(session, &JsValue::from_str("setActionHandler"));
    let Ok(set) = set else {
        return;
    };
    if !set.is_function() {
        return;
    }
    let set: js_sys::Function = set.unchecked_into();
    // Unsupported actions throw NotSupportedError — intentionally ignored.
    let _ = set.call2(session, &JsValue::from_str(action), handler);
}

/// Register OS media-key handlers for the lifetime of the app. Each closure
/// is leaked (like the other global listeners in `app.rs`) because the
/// session outlives any single view.
pub fn init_media_handlers(
    on_play: impl Fn() + 'static,
    on_pause: impl Fn() + 'static,
    on_previous: impl Fn() + 'static,
    on_next: impl Fn() + 'static,
) {
    let Some(session) = media_session() else {
        return;
    };

    let play = Closure::wrap(Box::new(move |_: JsValue| on_play()) as Box<dyn Fn(JsValue)>);
    set_handler(&session, "play", play.as_ref().unchecked_ref());
    play.forget();

    let pause = Closure::wrap(Box::new(move |_: JsValue| on_pause()) as Box<dyn Fn(JsValue)>);
    set_handler(&session, "pause", pause.as_ref().unchecked_ref());
    pause.forget();

    let previous =
        Closure::wrap(Box::new(move |_: JsValue| on_previous()) as Box<dyn Fn(JsValue)>);
    set_handler(&session, "previoustrack", previous.as_ref().unchecked_ref());
    previous.forget();

    let next = Closure::wrap(Box::new(move |_: JsValue| on_next()) as Box<dyn Fn(JsValue)>);
    set_handler(&session, "nexttrack", next.as_ref().unchecked_ref());
    next.forget();
}
