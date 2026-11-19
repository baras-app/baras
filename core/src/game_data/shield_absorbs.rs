//! Shield absorption coefficients for estimating shield values.
//!
//! Formula: `estimated_absorb = std_health_min * STD_HEALTH + healing_coefficient * BONUS_HEALING`
//!
//! Data sourced from Parsely.io shield definitions.

use phf::phf_map;

/// Standard health at level 80
const STD_HEALTH: f32 = 77045.0;
/// Estimated bonus healing for a typical geared level 80 character
const BONUS_HEALING: f32 = 6700.0;

/// Shield absorption coefficient data
#[derive(Debug, Clone, Copy)]
pub struct ShieldInfo {
    pub std_health_min: f32,
    pub healing_coefficient: f32,
    /// For percentage-based shields (like Saber Ward at 25%)
    pub amount_percent: f32,
}

impl ShieldInfo {
    const fn new(std_health_min: f32, healing_coefficient: f32, amount_percent: f32) -> Self {
        Self {
            std_health_min,
            healing_coefficient,
            amount_percent,
        }
    }

    /// Returns true if this shield has a finite absorb pool (can "break")
    /// Limited shields have stdHealthMin > 0 or healingCoefficient > 0
    pub fn is_limited(&self) -> bool {
        self.std_health_min > 0.0 || self.healing_coefficient > 0.0
    }

    /// Estimated max absorb for limited shields, None for unlimited (percentage-based)
    pub fn estimated_absorb(&self) -> Option<i64> {
        if self.is_limited() {
            Some(((self.std_health_min * STD_HEALTH) + (self.healing_coefficient * BONUS_HEALING)) as i64)
        } else {
            None
        }
    }
}

/// Get shield info for an effect ID
pub fn get_shield_info(effect_id: i64) -> Option<&'static ShieldInfo> {
    SHIELD_INFO.get(&effect_id)
}

/// Check if an effect ID is a known shield
pub fn is_known_shield(effect_id: i64) -> bool {
    SHIELD_INFO.contains_key(&effect_id)
}

/// Shield info lookup table indexed by effect ID
pub static SHIELD_INFO: phf::Map<i64, ShieldInfo> = phf_map! {
    // ═══════════════════════════════════════════════════════════════════════════
    // Agent/Operative Shields
    // ═══════════════════════════════════════════════════════════════════════════
    784716294782976i64 => ShieldInfo::new(0.164, 3.28, 1.0),   // Shield Probe
    962484991164416i64 => ShieldInfo::new(0.164, 3.28, 1.0),   // Defense Screen
    3394012006318080i64 => ShieldInfo::new(0.0, 0.0, 0.3),     // Ballistic Dampers (3 charges, unlimited)
    3404298452992000i64 => ShieldInfo::new(0.0, 0.0, 0.3),     // Ballistic Dampers (3 charges, unlimited)

    // Decoy (2 charges, unlimited - 100% absorb but charge-based)
    814192655335739i64 => ShieldInfo::new(0.0, 0.0, 1.0),      // Decoy
    3165584170680320i64 => ShieldInfo::new(0.0, 0.0, 1.0),     // Decoy
    3169672979546112i64 => ShieldInfo::new(0.0, 0.0, 1.0),     // Decoy
    801273393709344i64 => ShieldInfo::new(0.0, 0.0, 1.0),      // Decoy

    // Energy Redoubt
    3182695320387849i64 => ShieldInfo::new(0.029, 0.58, 1.0),  // Energy Redoubt
    3394840935006476i64 => ShieldInfo::new(0.029, 0.58, 1.0),  // Energy Redoubt
    3174264299585801i64 => ShieldInfo::new(0.029, 0.58, 1.0),  // Energy Redoubt
    3413631416926473i64 => ShieldInfo::new(0.029, 0.58, 1.0),  // Energy Redoubt

    // Shell Shield / Trauma Shield
    985226842997040i64 => ShieldInfo::new(0.013, 0.26, 1.0),   // Shell Shield
    4498402716942602i64 => ShieldInfo::new(0.013, 0.26, 1.0),  // Shell Shield
    999516199190834i64 => ShieldInfo::new(0.013, 0.26, 1.0),   // Trauma Shield
    4561689060048896i64 => ShieldInfo::new(0.013, 0.26, 1.0),  // Trauma Shield

    // ═══════════════════════════════════════════════════════════════════════════
    // Sorcerer/Sage Shields
    // ═══════════════════════════════════════════════════════════════════════════
    // Static Barrier
    3411286364782592i64 => ShieldInfo::new(0.134, 2.68, 1.0),  // Static Barrier
    3411286364782955i64 => ShieldInfo::new(0.134, 2.68, 1.0),  // Static Barrier
    3411286364782957i64 => ShieldInfo::new(0.134, 2.68, 1.0),  // Static Barrier
    3411286364782959i64 => ShieldInfo::new(0.134, 2.68, 1.0),  // Static Barrier

    // Force Armor
    812736661422080i64 => ShieldInfo::new(0.134, 2.68, 1.0),   // Force Armor
    812736661422603i64 => ShieldInfo::new(0.134, 2.68, 1.0),   // Mending Force Armor
    812736661422605i64 => ShieldInfo::new(0.134, 2.68, 1.0),   // Preserved Force Armor
    812736661422607i64 => ShieldInfo::new(0.134, 2.68, 1.0),   // Imbued Force Armor

    // Enduring Bastion (stacking)
    3120895035965733i64 => ShieldInfo::new(0.134, 2.68, 1.0),  // Enduring Bastion (1)
    3120895035965737i64 => ShieldInfo::new(0.268, 5.36, 1.0),  // Enduring Bastion (2)
    3120895035965741i64 => ShieldInfo::new(0.402, 8.04, 1.0),  // Enduring Bastion (3)
    3120895035965745i64 => ShieldInfo::new(0.536, 10.72, 1.0), // Enduring Bastion (4)
    3120899330933026i64 => ShieldInfo::new(0.134, 2.68, 1.0),  // Enduring Bastion (1)
    3120899330933056i64 => ShieldInfo::new(0.268, 5.36, 1.0),  // Enduring Bastion (2)
    3120899330933058i64 => ShieldInfo::new(0.402, 8.04, 1.0),  // Enduring Bastion (3)
    3120899330933060i64 => ShieldInfo::new(0.536, 10.72, 1.0), // Enduring Bastion (4)

    // ═══════════════════════════════════════════════════════════════════════════
    // Knight/Warrior Shields
    // ═══════════════════════════════════════════════════════════════════════════
    // Saber Ward (percentage-based, unlimited)
    812169725739008i64 => ShieldInfo::new(0.0, 0.0, 0.25),     // Saber Ward
    807793154064384i64 => ShieldInfo::new(0.0, 0.0, 0.25),     // Saber Ward

    // Blade Barrier / Sonic Barrier
    2308467612188672i64 => ShieldInfo::new(0.05, 1.0, 1.0),    // Blade Barrier
    2308471907155968i64 => ShieldInfo::new(0.05, 1.0, 1.0),    // Sonic Barrier

    // Guardianship / Sonic Wall
    3430016717160448i64 => ShieldInfo::new(0.05, 1.0, 1.0),    // Guardianship
    3426550678552576i64 => ShieldInfo::new(0.05, 1.0, 1.0),    // Sonic Wall

    // Zealous Defense / Seething Defense
    4470962170888192i64 => ShieldInfo::new(0.1, 1.0, 1.0),     // Zealous Defense
    4468780327501824i64 => ShieldInfo::new(0.1, 1.28, 1.0),    // Zealous Defense
    4499652552425472i64 => ShieldInfo::new(0.1, 1.0, 1.0),     // Seething Defense
    4490306703589376i64 => ShieldInfo::new(0.1, 1.28, 1.0),    // Seething Defense

    // ═══════════════════════════════════════════════════════════════════════════
    // Tech/Heroic Moment Shields
    // ═══════════════════════════════════════════════════════════════════════════
    // Mini Shield
    4505867370102784i64 => ShieldInfo::new(0.041, 0.82, 1.0),  // Mini Shield
    4505867370103064i64 => ShieldInfo::new(0.041, 0.82, 1.0),  // Mini Shield
    4307220837695488i64 => ShieldInfo::new(0.041, 0.82, 1.0),  // Mini Shield
    4307220837695785i64 => ShieldInfo::new(0.041, 0.82, 1.0),  // Mini Shield

    // Emergency Power / Supercommando (very large shields)
    4511704230658308i64 => ShieldInfo::new(21.033, 0.0, 1.0),  // Emergency Power
    4511704230658312i64 => ShieldInfo::new(21.033, 0.0, 1.0),  // Supercommando
    4511704230658314i64 => ShieldInfo::new(21.033, 0.0, 1.0),  // Supercommando
    4511704230658316i64 => ShieldInfo::new(21.033, 0.0, 1.0),  // Supercommando
    4364455571882266i64 => ShieldInfo::new(21.033, 0.0, 1.0),  // Emergency Power
    4364455571882268i64 => ShieldInfo::new(21.033, 0.0, 1.0),  // Emergency Power

    // Resilient Powerbase / Battle Meditation / Multibarrier
    4511717115559936i64 => ShieldInfo::new(0.268, 5.36, 1.0),  // Resilient Powerbase
    4511717115560216i64 => ShieldInfo::new(0.268, 5.36, 1.0),  // Battle Meditation
    4511717115560218i64 => ShieldInfo::new(0.268, 5.36, 1.0),  // Battle Meditation
    4511717115560220i64 => ShieldInfo::new(0.268, 5.36, 1.0),  // Battle Meditation
    4374866572607488i64 => ShieldInfo::new(0.268, 5.36, 1.0),  // Multibarrier
    4374866572607773i64 => ShieldInfo::new(0.268, 5.36, 1.0),  // Multibarrier

    // ═══════════════════════════════════════════════════════════════════════════
    // Consumables / Equipment Shields
    // ═══════════════════════════════════════════════════════════════════════════
    // Emergency Shield Generator (percentage-based, unlimited)
    870894813577216i64 => ShieldInfo::new(0.0, 0.0, 0.33),     // Emergency Shield Generator
    870869043773440i64 => ShieldInfo::new(0.0, 0.0, 1.0),      // Portable Deflector Shield
    4626328317853696i64 => ShieldInfo::new(0.0, 0.0, 1.0),     // Absorb Shield (generic)

    // Bek's Pre-Boom Precautions
    4394902595043328i64 => ShieldInfo::new(0.5, 0.0, 1.0),     // Bek's Pre-Boom Precautions

    // ═══════════════════════════════════════════════════════════════════════════
    // Shield Adrenals (Advanced - 30% DR + absorb)
    // ═══════════════════════════════════════════════════════════════════════════
    3774812396716032i64 => ShieldInfo::new(1.77, 0.0, 0.3),    // Advanced Battle Shield Adrenal
    3774825281617920i64 => ShieldInfo::new(1.77, 0.0, 0.3),    // Advanced Field Tech Shield Adrenal
    3774833871552512i64 => ShieldInfo::new(1.77, 0.0, 0.3),    // Advanced Bio-Enhanced Shield Adrenal
    3774838166519808i64 => ShieldInfo::new(1.77, 0.0, 0.3),    // Advanced Experimental Shield Adrenal
    3774846756454400i64 => ShieldInfo::new(1.77, 0.0, 0.3),    // Advanced Exotech Shield Adrenal
    3774851051421696i64 => ShieldInfo::new(1.77, 0.0, 0.3),    // Advanced Hyper-Battle Shield Adrenal
    3774855346388992i64 => ShieldInfo::new(1.77, 0.0, 0.3),    // Advanced Nano-Infused Shield Adrenal
    3774859641356288i64 => ShieldInfo::new(1.77, 0.0, 0.3),    // Advanced Anodyne Shield Adrenal
    3827816588115968i64 => ShieldInfo::new(1.77, 0.0, 0.3),    // Advanced Battle Shield Adrenal
    3827820883083264i64 => ShieldInfo::new(1.77, 0.0, 0.3),    // Advanced Field Tech Shield Adrenal
    3827825178050560i64 => ShieldInfo::new(1.77, 0.0, 0.3),    // Advanced Bio-Enhanced Shield Adrenal
    3827829473017856i64 => ShieldInfo::new(1.77, 0.0, 0.3),    // Advanced Experimental Shield Adrenal
    3827833767985152i64 => ShieldInfo::new(1.77, 0.0, 0.3),    // Advanced Exotech Shield Adrenal
    3827838062952448i64 => ShieldInfo::new(1.77, 0.0, 0.3),    // Advanced Hyper-Battle Shield Adrenal
    3827842357919744i64 => ShieldInfo::new(1.77, 0.0, 0.3),    // Advanced Nano-Infused Shield Adrenal
    3827846652887040i64 => ShieldInfo::new(1.77, 0.0, 0.3),    // Advanced Anodyne Shield Adrenal
    3827889602560000i64 => ShieldInfo::new(1.77, 0.0, 0.3),    // Advanced Polybiotic Shield Adrenal
    3871522175320064i64 => ShieldInfo::new(1.77, 0.0, 0.3),    // Advanced Polybiotic Shield Adrenal
    4258855210975232i64 => ShieldInfo::new(1.77, 0.0, 0.3),    // Advanced Kyrprax Shield Adrenal

    // Shield Adrenals (Standard - 20% DR + absorb)
    3774872526258176i64 => ShieldInfo::new(1.58, 0.0, 0.2),    // Battle Shield Adrenal
    3776126656708608i64 => ShieldInfo::new(1.58, 0.0, 0.2),    // Field Tech Shield Adrenal
    3776130951675904i64 => ShieldInfo::new(1.58, 0.0, 0.2),    // Bio-Enhanced Shield Adrenal
    3776135246643200i64 => ShieldInfo::new(1.58, 0.0, 0.2),    // Experimental Shield Adrenal
    3776139541610496i64 => ShieldInfo::new(1.58, 0.0, 0.2),    // Exotech Shield Adrenal
    3776143836577792i64 => ShieldInfo::new(1.58, 0.0, 0.2),    // Hyper-Battle Shield Adrenal
    3776148131545088i64 => ShieldInfo::new(1.58, 0.0, 0.2),    // Nano-Infused Shield Adrenal
    3776152426512384i64 => ShieldInfo::new(1.58, 0.0, 0.2),    // Anodyne Shield Adrenal
    3827850947854336i64 => ShieldInfo::new(1.58, 0.0, 0.2),    // Battle Shield Adrenal
    3827855242821632i64 => ShieldInfo::new(1.58, 0.0, 0.2),    // Field Tech Shield Adrenal
    3827859537788928i64 => ShieldInfo::new(1.58, 0.0, 0.2),    // Bio-Enhanced Shield Adrenal
    3827863832756224i64 => ShieldInfo::new(1.58, 0.0, 0.2),    // Experimental Shield Adrenal
    3827868127723520i64 => ShieldInfo::new(1.58, 0.0, 0.2),    // Exotech Shield Adrenal
    3827872422690816i64 => ShieldInfo::new(1.58, 0.0, 0.2),    // Hyper-Battle Shield Adrenal
    3827876717658112i64 => ShieldInfo::new(1.58, 0.0, 0.2),    // Nano-Infused Shield Adrenal
    3827881012625408i64 => ShieldInfo::new(1.58, 0.0, 0.2),    // Anodyne Shield Adrenal
    3827885307592704i64 => ShieldInfo::new(1.58, 0.0, 0.2),    // Polybiotic Shield Adrenal
    3871526470287360i64 => ShieldInfo::new(1.58, 0.0, 0.2),    // Polybiotic Shield Adrenal
    4258859505942528i64 => ShieldInfo::new(1.58, 0.0, 0.2),    // Prototype Kyrprax Shield Adrenal

    // ═══════════════════════════════════════════════════════════════════════════
    // Shield Defense (Tank passive absorb shields - many variants)
    // ═══════════════════════════════════════════════════════════════════════════
    3312970268409856i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Shield Defense
    3318506481254400i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Shield Defense
    3302731066376192i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Shield Defense
    3302735361343488i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Shield Defense
    3366988072091648i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Shield Defense
    3303156268138496i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Shield Defense
    3655605579415552i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Shield Defense
    3655609874382848i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Shield Defense
    3655614169350144i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Shield Defense
    3655618464317440i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Shield Defense
    3729526261547008i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Shield Defense
    3655622759284736i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Shield Defense
    3655627054252032i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Shield Defense
    3479834042826752i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Shield Defense
    3479829747859456i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Shield Defense
    3405621302919168i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Shield Defense
    3417599966707712i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Shield Defense
    3417604261675008i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Shield Defense
    3417565606969344i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Shield Defense
    3417569901936640i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Shield Defense
    3728710217760768i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Shield Defense
    3655631349219328i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Shield Defense
    3728714512728064i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Shield Defense
    3724703013273600i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Shield Defense
    3706488056971264i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Shield Defense
    3706483762003968i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Shield Defense
    3647032824692736i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Shield Defense
    3871234412511232i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Shield Defense
    3871092678590464i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Shield Defense
    3871109858459648i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Shield Defense
    4021919045124096i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Shield Defense
    3871273067216896i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Shield Defense
    4048998813925376i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Shield Defense
    3871337491726336i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Shield Defense
    3871384736366592i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Shield Defense
    4049046058565632i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Shield Defense
    4049050353532928i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Shield Defense
    3647539630833664i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Shield Defense
    3647543925800960i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Shield Defense
    3647548220768256i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Shield Defense
    3647552515735552i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Shield Defense
    3647556810702848i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Shield Defense
    3647561105670144i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Shield Defense
    3647565400637440i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Shield Defense
    3647569695604736i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Shield Defense
    3647573990572032i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Shield Defense
    3647578285539328i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Shield Defense
    3647582580506624i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Shield Defense
    3647586875473920i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Shield Defense
    3647591170441216i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Shield Defense
    3405415144488960i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Shield Defense
    3647595465408512i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Shield Defense
    3810108437954560i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Shield Defense
    3871118448394240i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Shield Defense
    3647599760375808i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Shield Defense
    3647604055343104i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Shield Defense
    3647608350310400i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Shield Defense
    3647612645277696i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Shield Defense
    3647616940244992i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Shield Defense
    3647625530179584i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Shield Defense
    3647629825146880i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Shield Defense
    3647634120114176i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Shield Defense
    3647638415081472i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Shield Defense
    3647642710048768i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Shield Defense
    3647647005016064i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Shield Defense
    3647651299983360i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Shield Defense
    3647655594950656i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Shield Defense
    3405101611876352i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Shield Defense
    3647664184885248i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Shield Defense
    3724698718306304i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Shield Defense
    3871122743361536i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Shield Defense

    // ═══════════════════════════════════════════════════════════════════════════
    // Absorb Shield (Generic / NPC variants with varying values)
    // ═══════════════════════════════════════════════════════════════════════════
    4103600733159424i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Absorb Shield
    4103665157668864i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Absorb Shield
    4103669452636160i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Absorb Shield
    4211193958891520i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Absorb Shield
    4211215433728000i64 => ShieldInfo::new(0.19, 0.0, 1.0),    // Absorb Shield
    4293760410189824i64 => ShieldInfo::new(0.193, 0.0, 1.0),   // Absorb Shield
    4293769000124416i64 => ShieldInfo::new(0.196, 0.0, 1.0),   // Absorb Shield
    4293773295091712i64 => ShieldInfo::new(0.199, 0.0, 1.0),   // Absorb Shield
    4293777590059008i64 => ShieldInfo::new(0.203, 0.0, 1.0),   // Absorb Shield
    4293781885026304i64 => ShieldInfo::new(0.206, 0.0, 1.0),   // Absorb Shield
    4293786179993600i64 => ShieldInfo::new(0.209, 0.0, 1.0),   // Absorb Shield
    4293790474960896i64 => ShieldInfo::new(0.213, 0.0, 1.0),   // Absorb Shield
    4293794769928192i64 => ShieldInfo::new(0.216, 0.0, 1.0),   // Absorb Shield
    4293799064895488i64 => ShieldInfo::new(0.219, 0.0, 1.0),   // Absorb Shield
    4293803359862784i64 => ShieldInfo::new(0.222, 0.0, 1.0),   // Absorb Shield
    4293807654830080i64 => ShieldInfo::new(0.226, 0.0, 1.0),   // Absorb Shield
    4293811949797376i64 => ShieldInfo::new(0.229, 0.0, 1.0),   // Absorb Shield
    4293816244764672i64 => ShieldInfo::new(0.233, 0.0, 1.0),   // Absorb Shield
    4293820539731968i64 => ShieldInfo::new(0.236, 0.0, 1.0),   // Absorb Shield
    4293824834699264i64 => ShieldInfo::new(0.239, 0.0, 1.0),   // Absorb Shield
    4293829129666560i64 => ShieldInfo::new(0.243, 0.0, 1.0),   // Absorb Shield
    4293833424633856i64 => ShieldInfo::new(0.246, 0.0, 1.0),   // Absorb Shield
    4293837719601152i64 => ShieldInfo::new(0.25, 0.0, 1.0),    // Absorb Shield
    4293842014568448i64 => ShieldInfo::new(0.253, 0.0, 1.0),   // Absorb Shield
    4293846309535744i64 => ShieldInfo::new(0.257, 0.0, 1.0),   // Absorb Shield
};
