//! Browser audio playback with the same retry/volume semantics as the old JS UI.
//!
//! One intentional fix over the original: the old `attemptRetry` cleared
//! `isPlaying` *before* checking it, so its retry chain could never run. Here
//! a dedicated `wants_playing` flag tracks playback intent, so the configured
//! 5 × 3s retry chain actually works.
//!
//! Station switches are gapless: the previous stream keeps playing until the
//! new one fires its first `playing` event, and `is_playing` stays true
//! throughout, so there is neither a silence gap nor a play-button flicker.

use std::sync::{Arc, Mutex};

use gloo_storage::{LocalStorage, Storage};
use gloo_timers::future::TimeoutFuture;
use leptos::prelude::*;
use wasm_bindgen::{closure::Closure, JsCast};
use wasm_bindgen_futures::JsFuture;
use web_sys::HtmlAudioElement;

use crate::models::{station_stream_url, Station};

const VOLUME_KEY: &str = "radio-browser-plus-volume";
const MUTED_KEY: &str = "radio-browser-plus-muted";
const MAX_RETRIES: u32 = 5;
const RETRY_DELAY_MS: u32 = 3000;

#[derive(Clone)]
pub struct Player {
    pub current: RwSignal<Option<Station>>,
    pub is_playing: RwSignal<bool>,
    pub loading: RwSignal<bool>,
    pub volume: RwSignal<f64>,
    pub muted: RwSignal<bool>,
    wants_playing: RwSignal<bool>,
    audio: Arc<Mutex<Option<HtmlAudioElement>>>,
    /// Element parked for gapless handoff: the previously-live stream, kept
    /// sounding until the newly adopted one proves itself. Always `None`
    /// outside the brief switch window; every path that can observe it
    /// (`play`, `pause`, first `play`/`error` of the new element) drains it.
    previous: Arc<Mutex<Option<HtmlAudioElement>>>,
    retry_attempt: Arc<Mutex<u32>>,
    retry_gen: Arc<Mutex<u64>>,
}

impl Player {
    pub fn new() -> Self {
        let saved_volume: f64 = LocalStorage::get(VOLUME_KEY).unwrap_or(1.0);
        let muted: bool = LocalStorage::get(MUTED_KEY).unwrap_or(false);
        Self {
            current: RwSignal::new(None),
            is_playing: RwSignal::new(false),
            loading: RwSignal::new(false),
            volume: RwSignal::new(saved_volume.clamp(0.0, 1.0)),
            muted: RwSignal::new(muted),
            wants_playing: RwSignal::new(false),
            audio: Arc::new(Mutex::new(None)),
            previous: Arc::new(Mutex::new(None)),
            retry_attempt: Arc::new(Mutex::new(0)),
            retry_gen: Arc::new(Mutex::new(0)),
        }
    }

    fn cancel_retry(&self) {
        *self.retry_gen.lock().unwrap() += 1;
        *self.retry_attempt.lock().unwrap() = 0;
    }

    /// Detach an element's handlers so its late events can no longer touch
    /// player state. A parked element stays dumb but audible until teardown.
    fn detach(audio: &HtmlAudioElement) {
        audio.set_onplay(None);
        audio.set_onpause(None);
        audio.set_onended(None);
        audio.set_onwaiting(None);
        audio.set_oncanplay(None);
        audio.set_onerror(None);
    }

    /// Detach handlers, stop, and release an element.
    fn silence(audio: &HtmlAudioElement) {
        Self::detach(audio);
        let _ = audio.pause();
        audio.set_src("");
    }

    /// Drain a parked handoff element, silencing it if still present.
    /// `Option::take` makes this a one-shot: only the first caller wins.
    fn teardown_previous(&self) {
        if let Some(prev) = self.previous.lock().unwrap().take() {
            Self::silence(&prev);
        }
    }

    pub fn play(&self, station: Station) {
        self.cancel_retry();

        let Some(stream) = station_stream_url(&station) else {
            return;
        };
        let audio = match HtmlAudioElement::new_with_src(&stream) {
            Ok(a) => a,
            Err(_) => return,
        };
        audio.set_volume(if self.muted.get_untracked() {
            0.0
        } else {
            self.volume.get_untracked()
        });

        // Park the live element instead of stopping it: it keeps sounding
        // until the new stream's first `playing` event completes the handoff
        // (or its first error aborts into the retry path). Its handlers are
        // detached up front so nothing it does meanwhile (e.g. ending
        // naturally mid-handoff) can corrupt player state.
        self.teardown_previous();
        let live = self.audio.lock().unwrap().replace(audio.clone());
        if let Some(ref prev) = live {
            Self::detach(prev);
        }
        *self.previous.lock().unwrap() = live;

        self.current.set(Some(station.clone()));
        self.wants_playing.set(true);
        // Stays true across the switch: audio never stops in the success
        // case, so the button must not flicker to "not playing".
        self.is_playing.set(true);
        self.loading.set(true);
        self.attach_handlers(&audio);

        let this = self.clone();
        leptos::task::spawn_local(async move {
            let failed = match audio.play() {
                Ok(promise) => JsFuture::from(promise).await.is_err(),
                Err(_) => true,
            };
            if failed {
                this.teardown_previous();
                this.is_playing.set(false);
                this.schedule_retry();
            }
        });
    }

    fn attach_handlers(&self, audio: &HtmlAudioElement) {
        let this = self.clone();
        let on_play = Closure::wrap(Box::new(move || {
            // First `playing` of a fresh element completes any pending
            // handoff; later ones (e.g. after resume) find nothing to do.
            this.teardown_previous();
            // Guarded: a pause issued while buffering must win over the
            // still-resolving play promise.
            if this.wants_playing.get_untracked() {
                this.is_playing.set(true);
            }
            this.loading.set(false);
        }) as Box<dyn Fn()>);
        audio.set_onplay(Some(on_play.as_ref().unchecked_ref()));
        on_play.forget();

        let this = self.clone();
        let on_pause = Closure::wrap(Box::new(move || {
            this.is_playing.set(false);
            this.loading.set(false);
        }) as Box<dyn Fn()>);
        audio.set_onpause(Some(on_pause.as_ref().unchecked_ref()));
        on_pause.forget();

        let this = self.clone();
        let on_ended = Closure::wrap(Box::new(move || {
            this.wants_playing.set(false);
            this.is_playing.set(false);
            this.loading.set(false);
        }) as Box<dyn Fn()>);
        audio.set_onended(Some(on_ended.as_ref().unchecked_ref()));
        on_ended.forget();

        let this = self.clone();
        let on_waiting = Closure::wrap(Box::new(move || {
            this.loading.set(true);
        }) as Box<dyn Fn()>);
        audio.set_onwaiting(Some(on_waiting.as_ref().unchecked_ref()));
        on_waiting.forget();

        let this = self.clone();
        let on_canplay = Closure::wrap(Box::new(move || {
            this.loading.set(false);
        }) as Box<dyn Fn()>);
        audio.set_oncanplay(Some(on_canplay.as_ref().unchecked_ref()));
        on_canplay.forget();

        let this = self.clone();
        let on_error = Closure::wrap(Box::new(move || {
            // Abort a pending handoff: the parked element must not keep
            // sounding behind a failed switch, and the retry path below takes
            // over the adopted element.
            this.teardown_previous();
            this.is_playing.set(false);
            this.schedule_retry();
        }) as Box<dyn Fn()>);
        audio.set_onerror(Some(on_error.as_ref().unchecked_ref()));
        on_error.forget();
    }

    fn schedule_retry(&self) {
        if !self.wants_playing.get_untracked() {
            self.loading.set(false);
            return;
        }
        if *self.retry_attempt.lock().unwrap() >= MAX_RETRIES {
            *self.retry_attempt.lock().unwrap() = 0;
            self.loading.set(false);
            return;
        }
        *self.retry_attempt.lock().unwrap() += 1;
        self.loading.set(true);
        *self.retry_gen.lock().unwrap() += 1;
        let gen = *self.retry_gen.lock().unwrap();
        let this = self.clone();
        leptos::task::spawn_local(async move {
            TimeoutFuture::new(RETRY_DELAY_MS).await;
            if *this.retry_gen.lock().unwrap() != gen || !this.wants_playing.get_untracked() {
                return;
            }
            let (audio_opt, station_opt) = (
                this.audio.lock().unwrap().clone(),
                this.current.get_untracked(),
            );
            let (Some(audio), Some(station)) = (audio_opt, station_opt) else {
                this.loading.set(false);
                return;
            };
            let Some(stream) = station_stream_url(&station) else {
                this.loading.set(false);
                return;
            };
            audio.set_src(&stream);
            let failed = match audio.play() {
                Ok(promise) => JsFuture::from(promise).await.is_err(),
                Err(_) => true,
            };
            if failed {
                this.schedule_retry();
            } else {
                this.is_playing.set(true);
                *this.retry_attempt.lock().unwrap() = 0;
            }
        });
    }

    pub fn pause(&self) {
        self.cancel_retry();
        // Also stop a parked handoff element, otherwise it would keep
        // sounding with no remaining handle on it.
        self.teardown_previous();
        self.wants_playing.set(false);
        self.loading.set(false);
        if let Some(audio) = self.audio.lock().unwrap().as_ref() {
            let _ = audio.pause();
        }
        self.is_playing.set(false);
    }

    pub fn resume(&self) {
        let audio = self.audio.lock().unwrap().clone();
        let Some(audio) = audio else { return };
        if self.current.get_untracked().is_none() {
            return;
        }
        self.wants_playing.set(true);
        self.cancel_retry();
        let this = self.clone();
        leptos::task::spawn_local(async move {
            let failed = match audio.play() {
                Ok(promise) => JsFuture::from(promise).await.is_err(),
                Err(_) => true,
            };
            if failed {
                this.is_playing.set(false);
                this.schedule_retry();
            } else {
                this.is_playing.set(true);
            }
        });
    }

    pub fn set_volume(&self, value: f64) {
        let clamped = value.clamp(0.0, 1.0);
        self.muted.set(false);
        let _ = LocalStorage::set(MUTED_KEY, false);
        self.volume.set(clamped);
        let _ = LocalStorage::set(VOLUME_KEY, clamped);
        if let Some(audio) = self.audio.lock().unwrap().as_ref() {
            audio.set_volume(clamped);
        }
    }

    pub fn toggle_mute(&self) {
        let muted = !self.muted.get_untracked();
        self.muted.set(muted);
        let _ = LocalStorage::set(MUTED_KEY, muted);
        if let Some(audio) = self.audio.lock().unwrap().as_ref() {
            audio.set_volume(if muted {
                0.0
            } else {
                self.volume.get_untracked()
            });
        }
    }

    /// Icon for the mute button, same thresholds as before.
    pub fn volume_icon(muted: bool, volume: f64) -> &'static str {
        if muted {
            "🔇"
        } else if volume <= 0.0 {
            "🔈"
        } else if volume < 0.5 {
            "🔉"
        } else {
            "🔊"
        }
    }
}
