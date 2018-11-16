extern crate chrono;
extern crate tempfile;
extern crate yasc;

use std::cmp::Ordering;
use std::collections::HashMap;

use chrono::prelude::*;
use chrono::Duration;

use yasc::game::{
    Coord, GameEngine, IdProducer, Location, Player, Region, Tile, TileSurface::*, Unit, UnitType,
    ID,
};
use yasc::saves::SavedGamesCatalog;

fn create_valid_engine() -> (Vec<Player>, Vec<ID>, GameEngine) {
    let mut id_producer = IdProducer::default();
    let mut map = HashMap::default();
    map.insert(Coord::new(0, -1), Tile::new(id_producer.next_id(), Land));
    map.insert(Coord::new(1, -1), Tile::new(id_producer.next_id(), Land));
    map.insert(Coord::new(2, -1), Tile::new(id_producer.next_id(), Land));
    map.insert(Coord::new(3, -1), Tile::new(id_producer.next_id(), Water));
    map.insert(Coord::new(-1, 0), Tile::new(id_producer.next_id(), Land));
    map.insert(Coord::new(0, 0), Tile::new(id_producer.next_id(), Water));
    map.insert(Coord::new(1, 0), Tile::new(id_producer.next_id(), Land));
    map.insert(Coord::new(2, 0), Tile::new(id_producer.next_id(), Land));
    map.insert(Coord::new(-2, 1), Tile::new(id_producer.next_id(), Land));
    map.insert(Coord::new(-1, 1), Tile::new(id_producer.next_id(), Land));
    map.insert(Coord::new(0, 1), Tile::new(id_producer.next_id(), Land));
    map.insert(Coord::new(1, 1), Tile::new(id_producer.next_id(), Land));

    let players = vec![
        Player::new(id_producer.next_id()),
        Player::new(id_producer.next_id()),
        Player::new(id_producer.next_id()),
    ];
    let region_ids = vec![
        id_producer.next_id(),
        id_producer.next_id(),
        id_producer.next_id(),
        id_producer.next_id(),
    ];

    let coords = [
        Coord::new(0, -1),
        Coord::new(1, -1),
        Coord::new(2, -1),
        Coord::new(1, 0),
    ]
        .iter()
        .cloned()
        .collect();
    let region_one = Region::new(region_ids[0], players[0], coords);

    let coords = [Coord::new(2, 0), Coord::new(1, 1), Coord::new(0, 1)]
        .iter()
        .cloned()
        .collect();
    let region_two = Region::new(region_ids[1], players[1], coords);

    let coords = [Coord::new(-1, 1)].iter().cloned().collect();
    let region_three = Region::new(region_ids[2], players[0], coords);

    let coords = [Coord::new(-1, 0), Coord::new(-2, 1)]
        .iter()
        .cloned()
        .collect();
    let region_four = Region::new(region_ids[3], players[2], coords);

    let mut location =
        Location::new(map, vec![region_one, region_two, region_three, region_four]).unwrap();
    location
        .place_unit(
            Unit::new(id_producer.next_id(), UnitType::Village),
            Coord::new(1, -1),
        ).unwrap();
    location
        .place_unit(
            Unit::new(id_producer.next_id(), UnitType::Soldier),
            Coord::new(1, 0),
        ).unwrap();
    location
        .place_unit(
            Unit::new(id_producer.next_id(), UnitType::Militia),
            Coord::new(0, 1),
        ).unwrap();
    location
        .place_unit(
            Unit::new(id_producer.next_id(), UnitType::Village),
            Coord::new(2, 0),
        ).unwrap();
    location
        .place_unit(
            Unit::new(id_producer.next_id(), UnitType::Village),
            Coord::new(-1, 0),
        ).unwrap();

    let game_engine = GameEngine::new(location, vec![players[0], players[1], players[2]]).unwrap();

    (players, region_ids, game_engine)
}

#[test]
fn check_saved_games_catalog_is_empty_in_beginning() {
    let dir = tempfile::tempdir().unwrap();
    let catalog = SavedGamesCatalog::new(dir.path().to_str().unwrap(), "test").unwrap();

    assert_eq!(catalog.list_saved_games().len(), 0);
}

#[test]
fn check_saved_games_catalog_is_not_empty_after_saving() {
    let dir = tempfile::tempdir().unwrap();
    let mut catalog = SavedGamesCatalog::new(dir.path().to_str().unwrap(), "test").unwrap();

    assert_eq!(catalog.list_saved_games().len(), 0);

    let (_, _, engine) = create_valid_engine();
    let info = catalog.save("some_name", &engine);

    assert!(info.is_ok());
    assert_eq!(catalog.list_saved_games().len(), 1);

    let info = info.unwrap();
    assert_eq!(info.name, "some_name");
    assert_eq!(info.version, 1);
    let now = Utc::now();
    let before = now - Duration::seconds(10);
    assert_eq!(info.timestamp.cmp(&now), Ordering::Less);
    assert_eq!(info.timestamp.cmp(&before), Ordering::Greater);
}

#[test]
fn check_saved_engine_is_recoverable() {
    let dir = tempfile::tempdir().unwrap();
    let mut catalog = SavedGamesCatalog::new(dir.path().to_str().unwrap(), "test").unwrap();

    let (_, _, engine) = create_valid_engine();
    let info = catalog.save("name", &engine);

    assert!(info.is_ok());

    let loaded_engine = catalog.load(&info.unwrap());
    assert!(loaded_engine.is_ok());
    assert_eq!(loaded_engine.unwrap(), engine);
}

#[test]
fn check_saved_engine_is_recoverable_through_new_catalog() {
    let dir = tempfile::tempdir().unwrap();
    let mut catalog = SavedGamesCatalog::new(dir.path().to_str().unwrap(), "test").unwrap();

    let (_, _, engine) = create_valid_engine();
    let info = catalog.save("name", &engine);

    assert!(info.is_ok());
    let info = info.unwrap();

    let other_catalog = SavedGamesCatalog::new(dir.path().to_str().unwrap(), "test").unwrap();
    assert_eq!(other_catalog.list_saved_games().len(), 1);
    assert_eq!(
        other_catalog.list_saved_games().get(0).cloned(),
        Some(info.clone())
    );

    let loaded_engine = other_catalog.load(&info);
    assert!(loaded_engine.is_ok());
    assert_eq!(loaded_engine.unwrap(), engine);
}
