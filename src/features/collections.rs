// Collections feature handlers (countries, languages, tags)
use crate::domain::{Country, Language, Tag};
use crate::error::Result;
use crate::infra::RadioBrowserClient;
use axum::Json;
use std::collections::HashMap;
use tracing::debug;

#[derive(Clone)]
pub struct CollectionsHandlers {
    client: RadioBrowserClient,
}

impl CollectionsHandlers {
    pub fn new(client: RadioBrowserClient) -> Self {
        Self { client }
    }

    /// GET /api/countries - Get all countries
    pub async fn countries(&self) -> Result<Json<Vec<Country>>> {
        debug!("Fetching countries");
        let limit = super::stations::default_station_limit();
        let countries = self.client.get_countries().await?;
        Ok(Json(
            countries
                .into_iter()
                .map(|mut country| {
                    country.stationcount = country.stationcount.map(|n| n.min(limit));
                    country
                })
                .collect(),
        ))
    }

    /// GET /api/languages - Get all languages
    pub async fn languages(&self) -> Result<Json<Vec<Language>>> {
        debug!("Fetching languages");
        let limit = super::stations::default_station_limit();
        let languages = self.client.get_languages().await?;
        Ok(Json(
            languages
                .into_iter()
                .map(|mut language| {
                    language.stationcount = language.stationcount.map(|n| n.min(limit));
                    language
                })
                .collect(),
        ))
    }

    /// GET /api/tags - Get all tags/genres
    pub async fn tags(&self) -> Result<Json<Vec<Tag>>> {
        debug!("Fetching tags");
        let tags = self.client.get_tags().await?;
        Ok(Json(tags))
    }

    /// GET /api/genres - Get the curated genre catalog with aggregated
    /// station counts. Raw tags matching a canonical genre contribute their
    /// count to it; everything else is summed into the Variety bucket.
    /// Displayed counts are clamped to what a click actually fetches, so the
    /// UI never promises stations it cannot show.
    pub async fn genres(&self) -> Result<Json<Vec<Tag>>> {
        debug!("Fetching curated genres");
        let tags = self.client.get_tags().await?;
        let limit = super::stations::default_station_limit();
        Ok(Json(aggregate_genre_counts(&tags, limit)))
    }
}

/// Fold raw upstream tags into the curated catalog. Pure function so the
/// bucketing/counting is unit-testable without network access.
fn aggregate_genre_counts(tags: &[Tag], display_limit: u32) -> Vec<Tag> {
    let mut counts: HashMap<String, u32> = HashMap::new();
    let mut variety_count: u32 = 0;
    for tag in tags {
        let count = tag.stationcount.unwrap_or(0);
        match crate::genres::classify(&tag.name) {
            Some(genre) => {
                let entry = counts.entry(genre.to_string()).or_insert(0);
                *entry = entry.saturating_add(count);
            }
            None => variety_count = variety_count.saturating_add(count),
        }
    }

    let mut genres: Vec<Tag> = crate::genres::GENRES
        .iter()
        .map(|genre| Tag {
            name: genre.name.to_string(),
            stationcount: Some(
                counts
                    .get(genre.name)
                    .copied()
                    .unwrap_or(0)
                    .min(display_limit),
            ),
        })
        .collect();
    genres.push(Tag {
        name: crate::genres::VARIETY_GENRE.to_string(),
        stationcount: Some(variety_count.min(display_limit)),
    });
    genres
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tag(name: &str, count: u32) -> Tag {
        Tag {
            name: name.to_string(),
            stationcount: Some(count),
        }
    }

    #[test]
    fn counts_aggregate_into_canonical_genres_and_variety() {
        let tags = vec![
            tag("kids", 100),
            tag("Children", 50), // case-insensitive match
            tag("cdmx", 7),      // unknown -> Variety
            tag("rock", 300),
            tag("Rock'n'Roll", 20),
        ];
        let genres = aggregate_genre_counts(&tags, 10_000);
        // Catalog order + Variety last.
        assert_eq!(genres.len(), crate::genres::GENRES.len() + 1);
        assert_eq!(genres.last().unwrap().name, crate::genres::VARIETY_GENRE);
        let count_of = |name: &str| {
            genres
                .iter()
                .find(|g| g.name == name)
                .unwrap()
                .stationcount
                .unwrap()
        };
        assert_eq!(count_of("Kids"), 150);
        assert_eq!(count_of("Rock"), 320);
        assert_eq!(count_of("Jazz"), 0);
        assert_eq!(count_of(crate::genres::VARIETY_GENRE), 7);
    }

    #[test]
    fn displayed_counts_never_exceed_what_a_click_fetches() {
        let tags = vec![tag("pop", 50_000), tag("obscure", 5)];
        let genres = aggregate_genre_counts(&tags, 1000);
        let count_of = |name: &str| {
            genres
                .iter()
                .find(|g| g.name == name)
                .unwrap()
                .stationcount
                .unwrap()
        };
        assert_eq!(count_of("Pop"), 1000);
        assert_eq!(count_of(crate::genres::VARIETY_GENRE), 5);
    }
}
