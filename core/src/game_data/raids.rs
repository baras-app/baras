//! Operation/Raid area IDs
//!
//! Maps area IDs to operation names for lazy-loading encounter definitions.
//! Extracted from Orbs SWTOR Combat Parser data.

/// Operation area info
pub struct OperationInfo {
    pub name: &'static str,
    pub log_name: &'static str,
    /// 1 = Operation, 2 = World Boss/Lair
    pub encounter_type: u8,
}

/// Map of area_id -> operation info
pub static OPERATION_AREAS: &[(i64, OperationInfo)] = &[
    // Operations
    (
        833571547775670,
        OperationInfo {
            name: "Eternity Vault",
            log_name: "Eternity Vault",
            encounter_type: 1,
        },
    ),
    (
        833571547775669,
        OperationInfo {
            name: "Karagga's Palace",
            log_name: "Karagga's Palace",
            encounter_type: 1,
        },
    ),
    (
        833571547775688,
        OperationInfo {
            name: "Explosive Conflict",
            log_name: "Denova",
            encounter_type: 1,
        },
    ),
    (
        137438992720,
        OperationInfo {
            name: "Terror From Beyond",
            log_name: "Asation",
            encounter_type: 1,
        },
    ),
    (
        137438993037,
        OperationInfo {
            name: "Scum and Villainy",
            log_name: "Darvannis",
            encounter_type: 1,
        },
    ),
    (
        137438993402,
        OperationInfo {
            name: "The Dread Fortress",
            log_name: "The Dread Fortress",
            encounter_type: 1,
        },
    ),
    (
        137438993410,
        OperationInfo {
            name: "The Dread Palace",
            log_name: "The Dread Palace",
            encounter_type: 1,
        },
    ),
    (
        3367421863788544,
        OperationInfo {
            name: "The Ravagers",
            log_name: "The Ravagers",
            encounter_type: 1,
        },
    ),
    (
        3367426158755840,
        OperationInfo {
            name: "Temple of Sacrifice",
            log_name: "Temple of Sacrifice",
            encounter_type: 1,
        },
    ),
    (
        833571547775765,
        OperationInfo {
            name: "The Gods from the Machine",
            log_name: "Valley of the Machine Gods",
            encounter_type: 1,
        },
    ),
    (
        833571547775792,
        OperationInfo {
            name: "Dxun",
            log_name: "Dxun - The CI-004 Facility",
            encounter_type: 1,
        },
    ),
    (
        833571547775799,
        OperationInfo {
            name: "R4",
            log_name: "R-4 Anomaly",
            encounter_type: 1,
        },
    ),
    // World Bosses / Lairs
    (
        137438993300,
        OperationInfo {
            name: "Xeno",
            log_name: "Primary Observatory Four",
            encounter_type: 2,
        },
    ),
    (
        137438993438,
        OperationInfo {
            name: "Eyeless",
            log_name: "Lair of the Eyeless",
            encounter_type: 2,
        },
    ),
    (
        945872057669593,
        OperationInfo {
            name: "Queen's Hive",
            log_name: "Hive of the Mountain Queen",
            encounter_type: 2,
        },
    ),
    (
        3541801830973440,
        OperationInfo {
            name: "Colossal Monolith",
            log_name: "Heart of Ruin",
            encounter_type: 2,
        },
    ),
    (
        945872057669657,
        OperationInfo {
            name: "Propagator Core",
            log_name: "Emperor's Fortress Ruins",
            encounter_type: 2,
        },
    ),
    // Note: Toborro's Palace has LogId "0" in source data - may need special handling
];

/// Look up operation name by area ID
pub fn get_operation_name(area_id: i64) -> Option<&'static str> {
    OPERATION_AREAS
        .iter()
        .find(|(id, _)| *id == area_id)
        .map(|(_, info)| info.name)
}

/// Check if an area ID is an operation
pub fn is_operation(area_id: i64) -> bool {
    OPERATION_AREAS.iter().any(|(id, _)| *id == area_id)
}

/// Check if an area ID is a world boss/lair
pub fn is_world_boss(area_id: i64) -> bool {
    OPERATION_AREAS
        .iter()
        .find(|(id, _)| *id == area_id)
        .map(|(_, info)| info.encounter_type == 2)
        .unwrap_or(false)
}
