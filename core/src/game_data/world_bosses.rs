//! World boss data
//!
//! Open world bosses found on various planets and during events.

use super::bosses::{BossInfo, ContentType};

pub static WORLD_BOSS_DATA: &[(i64, BossInfo)] = &[
    // ─────────────────────────────────────────────────────────────────────────
    // Planet World Bosses
    // ─────────────────────────────────────────────────────────────────────────
    (
        1476433662705664,
        BossInfo {
            content_type: ContentType::OpenWorld,
            operation: "Coruscant",
            boss: "SD-0",
            difficulty: None,
            is_kill_target: true,
        },
    ),
    (
        1467216662888448,
        BossInfo {
            content_type: ContentType::OpenWorld,
            operation: "Dromund Kaas",
            boss: "The First",
            difficulty: None,
            is_kill_target: true,
        },
    ),
    (
        1514096230924288,
        BossInfo {
            content_type: ContentType::OpenWorld,
            operation: "Taris",
            boss: "Subject Alpha",
            difficulty: None,
            is_kill_target: true,
        },
    ),
    (
        1486561195589632,
        BossInfo {
            content_type: ContentType::OpenWorld,
            operation: "Balmorra",
            boss: "Grandfather",
            difficulty: None,
            is_kill_target: true,
        },
    ),
    (
        1512537157795840,
        BossInfo {
            content_type: ContentType::OpenWorld,
            operation: "Nar Shaddaa",
            boss: "R4-GL",
            difficulty: None,
            is_kill_target: true,
        },
    ),
    (
        1491212645171200,
        BossInfo {
            content_type: ContentType::OpenWorld,
            operation: "Tatooine",
            boss: "Trapjaw",
            difficulty: None,
            is_kill_target: true,
        },
    ),
    (
        1506708887175168,
        BossInfo {
            content_type: ContentType::OpenWorld,
            operation: "Alderaan",
            boss: "Siegebreaker",
            difficulty: None,
            is_kill_target: true,
        },
    ),
    (
        2455746335735808,
        BossInfo {
            content_type: ContentType::OpenWorld,
            operation: "Taris",
            boss: "Ancient One",
            difficulty: None,
            is_kill_target: true,
        },
    ),
    (
        1781380635688960,
        BossInfo {
            content_type: ContentType::OpenWorld,
            operation: "Quesh",
            boss: "Cartel Warbot",
            difficulty: None,
            is_kill_target: true,
        },
    ),
    (
        1514109115826176,
        BossInfo {
            content_type: ContentType::OpenWorld,
            operation: "Hoth",
            boss: "Gargath",
            difficulty: None,
            is_kill_target: true,
        },
    ),
    (
        1780723505692672,
        BossInfo {
            content_type: ContentType::OpenWorld,
            operation: "Hoth",
            boss: "Snowblind",
            difficulty: None,
            is_kill_target: true,
        },
    ),
    (
        2276465810866176,
        BossInfo {
            content_type: ContentType::OpenWorld,
            operation: "Belsavis",
            boss: "Primal Destroyer",
            difficulty: None,
            is_kill_target: true,
        },
    ),
    (
        3207528821293056,
        BossInfo {
            content_type: ContentType::OpenWorld,
            operation: "Corellia",
            boss: "Lucky",
            difficulty: None,
            is_kill_target: true,
        },
    ),
    (
        2821282412363776,
        BossInfo {
            content_type: ContentType::OpenWorld,
            operation: "Voss",
            boss: "Nightmare Pilgrim",
            difficulty: None,
            is_kill_target: true,
        },
    ),
    (
        2963995585675264,
        BossInfo {
            content_type: ContentType::OpenWorld,
            operation: "Section X",
            boss: "Dreadtooth",
            difficulty: None,
            is_kill_target: true,
        },
    ),
    (
        3427461211619328,
        BossInfo {
            content_type: ContentType::OpenWorld,
            operation: "Yavin 4",
            boss: "Lance Command Unit",
            difficulty: None,
            is_kill_target: true,
        },
    ),
    (
        3511870203887616,
        BossInfo {
            content_type: ContentType::OpenWorld,
            operation: "Yavin 4",
            boss: "Ancient Threat",
            difficulty: None,
            is_kill_target: true,
        },
    ),
    (
        4208651338252288,
        BossInfo {
            content_type: ContentType::OpenWorld,
            operation: "Ossus",
            boss: "Kil'Cik",
            difficulty: None,
            is_kill_target: true,
        },
    ),
    (
        4208664223154176,
        BossInfo {
            content_type: ContentType::OpenWorld,
            operation: "Ossus",
            boss: "R8-X8",
            difficulty: None,
            is_kill_target: true,
        },
    ),
    (
        4351227072610304,
        BossInfo {
            content_type: ContentType::OpenWorld,
            operation: "Mek-Sha",
            boss: "Karvoy",
            difficulty: None,
            is_kill_target: true,
        },
    ),
    (
        4641141660057600,
        BossInfo {
            content_type: ContentType::OpenWorld,
            operation: "Ruhnuk",
            boss: "Kithrawl",
            difficulty: None,
            is_kill_target: true,
        },
    ),
    // ─────────────────────────────────────────────────────────────────────────
    // Gree Event
    // ─────────────────────────────────────────────────────────────────────────
    (
        3213232537862144,
        BossInfo {
            content_type: ContentType::OpenWorld,
            operation: "Gree Event",
            boss: "Surgok'k",
            difficulty: None,
            is_kill_target: true,
        },
    ),
    (
        3211450126434304,
        BossInfo {
            content_type: ContentType::OpenWorld,
            operation: "Gree Event",
            boss: "Gravak'k",
            difficulty: None,
            is_kill_target: true,
        },
    ),
    // ─────────────────────────────────────────────────────────────────────────
    // Rakghoul Event
    // ─────────────────────────────────────────────────────────────────────────
    (
        3339998497603584,
        BossInfo {
            content_type: ContentType::OpenWorld,
            operation: "Rakghoul Event",
            boss: "Shellshock",
            difficulty: None,
            is_kill_target: true,
        },
    ),
    (
        3340011382505472,
        BossInfo {
            content_type: ContentType::OpenWorld,
            operation: "Rakghoul Event",
            boss: "Toxxun",
            difficulty: None,
            is_kill_target: true,
        },
    ),
    (
        3339968432832512,
        BossInfo {
            content_type: ContentType::OpenWorld,
            operation: "Rakghoul Event",
            boss: "Plaguehorn",
            difficulty: None,
            is_kill_target: true,
        },
    ),
    // ─────────────────────────────────────────────────────────────────────────
    // Dark vs Light Event
    // ─────────────────────────────────────────────────────────────────────────
    (
        4046713891323904,
        BossInfo {
            content_type: ContentType::OpenWorld,
            operation: "Dark vs Light",
            boss: "Night Stalker Raxine",
            difficulty: None,
            is_kill_target: true,
        },
    ),
    (
        4048023856349184,
        BossInfo {
            content_type: ContentType::OpenWorld,
            operation: "Dark vs Light",
            boss: "Keeper Anais",
            difficulty: None,
            is_kill_target: true,
        },
    ),
    (
        4048268669485056,
        BossInfo {
            content_type: ContentType::OpenWorld,
            operation: "Dark vs Light",
            boss: "Erdi the Relentless",
            difficulty: None,
            is_kill_target: true,
        },
    ),
    (
        4046718186291200,
        BossInfo {
            content_type: ContentType::OpenWorld,
            operation: "Dark vs Light",
            boss: "Gorso the Nightmare King",
            difficulty: None,
            is_kill_target: true,
        },
    ),
    (
        4048187065106432,
        BossInfo {
            content_type: ContentType::OpenWorld,
            operation: "Dark vs Light",
            boss: "Justice Orzmod",
            difficulty: None,
            is_kill_target: true,
        },
    ),
    (
        4048199950008320,
        BossInfo {
            content_type: ContentType::OpenWorld,
            operation: "Dark vs Light",
            boss: "Tulo the Fearless",
            difficulty: None,
            is_kill_target: true,
        },
    ),
    (
        4046344524136448,
        BossInfo {
            content_type: ContentType::OpenWorld,
            operation: "Dark vs Light",
            boss: "Cortella the Righteous",
            difficulty: None,
            is_kill_target: true,
        },
    ),
    (
        4038355884965888,
        BossInfo {
            content_type: ContentType::OpenWorld,
            operation: "Dark vs Light",
            boss: "Guardian Silaraz",
            difficulty: None,
            is_kill_target: true,
        },
    ),
    (
        4048251489615872,
        BossInfo {
            content_type: ContentType::OpenWorld,
            operation: "Dark vs Light",
            boss: "Preeda the Butcher",
            difficulty: None,
            is_kill_target: true,
        },
    ),
    (
        4046709596356608,
        BossInfo {
            content_type: ContentType::OpenWorld,
            operation: "Dark vs Light",
            boss: "Jaadel the Vindicator",
            difficulty: None,
            is_kill_target: true,
        },
    ),
    (
        4048212834910208,
        BossInfo {
            content_type: ContentType::OpenWorld,
            operation: "Dark vs Light",
            boss: "Thundering Bozwed",
            difficulty: None,
            is_kill_target: true,
        },
    ),
    (
        4037965042941952,
        BossInfo {
            content_type: ContentType::OpenWorld,
            operation: "Dark vs Light",
            boss: "Yezzil the Raging Storm",
            difficulty: None,
            is_kill_target: true,
        },
    ),
    (
        4048242899681280,
        BossInfo {
            content_type: ContentType::OpenWorld,
            operation: "Dark vs Light",
            boss: "Aloeek the Voracious",
            difficulty: None,
            is_kill_target: true,
        },
    ),
    (
        4048255784583168,
        BossInfo {
            content_type: ContentType::OpenWorld,
            operation: "Dark vs Light",
            boss: "Chanta the Unforgiving",
            difficulty: None,
            is_kill_target: true,
        },
    ),
    (
        4048247194648576,
        BossInfo {
            content_type: ContentType::OpenWorld,
            operation: "Dark vs Light",
            boss: "Overseer Qezzed",
            difficulty: None,
            is_kill_target: true,
        },
    ),
    (
        4048230014779392,
        BossInfo {
            content_type: ContentType::OpenWorld,
            operation: "Dark vs Light",
            boss: "Warden Nymessa",
            difficulty: None,
            is_kill_target: true,
        },
    ),
    (
        4038360179933184,
        BossInfo {
            content_type: ContentType::OpenWorld,
            operation: "Dark vs Light",
            boss: "Tormentor Urdig",
            difficulty: None,
            is_kill_target: true,
        },
    ),
    (
        4048221424844800,
        BossInfo {
            content_type: ContentType::OpenWorld,
            operation: "Dark vs Light",
            boss: "Malussa the Gleaming Star",
            difficulty: None,
            is_kill_target: true,
        },
    ),
    (
        4038020877516800,
        BossInfo {
            content_type: ContentType::OpenWorld,
            operation: "Dark vs Light",
            boss: "Defender Gilada",
            difficulty: None,
            is_kill_target: true,
        },
    ),
    // ─────────────────────────────────────────────────────────────────────────
    // Training Dummy
    // ─────────────────────────────────────────────────────────────────────────
    (
        2857785339412480,
        BossInfo {
            content_type: ContentType::TrainingDummy,
            operation: "Parsing",
            boss: "Training Dummy",
            difficulty: None,
            is_kill_target: false,
        },
    ),
];
