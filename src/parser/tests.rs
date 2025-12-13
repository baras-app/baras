use super::*;

// parse_entity
#[test]
fn test_parse_entity_npc() {
    let input = "[Dread Master Bestia {3273941900591104}:5320000112163|(137.28,-120.98,-8.85,81.28)|(0/19129210)]";
    let result = parse_entity(input);
    assert!(result.is_some());

    let (remaining, entity) = result.unwrap();
    assert_eq!(remaining, "");
    assert_eq!(entity.name, "Dread Master Bestia");
    assert_eq!(entity.class_id, 3273941900591104);
    assert_eq!(entity.log_id, 5320000112163);
    assert_eq!(entity.entity_type, EntityType::Npc);
    assert_eq!(entity.health, (0, 19129210));
}

#[test]
fn test_parse_entity_player() {
    let input = "[@Galen Ayder#690129185314118|(-4700.43,-4750.48,710.03,-0.71)|(1/414851)]";
    let result = parse_entity(input);
    assert!(result.is_some());

    let (remaining, entity) = result.unwrap();
    assert_eq!(remaining, "");
    assert_eq!(entity.name, "Galen Ayder");
    assert_eq!(entity.class_id, 0);
    assert_eq!(entity.log_id, 690129185314118);
    assert_eq!(entity.entity_type, EntityType::Player);
    assert_eq!(entity.health, (1, 414851));
}

#[test]
fn test_parse_entity_companion() {
    let input = "[@Jerran Zeva#689501114780828/Raina Temple {493328533553152}:87481369009487|(4749.87,4694.53,710.05,0.00)|(288866/288866)]";
    let result = parse_entity(input);
    assert!(result.is_some());

    let (remaining, entity) = result.unwrap();
    assert_eq!(remaining, "");
    assert_eq!(entity.name, "Raina Temple");
    assert_eq!(entity.class_id, 493328533553152);
    assert_eq!(entity.log_id, 87481369009487);
    assert_eq!(entity.entity_type, EntityType::Companion);
    assert_eq!(entity.health, (288866, 288866));
}

#[test]
fn test_parse_entity_self_reference() {
    let input = "[=]";
    let result = parse_entity(input);
    assert!(result.is_some());

    let (remaining, entity) = result.unwrap();
    assert_eq!(remaining, "");
    assert_eq!(entity.entity_type, EntityType::SelfReference);
}

#[test]
fn test_parse_entity_empty() {
    let input = "[]";
    let result = parse_entity(input);
    assert!(result.is_some());

    let (remaining, entity) = result.unwrap();
    assert_eq!(remaining, "");
    assert_eq!(entity.entity_type, EntityType::Empty);
}

// parse_charges
#[test]
fn test_parse_charges_one() {
    let input = "(1 charges {836045448953667})";
    let result = parse_charges(input);
    assert!(result.is_some());

    let details = result.unwrap();
    assert_eq!(details.charges, 1);
}

#[test]
fn test_parse_charges_ten() {
    let input = "(10 charges {836045448953667})";
    let result = parse_charges(input);
    assert!(result.is_some());

    let details = result.unwrap();
    assert_eq!(details.charges, 10);
}
