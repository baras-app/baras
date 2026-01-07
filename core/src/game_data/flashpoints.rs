//! Flashpoint area IDs
//!
//! Maps area IDs to flashpoint names for lazy-loading encounter definitions.
//! Extracted from Orbs SWTOR Combat Parser data.

/// Flashpoint area info
pub struct FlashpointInfo {
    pub name: &'static str,
    pub log_name: &'static str,
}

/// Map of area_id -> flashpoint info
pub static FLASHPOINT_AREAS: &[(i64, FlashpointInfo)] = &[
    (
        833571547775679,
        FlashpointInfo {
            name: "The Black Talon",
            log_name: "The Black Talon",
        },
    ),
    (
        833571547775678,
        FlashpointInfo {
            name: "The Esseles",
            log_name: "The Esseles",
        },
    ),
    (
        833571547775680,
        FlashpointInfo {
            name: "Boarding Party",
            log_name: "Boarding Party",
        },
    ),
    (
        833571547775682,
        FlashpointInfo {
            name: "Hammer Station",
            log_name: "Hammer Station",
        },
    ),
    (
        833571547775683,
        FlashpointInfo {
            name: "Athiss",
            log_name: "Athiss",
        },
    ),
    (
        833571547775687,
        FlashpointInfo {
            name: "Mandalorian Raiders",
            log_name: "Mandalorian Raiders",
        },
    ),
    (
        833571547775684,
        FlashpointInfo {
            name: "Cademimu",
            log_name: "Cademimu",
        },
    ),
    (
        833571547775686,
        FlashpointInfo {
            name: "The Red Reaper",
            log_name: "The Red Reaper",
        },
    ),
    (
        833571547775681,
        FlashpointInfo {
            name: "The Foundry",
            log_name: "The Foundry",
        },
    ),
    (
        833571547775677,
        FlashpointInfo {
            name: "Taral V",
            log_name: "Taral V",
        },
    ),
    (
        833571547775671,
        FlashpointInfo {
            name: "Directive 7",
            log_name: "Directive 7",
        },
    ),
    (
        833571547775672,
        FlashpointInfo {
            name: "The Battle of Ilum",
            log_name: "The Battle of Ilum",
        },
    ),
    (
        833571547775673,
        FlashpointInfo {
            name: "The False Emperor",
            log_name: "The False Emperor",
        },
    ),
    (
        833571547775674,
        FlashpointInfo {
            name: "Kaon Under Siege",
            log_name: "Kaon Under Siege",
        },
    ),
    (
        833571547775675,
        FlashpointInfo {
            name: "Lost Island",
            log_name: "Lost Island",
        },
    ),
    (
        137438993332,
        FlashpointInfo {
            name: "Czerka Corporate Labs",
            log_name: "Czerka Corporate Labs",
        },
    ),
    (
        137438993342,
        FlashpointInfo {
            name: "Czerka Core Meltdown",
            log_name: "Czerka Research Biomes",
        },
    ),
    (
        833571547775709,
        FlashpointInfo {
            name: "Assault on Tython",
            log_name: "Assault on Tython",
        },
    ),
    (
        833571547775710,
        FlashpointInfo {
            name: "Korriban Incursion",
            log_name: "Korriban Incursion",
        },
    ),
    (
        833571547775707,
        FlashpointInfo {
            name: "Depths of Manaan",
            log_name: "Manaan Research Facility",
        },
    ),
    (
        137438993543,
        FlashpointInfo {
            name: "Legacy of the Rakata",
            log_name: "Rakata Prime",
        },
    ),
    (
        833571547775719,
        FlashpointInfo {
            name: "Blood Hunt",
            log_name: "Blood Hunt",
        },
    ),
    (
        833571547775720,
        FlashpointInfo {
            name: "Battle of Rishi",
            log_name: "Battle of Rishi",
        },
    ),
    (
        833571547775775,
        FlashpointInfo {
            name: "Crisis on Umbara",
            log_name: "Crisis on Umbara",
        },
    ),
    (
        833571547775777,
        FlashpointInfo {
            name: "A Traitor Among The Chiss",
            log_name: "A Traitor Among The Chiss",
        },
    ),
    (
        833571547775764,
        FlashpointInfo {
            name: "The Nathema Conspiracy",
            log_name: "Nathema",
        },
    ),
    (
        833571547775793,
        FlashpointInfo {
            name: "Objective Meridian",
            log_name: "Objective Meridian",
        },
    ),
    (
        833571547775795,
        FlashpointInfo {
            name: "Spirit of Vengeance",
            log_name: "Spirit of Vengeance",
        },
    ),
    (
        833571547775786,
        FlashpointInfo {
            name: "Secrets of the Enclave",
            log_name: "Dantooine",
        },
    ),
    (
        833571547775798,
        FlashpointInfo {
            name: "Ruins of Nul",
            log_name: "Ruins of Nul",
        },
    ),
    (
        833571547775676,
        FlashpointInfo {
            name: "Maelstrom Prison",
            log_name: "Maelstrom Prison",
        },
    ),
    (
        137438987149,
        FlashpointInfo {
            name: "Shrine of Silence",
            log_name: "Voss",
        },
    ),
];

/// Look up flashpoint name by area ID
pub fn get_flashpoint_name(area_id: i64) -> Option<&'static str> {
    FLASHPOINT_AREAS
        .iter()
        .find(|(id, _)| *id == area_id)
        .map(|(_, info)| info.name)
}

/// Check if an area ID is a flashpoint
pub fn is_flashpoint(area_id: i64) -> bool {
    FLASHPOINT_AREAS.iter().any(|(id, _)| *id == area_id)
}
