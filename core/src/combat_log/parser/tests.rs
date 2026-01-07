use super::*;
use crate::context::resolve;
use chrono::NaiveDateTime;

fn test_parser() -> LogParser {
    let date = NaiveDateTime::parse_from_str("2024-01-01 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
    LogParser::new(date)
}

// parse_entity
#[test]
fn test_parse_entity_npc() {
    let parser = test_parser();
    let input = "Dread Master Bestia {3273941900591104}:5320000112163|(137.28,-120.98,-8.85,81.28)|(0/19129210)";
    let result = parser.parse_entity(input);
    assert!(result.is_some());

    let entity = result.unwrap();

    assert_eq!(resolve(entity.name), "Dread Master Bestia");
    assert_eq!(entity.class_id, 3273941900591104);
    assert_eq!(entity.log_id, 5320000112163);
    assert_eq!(entity.entity_type, EntityType::Npc);
    assert_eq!(entity.health, (0, 19129210));
}

#[test]
fn test_parse_entity_player() {
    let parser = test_parser();
    let input = "@Galen Ayder#690129185314118|(-4700.43,-4750.48,710.03,-0.71)|(1/414851)";
    let result = parser.parse_entity(input);
    assert!(result.is_some());

    let entity = result.unwrap();

    assert_eq!(resolve(entity.name), "Galen Ayder");
    assert_eq!(entity.class_id, 0);
    assert_eq!(entity.log_id, 690129185314118);
    assert_eq!(entity.entity_type, EntityType::Player);
    assert_eq!(entity.health, (1, 414851));
}

#[test]
fn test_parse_entity_companion() {
    let parser = test_parser();
    let input = "@Jerran Zeva#689501114780828/Raina Temple {493328533553152}:87481369009487|(4749.87,4694.53,710.05,0.00)|(288866/288866)";
    let result = parser.parse_entity(input);
    assert!(result.is_some());

    let entity = result.unwrap();
    assert_eq!(resolve(entity.name), "Raina Temple");
    assert_eq!(entity.class_id, 493328533553152);
    assert_eq!(entity.log_id, 87481369009487);
    assert_eq!(entity.entity_type, EntityType::Companion);
    assert_eq!(entity.health, (288866, 288866));
}

#[test]
fn test_parse_entity_self_reference() {
    let parser = test_parser();
    let input = "=";
    let result = parser.parse_entity(input);
    assert!(result.is_some());

    let entity = result.unwrap();
    assert_eq!(entity.entity_type, EntityType::SelfReference);
}

#[test]
fn test_parse_entity_empty() {
    let parser = test_parser();
    let input = "";
    let result = parser.parse_entity(input);
    assert!(result.is_some());

    let entity = result.unwrap();

    assert_eq!(entity.entity_type, EntityType::Empty);
}

// parse_charges
#[test]
fn test_parse_charges_one() {
    let input = "(1 charges {836045448953667})";
    let result = LogParser::parse_charges(input);
    assert!(result.is_some());
    let details = result.unwrap();
    assert_eq!(details.charges, 1);
}

#[test]
fn test_parse_charges_ten() {
    let input = "(10 charges {836045448953667})";
    let result = LogParser::parse_charges(input);
    assert!(result.is_some());

    let details = result.unwrap();
    assert_eq!(details.charges, 10);
}

// parse_details
#[test]
fn test_parse_details_damage_basic() {
    let input = " (5765 energy {836045448940874}) <5765.0>";
    let result = LogParser::parse_details(input, effect_id::DAMAGE, effect_type_id::APPLYEFFECT);
    assert!(result.is_some());

    let details = result.unwrap();
    assert_eq!(details.dmg_amount, 5765);
    assert_eq!(details.dmg_effective, 5765);
    assert_eq!(resolve(details.dmg_type), "energy");
    assert_eq!(details.dmg_type_id, 836045448940874);
    assert_eq!(details.threat, 5765.0);
    assert!(!details.is_crit);
    assert!(!details.is_reflect);
}

#[test]
fn test_parse_details_damage_crit() {
    let input = " (7500* energy {836045448940874}) <7500.0>";
    let result = LogParser::parse_details(input, effect_id::DAMAGE, effect_type_id::APPLYEFFECT);
    assert!(result.is_some());

    let details = result.unwrap();
    assert_eq!(details.dmg_amount, 7500);
    assert_eq!(details.dmg_effective, 7500);
    assert!(details.is_crit);
}

#[test]
fn test_parse_details_damage_with_effective() {
    let input = " (5000 ~3500 kinetic {836045448940873}) <3500.0>";
    let result = LogParser::parse_details(input, effect_id::DAMAGE, effect_type_id::APPLYEFFECT);
    assert!(result.is_some());

    let details = result.unwrap();
    assert_eq!(details.dmg_amount, 5000);
    assert_eq!(details.dmg_effective, 3500);
    assert_eq!(resolve(details.dmg_type), "kinetic");
}

#[test]
fn test_parse_details_damage_with_absorbed() {
    let input =
        " (5000 ~3000 kinetic {836045448940873} (2000 absorbed {836045448945511})) <5000.0>";
    let result = LogParser::parse_details(input, effect_id::DAMAGE, effect_type_id::APPLYEFFECT);
    assert!(result.is_some());

    let details = result.unwrap();
    assert_eq!(details.dmg_amount, 5000);
    assert_eq!(resolve(details.dmg_type), "kinetic");
    assert_eq!(details.dmg_effective, 3000);
    assert_eq!(details.dmg_absorbed, 2000);
}

#[test]
fn test_parse_details_damage_miss() {
    let input = " (0 -miss {836045448945502}) <15000.0>";
    let result = LogParser::parse_details(input, effect_id::DAMAGE, effect_type_id::APPLYEFFECT);
    assert!(result.is_some());

    let details = result.unwrap();
    assert_eq!(details.dmg_amount, 0);
    assert_eq!(resolve(details.dmg_type), "");
    assert_eq!(details.defense_type_id, defense_type::MISS);
}

#[test]
fn test_parse_dmage_shielded() {
    let input = "(2583* energy {836045448940874} -shield {836045448945509} (1150 absorbed {836045448945511})) <2583.0>";
    let result = LogParser::parse_details(input, effect_id::DAMAGE, effect_type_id::APPLYEFFECT);
    assert!(result.is_some());

    let details = result.unwrap();
    assert_eq!(resolve(details.dmg_type), "energy");
    assert_eq!(details.dmg_absorbed, 1150);
    assert_eq!(details.defense_type_id, defense_type::SHIELD);
    assert_eq!(details.dmg_effective, 2583)
}

#[test]
fn test_parse_damage_after_death() {
    let input = "(41422 ~0 energy {836045448940874} -)";
    let result = LogParser::parse_details(input, effect_id::DAMAGE, effect_type_id::APPLYEFFECT);

    assert!(result.is_some());

    let details = result.unwrap();
    assert_eq!(details.defense_type_id, 0);
    assert_eq!(details.dmg_effective, 0);
    assert_eq!(resolve(details.dmg_type), "energy");
}

#[test]
fn test_parse_details_damage_reflect() {
    let input = "(116010 kinetic {836045448940873}(reflected {836045448953649}))";
    let result = LogParser::parse_details(input, effect_id::DAMAGE, effect_type_id::APPLYEFFECT);
    let details = result.unwrap();

    assert!(details.is_reflect);
    assert_eq!(details.dmg_effective, 116010);
    assert_eq!(resolve(details.dmg_type), "kinetic");
}

#[test]
fn test_parse_details_damage_reflect_nullified() {
    let input = " (0 -) <1500.0>";
    let result = LogParser::parse_details(input, effect_id::DAMAGE, effect_type_id::APPLYEFFECT);
    assert!(result.is_some());

    let details = result.unwrap();
    assert_eq!(details.dmg_amount, 0);
    assert!(details.is_reflect);
    assert_eq!(details.threat, 1500.0)
}

#[test]
fn test_parse_details_heal_basic() {
    let input = " (3500) <1750>";
    let result = LogParser::parse_details(input, effect_id::HEAL, effect_type_id::APPLYEFFECT);
    assert!(result.is_some());

    let details = result.unwrap();
    assert_eq!(details.heal_amount, 3500);
    assert_eq!(details.heal_effective, 3500);
    assert_eq!(details.threat, 1750.0);
    assert!(!details.is_crit);
}

#[test]
fn test_parse_details_heal_crit() {
    let input = " (5000*) <2500>";
    let result = LogParser::parse_details(input, effect_id::HEAL, effect_type_id::APPLYEFFECT);
    assert!(result.is_some());

    let details = result.unwrap();
    assert_eq!(details.heal_amount, 5000);
    assert!(details.is_crit);
}

#[test]
fn test_parse_details_heal_with_effective() {
    let input = " (4000 ~2000) <1000>";
    let result = LogParser::parse_details(input, effect_id::HEAL, effect_type_id::APPLYEFFECT);
    assert!(result.is_some());

    let details = result.unwrap();
    assert_eq!(details.heal_amount, 4000);
    assert_eq!(details.heal_effective, 2000);
    assert_eq!(details.threat, 1000.0);
}

#[test]
fn test_parse_details_modify_charges() {
    let input = " (3 charges {836045448953667})";
    let result = LogParser::parse_details(
        input,
        effect_id::ABILITYACTIVATE,
        effect_type_id::MODIFYCHARGES,
    );
    assert!(result.is_some());

    let details = result.unwrap();
    assert_eq!(details.charges, 3);
    assert_eq!(details.ability_id, 836045448953667);
}

#[test]
fn test_parse_details_apply_effect_with_charges() {
    let input = " (5 charges {836045448953667})";
    let result = LogParser::parse_details(
        input,
        effect_id::ABILITYACTIVATE,
        effect_type_id::APPLYEFFECT,
    );
    assert!(result.is_some());

    let details = result.unwrap();
    assert_eq!(details.charges, 5);
}

#[test]
fn test_parse_details_default() {
    let input = "";
    let result = LogParser::parse_details(
        input,
        effect_id::ABILITYACTIVATE,
        effect_type_id::DISCIPLINECHANGED,
    );
    assert!(result.is_some());

    let details = result.unwrap();
    assert_eq!(details.dmg_amount, 0);
    assert_eq!(details.heal_amount, 0);
    assert_eq!(details.charges, 0);
}
