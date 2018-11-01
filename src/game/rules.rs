//! This module contains util functions and classes that help enforcing game rules
use std::collections::{HashSet};
use std::iter::FromIterator;

use super::consts::MIN_LOCATION_LAND_COVERAGE_PCT;
use super::ids::ID;
use super::location::{Coord, Location, LocationValidationError};

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Ord, PartialOrd)]
pub enum LocationRulesValidationError {
    NoLand,
    InsufficientLand(u8),
    UnconnectedLand,
    NotCoveredWithRegions(Coord),
    InitiationError(LocationValidationError),
    MisplacedUnit(Coord),
    RegionContainsWater(ID),
}

impl From<LocationValidationError> for LocationRulesValidationError {
    fn from(e: LocationValidationError) -> Self {
        LocationRulesValidationError::InitiationError(e)
    }
}

/// This method checks that location is generally valid and constructed according to game rules:
///
/// - There should be one and only one piece of land, covering more than
///   `MIN_LOCATION_LAND_COVERAGE_PCT` of location;
/// - Land should be fully covered with nonintersecting regions;
/// - All regions should cover only land, not water;
/// - All units should be places on land, not on water;
///
pub fn validate_location(location: &Location) -> Result<(), LocationRulesValidationError> {
    // Start with checking general location consistency
    Location::validate(location)?;

    // Check if there are coordinates that are land and not part of any region
    // Also check if there are coordinates that are water and part of region.
    for (coordinate, tile) in location.map().iter() {
        if tile.surface().is_land() && location.region_at(coordinate).is_none() {
            return Err(LocationRulesValidationError::NotCoveredWithRegions(
                coordinate.clone(),
            ));
        } else if tile.surface().is_water() && location.region_at(coordinate).is_some() {
            return Err(LocationRulesValidationError::RegionContainsWater(
                location.region_at(coordinate).unwrap().id(),
            ));
        }
    }

    // Check if we have any land
    let mut first_land = None;
    for (coordinate, tile) in location.map().iter() {
        if tile.surface().is_land() {
            first_land = Some(coordinate.to_owned());
            break;
        }
    }

    if first_land.is_none() {
        return Err(LocationRulesValidationError::NoLand);
    }

    // Check if there are pieces of land that do not have ground connection
    let land = location.bfs_iter(&first_land.unwrap(), |c| {
        location.tile_at(c).map_or(false, |t| t.surface().is_land())
    });
    let land: HashSet<Coord> = HashSet::from_iter(land);

    for (coordinate, tile) in location.map().iter() {
        if tile.surface().is_land() && !land.contains(coordinate) {
            return Err(LocationRulesValidationError::UnconnectedLand);
        }
    }

    // Check if land size is higher than min_coverage
    // We can rely on land containing all land cells
    let real_coverage = (land.len() * 100 / location.map().len()) as u8;
    if MIN_LOCATION_LAND_COVERAGE_PCT > real_coverage {
        return Err(LocationRulesValidationError::InsufficientLand(
            real_coverage,
        ));
    }

    // Check if there are unit that are placed on inappropriate surface
    // (Currently you can place unit only on land)
    for (coordinate, tile) in location.map().iter() {
        if tile.unit().is_some() && tile.surface().is_water() {
            return Err(LocationRulesValidationError::MisplacedUnit(*coordinate));
        }
    }

    // Return none because no errors were found
    Ok(())
}

pub enum RegionsValidationError {
    NoActiveRegions(ID),
}

/// Validate that each active player has at least one active region
pub fn validate_regions(
    location: &Location,
    active_players_ids: Vec<ID>,
) -> Option<RegionsValidationError> {
    unimplemented!()
}

#[cfg(test)]
mod test {
    use std::collections::{HashMap, HashSet};

    use game::location::TileSurface::*;
    use game::location::{
        Coord, Location, Player, Region, Tile, TileSurface,
    };
    use game::unit::{Unit, UnitType};

    use super::{validate_location, LocationRulesValidationError};

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
    fn test_map(surfaces: [TileSurface; 7]) -> HashMap<Coord, Tile> {
        let mut map = HashMap::default();
        map.insert(Coord::new(0, 1), Tile::new(1, surfaces[0]));
        map.insert(Coord::new(1, 0), Tile::new(2, surfaces[1]));
        map.insert(Coord::new(-1, 1), Tile::new(3, surfaces[2]));
        map.insert(Coord::new(0, 0), Tile::new(4, surfaces[3]));
        map.insert(Coord::new(1, -1), Tile::new(5, surfaces[4]));
        map.insert(Coord::new(-1, 0), Tile::new(6, surfaces[5]));
        map.insert(Coord::new(0, -1), Tile::new(7, surfaces[6]));
        map
    }

    #[test]
    fn validate_location_no_errors() {
        let mut map = test_map([Water, Water, Land, Land, Land, Water, Land]);
        map.get_mut(&Coord::new(0, 0))
            .unwrap()
            .place_unit(Unit::new(31, UnitType::Soldier));
        map.get_mut(&Coord::new(0, -1))
            .unwrap()
            .place_unit(Unit::new(31, UnitType::Tower));

        let mut coords_one = HashSet::default();
        coords_one.insert(Coord::new(-1, 1));
        coords_one.insert(Coord::new(0, 0));
        let region_one = Region::new(11, Player::new(21), coords_one);

        let mut coords_two = HashSet::default();
        coords_two.insert(Coord::new(1, -1));
        coords_two.insert(Coord::new(0, -1));
        let region_two = Region::new(12, Player::new(22), coords_two);
        let location = Location::new(map, vec![region_one, region_two]).unwrap();
        let res = validate_location(&location);

        assert_eq!(res, Ok(()));
    }

    #[test]
    fn validate_location_no_land() {
        let map = test_map([Water, Water, Water, Water, Water, Water, Water]);
        let location = Location::new(map, Vec::new()).unwrap();
        let res = validate_location(&location);

        assert_eq!(res, Err(LocationRulesValidationError::NoLand));
    }

    #[test]
    fn validate_location_unconnected_land() {
        let mut map = test_map([Land, Water, Land, Water, Land, Water, Land]);
        map.get_mut(&Coord::new(0, 1))
            .unwrap()
            .place_unit(Unit::new(31, UnitType::Soldier));
        map.get_mut(&Coord::new(0, -1))
            .unwrap()
            .place_unit(Unit::new(31, UnitType::Tower));

        let mut coords_one = HashSet::default();
        coords_one.insert(Coord::new(-1, 1));
        coords_one.insert(Coord::new(0, 1));
        let region_one = Region::new(11, Player::new(21), coords_one);

        let mut coords_two = HashSet::default();
        coords_two.insert(Coord::new(1, -1));
        coords_two.insert(Coord::new(0, -1));
        let region_two = Region::new(12, Player::new(22), coords_two);
        let location = Location::new(map, vec![region_one, region_two]).unwrap();
        let res = validate_location(&location);

        assert_eq!(res, Err(LocationRulesValidationError::UnconnectedLand));
    }

    #[test]
    fn validate_location_not_covered_with_region() {
        let mut map = test_map([Land, Water, Land, Land, Land, Water, Land]);
        map.get_mut(&Coord::new(0, 1))
            .unwrap()
            .place_unit(Unit::new(31, UnitType::Soldier));
        map.get_mut(&Coord::new(0, -1))
            .unwrap()
            .place_unit(Unit::new(31, UnitType::Tower));

        let mut coords_one = HashSet::default();
        coords_one.insert(Coord::new(-1, 1));
        coords_one.insert(Coord::new(0, 1));
        let region_one = Region::new(11, Player::new(21), coords_one);

        let mut coords_two = HashSet::default();
        coords_two.insert(Coord::new(1, -1));
        coords_two.insert(Coord::new(0, -1));
        let region_two = Region::new(12, Player::new(22), coords_two);
        let location = Location::new(map, vec![region_one, region_two]).unwrap();
        let res = validate_location(&location);

        assert_eq!(
            res,
            Err(LocationRulesValidationError::NotCoveredWithRegions(
                Coord::new(0, 0)
            ))
        );
    }

    #[test]
    fn validate_location_misplaced_unit() {
        let mut map = test_map([Water, Water, Land, Land, Land, Water, Land]);
        map.get_mut(&Coord::new(0, 1))
            .unwrap()
            .place_unit(Unit::new(31, UnitType::Soldier));
        map.get_mut(&Coord::new(0, -1))
            .unwrap()
            .place_unit(Unit::new(31, UnitType::Tower));

        let mut coords_one = HashSet::default();
        coords_one.insert(Coord::new(-1, 1));
        coords_one.insert(Coord::new(0, 0));
        let region_one = Region::new(11, Player::new(21), coords_one);

        let mut coords_two = HashSet::default();
        coords_two.insert(Coord::new(1, -1));
        coords_two.insert(Coord::new(0, -1));
        let region_two = Region::new(12, Player::new(22), coords_two);
        let location = Location::new(map, vec![region_one, region_two]).unwrap();
        let res = validate_location(&location);

        assert_eq!(
            res,
            Err(LocationRulesValidationError::MisplacedUnit(Coord::new(
                0, 1
            )))
        );
    }

    #[test]
    fn validate_location_insufficient_land() {
        let mut map = test_map([Water, Water, Land, Land, Land, Water, Water]);
        map.get_mut(&Coord::new(0, 0))
            .unwrap()
            .place_unit(Unit::new(31, UnitType::Soldier));
        map.get_mut(&Coord::new(0, -1))
            .unwrap()
            .place_unit(Unit::new(31, UnitType::Tower));

        let mut coords_one = HashSet::default();
        coords_one.insert(Coord::new(-1, 1));
        coords_one.insert(Coord::new(0, 0));
        let region_one = Region::new(11, Player::new(21), coords_one);

        let mut coords_two = HashSet::default();
        coords_two.insert(Coord::new(1, -1));
        let region_two = Region::new(12, Player::new(22), coords_two);
        let location = Location::new(map, vec![region_one, region_two]).unwrap();
        let res = validate_location(&location);

        assert_eq!(
            res,
            Err(LocationRulesValidationError::InsufficientLand(42))
        );
    }

    #[test]
    fn validate_location_region_contains_water() {
        let mut map = test_map([Water, Water, Land, Land, Land, Water, Land]);
        map.get_mut(&Coord::new(0, 0))
            .unwrap()
            .place_unit(Unit::new(31, UnitType::Soldier));
        map.get_mut(&Coord::new(0, -1))
            .unwrap()
            .place_unit(Unit::new(31, UnitType::Tower));

        let mut coords_one = HashSet::default();
        coords_one.insert(Coord::new(0, 1));
        coords_one.insert(Coord::new(-1, 1));
        coords_one.insert(Coord::new(0, 0));
        let region_one = Region::new(11, Player::new(21), coords_one);

        let mut coords_two = HashSet::default();
        coords_two.insert(Coord::new(1, -1));
        coords_two.insert(Coord::new(0, -1));
        let region_two = Region::new(12, Player::new(22), coords_two);
        let location = Location::new(map, vec![region_one, region_two]).unwrap();
        let res = validate_location(&location);

        assert_eq!(
            res,
            Err(LocationRulesValidationError::RegionContainsWater(11))
        );
    }
}
