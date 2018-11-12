//! This module contains constants used for specifying game rules.
use super::location::UnitType;
use super::unit::UnitDescription;

pub const MIN_CONTROLLED_REGION_SIZE: usize = 2;

pub const EMPTY_TILE_DEFENCE: u8 = 0;

pub const EMPTY_TILE_INCOME: i32 = 1;

pub const CONTROLLED_REGION_STARTING_MONEY: i32 = 10;

pub const MIN_LOCATION_LAND_COVERAGE_PCT: u8 = 50;

pub const GRAVE: UnitDescription = UnitDescription {
    name: UnitType::Grave,
    is_purchasable: false,
    purchase_cost: 0,
    turn_cost: 0,
    max_moves: 0,
    defence: 0,
    attack: 0,
    upgrades_to: None,
};

pub const TREE: UnitDescription = UnitDescription {
    name: UnitType::Tree,
    is_purchasable: false,
    purchase_cost: 0,
    turn_cost: 1,
    max_moves: 0,
    defence: 0,
    attack: 0,
    upgrades_to: None,
};

pub const VILLAGE: UnitDescription = UnitDescription {
    name: UnitType::Village,
    is_purchasable: false,
    purchase_cost: 0,
    turn_cost: 0,
    max_moves: 0,
    defence: 1,
    attack: 0,
    upgrades_to: None,
};

pub const TOWER: UnitDescription = UnitDescription {
    name: UnitType::Tower,
    is_purchasable: true,
    purchase_cost: 15,
    turn_cost: 0,
    max_moves: 0,
    defence: 2,
    attack: 0,
    upgrades_to: None,
};

pub const GREAT_KNIGHT: UnitDescription = UnitDescription {
    name: UnitType::GreatKnight,
    is_purchasable: true,
    purchase_cost: 40,
    turn_cost: 4,
    max_moves: 5,
    defence: 4,
    attack: 4,
    upgrades_to: None,
};

pub const KNIGHT: UnitDescription = UnitDescription {
    name: UnitType::Knight,
    is_purchasable: true,
    purchase_cost: 30,
    turn_cost: 3,
    max_moves: 5,
    defence: 3,
    attack: 3,
    upgrades_to: Some(&GREAT_KNIGHT),
};

pub const SOLDIER: UnitDescription = UnitDescription {
    name: UnitType::Soldier,
    is_purchasable: true,
    purchase_cost: 20,
    turn_cost: 2,
    max_moves: 5,
    defence: 2,
    attack: 2,
    upgrades_to: Some(&KNIGHT),
};

pub const MILITIA: UnitDescription = UnitDescription {
    name: UnitType::Militia,
    is_purchasable: true,
    purchase_cost: 10,
    turn_cost: 1,
    max_moves: 5,
    defence: 1,
    attack: 1,
    upgrades_to: Some(&SOLDIER),
};
