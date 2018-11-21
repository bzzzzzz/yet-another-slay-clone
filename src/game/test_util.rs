//! Here lives some shared test behaviour
use std::collections::HashMap;

use super::engine::GameEngine;
use super::ids::{IdProducer, ID};
use super::location::TileSurface::*;
use super::location::{Coord, Location, Player, Region, Tile, TileSurface, Unit, UnitType};

/// This test method creates a small hex map like this one:
///  * *
/// * * *
///  * *
/// This game uses axial coordinates hexes with pointy tops, so coordinates will be:
///    (0,1)   (1,0)
/// (-1,1) (0,0) (1,-1)
///   (-1, 0)  (0,-1)
///
/// Surfaces array represents surfaces of each of seven points starting from top left one
pub fn create_simple_map(surfaces: [TileSurface; 7]) -> HashMap<Coord, Tile> {
    let mut id_producer = IdProducer::default();
    let mut map = HashMap::default();
    map.insert(
        Coord::new(0, 1),
        Tile::new(id_producer.next_id(), surfaces[0]),
    );
    map.insert(
        Coord::new(1, 0),
        Tile::new(id_producer.next_id(), surfaces[1]),
    );
    map.insert(
        Coord::new(-1, 1),
        Tile::new(id_producer.next_id(), surfaces[2]),
    );
    map.insert(
        Coord::new(0, 0),
        Tile::new(id_producer.next_id(), surfaces[3]),
    );
    map.insert(
        Coord::new(1, -1),
        Tile::new(id_producer.next_id(), surfaces[4]),
    );
    map.insert(
        Coord::new(-1, 0),
        Tile::new(id_producer.next_id(), surfaces[5]),
    );
    map.insert(
        Coord::new(0, -1),
        Tile::new(id_producer.next_id(), surfaces[6]),
    );
    map
}

pub fn create_map(
    surfaces: [TileSurface; 12],
    id_producer: &mut IdProducer,
) -> HashMap<Coord, Tile> {
    let mut map = HashMap::default();

    map.insert(
        Coord::new(0, -1),
        Tile::new(id_producer.next_id(), surfaces[0]),
    );
    map.insert(
        Coord::new(1, -1),
        Tile::new(id_producer.next_id(), surfaces[1]),
    );
    map.insert(
        Coord::new(2, -1),
        Tile::new(id_producer.next_id(), surfaces[2]),
    );
    map.insert(
        Coord::new(3, -1),
        Tile::new(id_producer.next_id(), surfaces[3]),
    );
    map.insert(
        Coord::new(-1, 0),
        Tile::new(id_producer.next_id(), surfaces[4]),
    );
    map.insert(
        Coord::new(0, 0),
        Tile::new(id_producer.next_id(), surfaces[5]),
    );
    map.insert(
        Coord::new(1, 0),
        Tile::new(id_producer.next_id(), surfaces[6]),
    );
    map.insert(
        Coord::new(2, 0),
        Tile::new(id_producer.next_id(), surfaces[7]),
    );
    map.insert(
        Coord::new(-2, 1),
        Tile::new(id_producer.next_id(), surfaces[8]),
    );
    map.insert(
        Coord::new(-1, 1),
        Tile::new(id_producer.next_id(), surfaces[9]),
    );
    map.insert(
        Coord::new(0, 1),
        Tile::new(id_producer.next_id(), surfaces[10]),
    );
    map.insert(
        Coord::new(1, 1),
        Tile::new(id_producer.next_id(), surfaces[11]),
    );

    map
}

/// This function will create valid engine for testing with following structure:
///
///  Surface:    Owners:
///   * V * ~     1 1 1 ~
///  V ~ S V     3 ~ 1 2
/// * * M *     3 1 2 2
///
/// Coordinates:
///       0/-1  1/-1  2/-1  3/-1
///   -1/0   0/0   1/0   2/0
/// -2/1  -1/1  0/1   1/1
///
pub fn create_valid_engine() -> (Vec<Player>, Vec<ID>, GameEngine) {
    let mut id_producer = IdProducer::default();
    let mut map = create_map(
        [
            Land, Land, Land, Water, Land, Water, Land, Land, Land, Land, Land, Land,
        ],
        &mut id_producer,
    );
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

    let game_engine = GameEngine::new(
        location,
        vec![players[0], players[1], players[2]],
        id_producer,
    ).unwrap();

    (players, region_ids, game_engine)
}
