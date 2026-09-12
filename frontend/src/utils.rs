//! Pure helper functions ported from the previous `app.js` frontend.

use crate::models::{station_id, Station};

pub const PLACEHOLDER_SVG: &str = "data:image/svg+xml;base64,PHN2ZyB3aWR0aD0iNjQiIGhlaWdodD0iNjQiIHZpZXdCb3g9IjAgMCA2NCA2NCIgZmlsbD0ibm9uZSIgeG1sbnM9Imh0dHA6Ly93d3cudzMub3JnLzIwMDAvc3ZnIj48Y2lyY2xlIGN4PSIzMiIgY3k9IjMyIiByPSIzMiIgZmlsbD0iIzJkMzU0OCIvPjxwYXRoIGQ9Ik0zMiAxNkMxNiAxNiAxNiAzMiAxNiA0OEgxNkMxNiAzMiAyNCAzMiAzMiAzMkM0MCAzMiA0OCAzMiA0OCA0OEg0OEM0OCAzMiA0OCAxNiAzMiAxNloiIGZpbGw9IiM5Y2EzYWYiLz48L3N2Zz4=";

pub const VALID_VIEWS: &[&str] = &[
    "all",
    "popular",
    "favorites",
    "countries",
    "languages",
    "tags",
    "genres",
    "search",
];

pub fn truncate_name(name: &str, max_len: usize) -> String {
    let value = name.trim();
    if value.is_empty() {
        return "Unknown station".to_string();
    }
    if value.chars().count() > max_len {
        let truncated: String = value.chars().take(max_len).collect();
        format!("{truncated}...")
    } else {
        value.to_string()
    }
}

pub fn normalize_view_name(view: &str) -> String {
    let normalized = view.trim().to_lowercase();
    if VALID_VIEWS.contains(&normalized.as_str()) {
        normalized
    } else {
        "all".to_string()
    }
}

pub fn format_bitrate(bitrate: Option<u32>) -> String {
    match bitrate {
        Some(b) if b > 0 => format!("{b} kbps"),
        _ => "Stream".to_string(),
    }
}

pub fn station_genre(station: &Station) -> String {
    if let Some(tags) = station
        .tags
        .as_ref()
        .map(|t| t.trim())
        .filter(|t| !t.is_empty())
    {
        return tags.to_string();
    }
    if let Some(genre) = station
        .genre
        .as_ref()
        .map(|g| g.trim())
        .filter(|g| !g.is_empty())
    {
        return genre.to_string();
    }
    if let Some(lang) = station
        .language
        .as_ref()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
    {
        return lang.to_string();
    }
    "Unknown genre".to_string()
}

/// Case-insensitive ascending sort by display name, mirroring `sortByNameAsc`.
/// Runs directly in WASM — fast enough (<5ms for 5k stations) that the old
/// `sort-worker.js` Web Worker is no longer needed.
pub fn sort_by_name_asc<T, F>(items: &mut [T], name_of: F)
where
    F: Fn(&T) -> String,
{
    items.sort_by(|a, b| {
        name_of(a)
            .trim()
            .to_lowercase()
            .cmp(&name_of(b).trim().to_lowercase())
    });
}

pub fn station_sort_key(station: &Station) -> String {
    if !station.name.trim().is_empty() {
        station.name.clone()
    } else {
        station_id(station).unwrap_or_default()
    }
}

/// Image fallback chain: direct favicon -> homepage favicon service -> placeholder.
pub fn image_candidates(station: &Station) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(favicon) = station
        .favicon
        .as_ref()
        .map(|f| f.trim())
        .filter(|f| !f.is_empty())
    {
        out.push(favicon.to_string());
    }
    let homepage = station
        .homepage
        .clone()
        .filter(|h| !h.trim().is_empty())
        .or_else(|| station.url.clone().filter(|u| !u.trim().is_empty()));
    if let Some(page) = homepage {
        // Derive an origin without pulling in a URL parser.
        let origin = page
            .split(['?', '#'])
            .next()
            .unwrap_or(&page)
            .trim_end_matches('/')
            .to_string();
        if !origin.is_empty() {
            // `domain_url` must be percent-encoded; keep it to the unreserved set.
            let encoded: String = origin
                .bytes()
                .map(|b| match b {
                    b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                        (b as char).to_string()
                    }
                    _ => format!("%{b:02X}"),
                })
                .collect();
            out.push(format!(
                "https://www.google.com/s2/favicons?sz=64&domain_url={encoded}"
            ));
        }
    }
    out.dedup();
    out
}

/// Resolve the best-effort image URL synchronously (first candidate or placeholder).
pub fn primary_image(station: &Station) -> String {
    image_candidates(station)
        .into_iter()
        .next()
        .unwrap_or_else(|| PLACEHOLDER_SVG.to_string())
}

/// Stream-health indicator, same scoring formula as the previous frontend.
/// Returns `(bar_height_pct, hue, lightness, tooltip)`.
pub fn status_score(station: &Station) -> (u8, u16, u16, String) {
    let bitrate_value = station.bitrate.map(|b| b as f64);
    let bitrate_score = match bitrate_value {
        Some(b) if b > 0.0 => (b / 320.0).clamp(0.15, 1.0),
        _ => 0.5,
    };
    let health_score = if station.lastcheckok == Some(false) {
        0.2
    } else if station.state.as_deref() == Some("offline") {
        0.15
    } else if station.ssl_error == Some(true) {
        0.3
    } else {
        0.82
    };
    let https = station.has_https == Some(true)
        || station
            .url
            .as_deref()
            .is_some_and(|u| u.starts_with("https"))
        || station
            .url_resolved
            .as_deref()
            .is_some_and(|u| u.starts_with("https"));
    let https_score = if https { 0.85 } else { 0.45 };
    let metadata_rich = station.favicon.is_some()
        || station.homepage.is_some()
        || station.tags.is_some()
        || station.genre.is_some()
        || station.country.is_some();
    let metadata_score = if metadata_rich { 0.8 } else { 0.45 };
    let popularity_score = match station.clickcount {
        Some(c) => ((c as f64) / 10000.0).clamp(0.2, 1.0),
        None => 0.55,
    };

    let score = bitrate_score * 0.35
        + health_score * 0.25
        + https_score * 0.15
        + metadata_score * 0.15
        + popularity_score * 0.1;
    let safety = score.clamp(0.12, 1.0);
    let height = (safety * 100.0).round() as u8;
    let (hue, lightness) = if safety < 0.35 {
        (0, 58)
    } else if safety < 0.7 {
        (35, 52)
    } else {
        (120, 50)
    };
    let label = if safety < 0.35 {
        "Poor"
    } else if safety < 0.7 {
        "Moderate"
    } else {
        "Stable"
    };

    let mut reasons = Vec::new();
    match bitrate_value {
        Some(b) => reasons.push(format!("bitrate {b} kbps")),
        None => reasons.push("bitrate unknown".to_string()),
    }
    reasons.push(
        if station.lastcheckok == Some(false) {
            "stream check failed"
        } else if station.state.as_deref() == Some("offline") {
            "offline state"
        } else {
            "recent check passed"
        }
        .to_string(),
    );
    reasons.push(
        if https {
            "HTTPS available"
        } else {
            "HTTPS not confirmed"
        }
        .to_string(),
    );
    reasons.push(
        if metadata_score > 0.7 {
            "station metadata is rich"
        } else {
            "metadata is sparse"
        }
        .to_string(),
    );
    reasons.push(
        if popularity_score > 0.6 {
            "popular stream"
        } else {
            "low popularity signal"
        }
        .to_string(),
    );
    let tooltip = format!("{label} signal — {}.", reasons.join(", "));

    (height, hue, lightness, tooltip)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncates_long_names() {
        assert_eq!(truncate_name("abcdefghij", 5), "abcde...");
        assert_eq!(truncate_name("abc", 5), "abc");
        assert_eq!(truncate_name("", 5), "Unknown station");
    }

    #[test]
    fn normalizes_views() {
        assert_eq!(normalize_view_name("Popular"), "popular");
        assert_eq!(normalize_view_name("nope"), "all");
    }

    #[test]
    fn formats_bitrate() {
        assert_eq!(format_bitrate(Some(128)), "128 kbps");
        assert_eq!(format_bitrate(None), "Stream");
        assert_eq!(format_bitrate(Some(0)), "Stream");
    }
}
