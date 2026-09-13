//! Curated genre catalog for the Genres view.
//!
//! The upstream tag list is raw per-station noise (typos, near-duplicates,
//! one-off tags like `cdmx`), which makes browsing hard. Instead the UI
//! offers this fixed catalog of canonical genres: each maps to the set of
//! raw tags (synonyms included, matched case-insensitively) that belong to
//! it. Anything outside the catalog falls into the [`VARIETY_GENRE`] bucket.

/// Catch-all bucket for stations whose tags match no canonical genre.
pub const VARIETY_GENRE: &str = "Variety";

/// A canonical genre and the raw upstream tags that belong to it. The first
/// tag is the primary one used for display ordering of merged results.
pub struct CanonicalGenre {
    pub name: &'static str,
    pub tags: &'static [&'static str],
}

/// Curated genre catalog. Synonym sets must stay disjoint (checked by test).
pub static GENRES: &[CanonicalGenre] = &[
    // Eras & decades
    CanonicalGenre {
        name: "70s",
        tags: &["70s", "70's", "seventies", "70er"],
    },
    CanonicalGenre {
        name: "80s",
        tags: &["80s", "80's", "eighties", "80er"],
    },
    CanonicalGenre {
        name: "90s",
        tags: &["90s", "90's", "nineties", "90er"],
    },
    CanonicalGenre {
        name: "2000s",
        tags: &["2000s", "2000's", "00s", "2000er"],
    },
    CanonicalGenre {
        name: "2010s",
        tags: &["2010s", "2010's"],
    },
    CanonicalGenre {
        name: "2020s",
        tags: &["2020s", "2020's"],
    },
    CanonicalGenre {
        name: "Oldies",
        tags: &[
            "oldies",
            "golden oldies",
            "oldie",
            "evergreen",
            "evergreens",
            "gold",
            "50s",
            "60s",
            "fifties",
            "sixties",
        ],
    },
    // Guitar & band music
    CanonicalGenre {
        name: "Pop",
        tags: &[
            "pop",
            "pop music",
            "top 40",
            "top40",
            "charts",
            "hits",
            "mainstream",
            "synthpop",
            "synth-pop",
        ],
    },
    CanonicalGenre {
        name: "Rock",
        tags: &[
            "rock",
            "rock music",
            "rock'n'roll",
            "rock and roll",
            "hard rock",
            "britpop",
            "grunge",
            "post rock",
            "post-rock",
            "stoner rock",
            "spanish rock",
            "rock en español",
            "rock en espanol",
            "deutschrock",
            "german rock",
            "new wave",
        ],
    },
    CanonicalGenre {
        name: "Classic Rock",
        tags: &["classic rock"],
    },
    CanonicalGenre {
        name: "Alternative",
        tags: &[
            "alternative",
            "alternative rock",
            "alt rock",
            "alt-rock",
            "indie",
            "indie rock",
            "post-punk",
            "post punk",
            "shoegaze",
            "dream pop",
        ],
    },
    CanonicalGenre {
        name: "Metal",
        tags: &[
            "metal",
            "heavy metal",
            "death metal",
            "black metal",
            "thrash metal",
            "doom metal",
            "power metal",
            "nu metal",
            "symphonic metal",
        ],
    },
    CanonicalGenre {
        name: "Punk",
        tags: &["punk", "punk rock"],
    },
    // Jazz, blues & classical
    CanonicalGenre {
        name: "Jazz",
        tags: &[
            "jazz",
            "smooth jazz",
            "acid jazz",
            "contemporary jazz",
            "latin jazz",
            "bebop",
            "bossa nova",
        ],
    },
    CanonicalGenre {
        name: "Blues",
        tags: &["blues", "blues rock"],
    },
    CanonicalGenre {
        name: "Classical",
        tags: &["classical", "classical music", "opera", "baroque"],
    },
    CanonicalGenre {
        name: "Country",
        tags: &["country", "country music", "bluegrass"],
    },
    CanonicalGenre {
        name: "Folk",
        tags: &[
            "folk",
            "folk music",
            "americana",
            "singer-songwriter",
            "singer songwriter",
            "acoustic",
        ],
    },
    // Urban & soul
    CanonicalGenre {
        name: "Hip-Hop",
        tags: &["hip hop", "hip-hop", "hiphop", "rap", "trap"],
    },
    CanonicalGenre {
        name: "R&B / Soul",
        tags: &[
            "rnb",
            "r&b",
            "rhythm and blues",
            "soul",
            "neo soul",
            "neo-soul",
            "motown",
        ],
    },
    CanonicalGenre {
        name: "Funk",
        tags: &["funk", "funky"],
    },
    // Electronic & dance
    CanonicalGenre {
        name: "Electronic",
        tags: &[
            "electronic",
            "electronica",
            "electro",
            "idm",
            "ebm",
            "industrial",
        ],
    },
    CanonicalGenre {
        name: "Dance",
        tags: &[
            "dance",
            "edm",
            "club",
            "dance music",
            "hands up",
            "hands-up",
            "eurodance",
            "italo dance",
        ],
    },
    CanonicalGenre {
        name: "House",
        tags: &[
            "house",
            "deep house",
            "tech house",
            "progressive house",
            "acid house",
            "electro house",
            "big room",
            "tropical house",
        ],
    },
    CanonicalGenre {
        name: "Techno",
        tags: &[
            "techno",
            "minimal",
            "minimal techno",
            "hard techno",
            "hardcore",
            "happy hardcore",
            "gabber",
            "schranz",
            "detroit techno",
            "melodic techno",
        ],
    },
    CanonicalGenre {
        name: "Trance",
        tags: &[
            "trance",
            "goa",
            "goa trance",
            "psytrance",
            "psy-trance",
            "uplifting trance",
            "vocal trance",
            "hard trance",
        ],
    },
    CanonicalGenre {
        name: "Disco",
        tags: &[
            "disco",
            "italo disco",
            "italo-disco",
            "eurodisco",
            "nu-disco",
            "nudisco",
        ],
    },
    CanonicalGenre {
        name: "Drum and Bass",
        tags: &["drum and bass", "drum'n'bass", "dnb", "jungle"],
    },
    CanonicalGenre {
        name: "Dubstep",
        tags: &["dubstep"],
    },
    // Caribbean & latin
    CanonicalGenre {
        name: "Reggae",
        tags: &["reggae", "dancehall", "dub", "ska", "soca"],
    },
    CanonicalGenre {
        name: "Latin",
        tags: &[
            "latin",
            "latin music",
            "reggaeton",
            "salsa",
            "bachata",
            "merengue",
            "cumbia",
            "latin pop",
            "tango",
            "samba",
            "mpb",
            "flamenco",
        ],
    },
    // Regional pop
    CanonicalGenre {
        name: "K-Pop",
        tags: &["k-pop", "kpop"],
    },
    CanonicalGenre {
        name: "J-Pop",
        tags: &["j-pop", "jpop", "anime", "anison"],
    },
    CanonicalGenre {
        name: "World",
        tags: &[
            "world",
            "world music",
            "african",
            "arabic",
            "bollywood",
            "indian",
            "celtic",
            "chanson",
            "afrobeats",
            "afrobeat",
        ],
    },
    // Downtempo
    CanonicalGenre {
        name: "Ambient",
        tags: &[
            "ambient",
            "dark ambient",
            "drone",
            "space music",
            "new age",
            "meditation",
        ],
    },
    CanonicalGenre {
        name: "Chillout",
        tags: &[
            "chillout",
            "chill out",
            "chill",
            "lounge",
            "downtempo",
            "lofi",
            "lo-fi",
            "trip hop",
            "trip-hop",
        ],
    },
    // Spoken word & formats
    CanonicalGenre {
        name: "News",
        tags: &["news", "news talk", "newstalk"],
    },
    CanonicalGenre {
        name: "Talk",
        tags: &[
            "talk",
            "talk radio",
            "talkshow",
            "talk show",
            "podcast",
            "podcasts",
        ],
    },
    CanonicalGenre {
        name: "Sports",
        tags: &["sports", "sport", "football", "soccer"],
    },
    CanonicalGenre {
        name: "Comedy",
        tags: &["comedy", "humour", "humor"],
    },
    CanonicalGenre {
        name: "Adult Contemporary",
        tags: &["adult contemporary", "soft rock"],
    },
    CanonicalGenre {
        name: "Easy Listening",
        tags: &[
            "easy listening",
            "elevator music",
            "beautiful music",
            "instrumental",
        ],
    },
    CanonicalGenre {
        name: "Religious",
        tags: &[
            "religious",
            "christian",
            "gospel",
            "catholic",
            "islamic",
            "quran",
            "worship",
        ],
    },
    CanonicalGenre {
        name: "Kids",
        tags: &["kids", "children", "children's"],
    },
    CanonicalGenre {
        name: "Community",
        tags: &[
            "community",
            "community radio",
            "college",
            "college radio",
            "campus",
        ],
    },
    CanonicalGenre {
        name: "Schlager",
        tags: &["schlager", "volksmusik"],
    },
    CanonicalGenre {
        name: "Swing",
        tags: &["swing", "big band", "electro swing", "electroswing"],
    },
    CanonicalGenre {
        name: "Soundtracks",
        tags: &[
            "soundtrack",
            "soundtracks",
            "film music",
            "game music",
            "video game music",
        ],
    },
];

/// Normalize a raw tag for catalog matching.
pub fn normalize_tag(tag: &str) -> String {
    tag.trim().to_lowercase()
}

/// Canonical genre name for a raw tag, if it belongs to the catalog.
pub fn classify(tag: &str) -> Option<&'static str> {
    let normalized = normalize_tag(tag);
    GENRES
        .iter()
        .find(|genre| genre.tags.iter().any(|t| *t == normalized))
        .map(|genre| genre.name)
}

/// Canonical genre by display name (case-insensitive).
pub fn find_genre(name: &str) -> Option<&'static CanonicalGenre> {
    let normalized = normalize_tag(name);
    GENRES
        .iter()
        .find(|genre| genre.name.to_lowercase() == normalized)
}

/// True when at least one of a station's comma-separated tags belongs to the
/// catalog. Stations with no (or only unknown) tags land in Variety.
pub fn has_canonical_tag(tags: &Option<String>) -> bool {
    tags.as_deref()
        .map(|list| list.split(',').any(|tag| classify(tag).is_some()))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn synonym_sets_are_disjoint() {
        let mut seen = HashSet::new();
        for genre in GENRES {
            assert!(!genre.tags.is_empty(), "{} has no tags", genre.name);
            for tag in genre.tags {
                assert_eq!(
                    *tag,
                    tag.trim().to_lowercase(),
                    "synonym not normalized: {tag}"
                );
                assert!(seen.insert(*tag), "raw tag '{tag}' claimed by two genres");
            }
        }
    }

    #[test]
    fn variety_is_not_a_canonical_genre() {
        assert!(find_genre(VARIETY_GENRE).is_none());
        assert!(classify("variety").is_none());
    }

    #[test]
    fn classification_spot_checks() {
        assert_eq!(classify("  Jazz "), Some("Jazz"));
        assert_eq!(classify("HIPHOP"), Some("Hip-Hop"));
        assert_eq!(classify("Top 40"), Some("Pop"));
        assert_eq!(classify("dnb"), Some("Drum and Bass"));
        assert_eq!(classify("schranz"), Some("Techno"));
        assert_eq!(classify("cdmx"), None);
        assert_eq!(classify(""), None);
        assert_eq!(classify("mix"), None);
        // Near-misses must not cross genres.
        assert_eq!(classify("acid house"), Some("House"));
        assert_eq!(classify("acid jazz"), Some("Jazz"));
        assert_eq!(classify("dub"), Some("Reggae"));
        assert_eq!(classify("dubstep"), Some("Dubstep"));
    }

    #[test]
    fn variety_partitioning() {
        assert!(has_canonical_tag(&Some("pop, charts".to_string())));
        assert!(has_canonical_tag(&Some("  SMOOTH JAZZ ".to_string())));
        assert!(!has_canonical_tag(&Some("cdmx, mix".to_string())));
        assert!(!has_canonical_tag(&None));
        assert!(!has_canonical_tag(&Some(String::new())));
    }

    #[test]
    fn catalog_has_substance() {
        assert!(GENRES.len() >= 40, "catalog shrank to {}", GENRES.len());
    }
}
