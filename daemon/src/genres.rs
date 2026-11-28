// Parent genres for this library. A search for a family name includes every
// specific genre in that family. The specific name on its own does not.

pub struct Family {
    pub id: &'static str,
    pub label: &'static str,
    pub members: &'static [&'static str],
    /// Words that mean the whole family. Member names are not included.
    pub keys: &'static [&'static str],
}

pub const FAMILIES: &[Family] = &[
    Family {
        id: "hip hop",
        label: "Hip-Hop",
        members: &[
            "hip hop",
            "gangsta rap",
            "christian hip hop",
            "pop rap",
            "dirty south",
            "hardcore hip hop",
            "conscious hip hop",
            "grime",
        ],
        keys: &["hip hop", "hiphop", "rap"],
    },
    Family {
        id: "metal",
        label: "Metal",
        members: &[
            "alternative metal",
            "heavy metal",
            "nu metal",
            "metal",
            "metalcore",
            "industrial metal",
            "thrash metal",
            "progressive metal",
            "groove metal",
            "melodic metalcore",
            "deathcore",
        ],
        keys: &["metal"],
    },
    Family {
        id: "rock",
        label: "Rock",
        members: &[
            "alternative rock",
            "rock",
            "hard rock",
            "indie rock",
            "blues rock",
            "pop rock",
            "post-grunge",
            "folk rock",
            "piano rock",
            "country rock",
            "grunge",
            "garage rock",
            "stoner rock",
            "psychedelic rock",
            "classic rock",
            "acoustic rock",
            "britpop",
            "art rock",
            "new wave",
        ],
        keys: &["rock"],
    },
    Family {
        id: "punk",
        label: "Punk",
        members: &["punk rock", "pop punk", "punk", "garage punk"],
        keys: &["punk"],
    },
    Family {
        id: "pop",
        label: "Pop",
        members: &[
            "pop",
            "dance-pop",
            "indie pop",
            "electropop",
            "synth-pop",
            "nederpop",
            "latin pop",
            "alternative pop",
            "folk pop",
            "country pop",
            "europop",
        ],
        keys: &["pop"],
    },
    Family {
        id: "electronic",
        label: "Electronic",
        members: &[
            "electronic",
            "dancefloor drum and bass",
            "disco",
            "dance",
            "big beat",
            "electro house",
            "electro",
            "dubstep",
            "house",
            "drum and bass",
            "deep house",
            "downtempo",
            "ambient",
            "chillout",
        ],
        keys: &["electronic", "edm"],
    },
    Family {
        id: "r&b",
        label: "R&B / Soul",
        members: &[
            "soul",
            "contemporary r&b",
            "r&b",
            "neo soul",
            "alternative r&b",
            "pop soul",
            "blue-eyed soul",
            "funk",
        ],
        keys: &["r&b", "rnb", "soul"],
    },
    Family {
        id: "blues",
        label: "Blues",
        members: &["blues"],
        keys: &["blues"],
    },
    Family {
        id: "country",
        label: "Country",
        members: &["country", "country pop"],
        keys: &["country"],
    },
    Family {
        id: "jazz",
        label: "Jazz",
        members: &["jazz", "bossa nova"],
        keys: &["jazz"],
    },
    Family {
        id: "reggae",
        label: "Reggae",
        members: &["reggae", "dancehall", "reggaeton"],
        keys: &["reggae"],
    },
];

pub fn normalize_key(value: &str) -> String {
    value
        .trim()
        .to_lowercase()
        .replace('-', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// The whole family when `term` is a family name such as "rap" or "hip hop".
/// A specific genre such as "gangsta rap" returns nothing.
pub fn expand_term(term: &str) -> Option<&'static [&'static str]> {
    let key = normalize_key(term);
    FAMILIES
        .iter()
        .find(|family| family.keys.iter().any(|candidate| normalize_key(candidate) == key))
        .map(|family| family.members)
}

pub fn family_members(id: &str) -> Option<&'static [&'static str]> {
    let key = normalize_key(id);
    FAMILIES
        .iter()
        .find(|family| normalize_key(family.id) == key || normalize_key(family.label) == key)
        .map(|family| family.members)
}
