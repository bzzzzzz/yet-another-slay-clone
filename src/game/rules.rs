//! This module contains util functions and classes that help enforcing game rules
use std::collections::{HashMap, HashSet};

use super::consts::*;
use super::ids::ID;
use super::location::{Coord, Location, LocationValidationError, Player, UnitType};

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Ord, PartialOrd)]
pub enum LocationRulesValidationError {
    NoLand,
    InsufficientLand(u8),
    UnconnectedLand,
    NotCoveredWithRegions(Coord),
    InitiationError(LocationValidationError),
    MisplacedUnit(Coord),
    RegionContainsWater(ID),
    ActiveRegionWithoutCapital(ID),
    MultiplyCapitals(ID),
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
/// - Each region should have one village capital;
///
pub fn validate_location(location: &Location) -> Result<(), LocationRulesValidationError> {
    // Start with checking general location consistency
    Location::validate(location)?;

    // Check if there are coordinates that are land and not part of any region
    // Also check if there are coordinates that are water and part of region.
    for (coordinate, tile) in location.map().iter() {
        if tile.surface().is_land() && location.region_at(*coordinate).is_none() {
            return Err(LocationRulesValidationError::NotCoveredWithRegions(
                *coordinate,
            ));
        } else if tile.surface().is_water() && location.region_at(*coordinate).is_some() {
            return Err(LocationRulesValidationError::RegionContainsWater(
                location.region_at(*coordinate).unwrap().id(),
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
    let land = location.bfs_set(first_land.unwrap(), |c| {
        location.tile_at(c).map_or(false, |t| t.surface().is_land())
    });

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

    // Check if there are regions without capitals
    for (id, region) in location.regions() {
        if region.coordinates().len() < MIN_CONTROLLED_REGION_SIZE {
            continue;
        }
        let mut capitals = 0;
        for &coordinate in region.coordinates() {
            let tile = location.tile_at(coordinate).unwrap();
            if let Some(unit) = tile.unit() {
                if unit.unit_type() == UnitType::Village {
                    capitals += 1;
                }
            }
        }

        if capitals == 0 {
            return Err(LocationRulesValidationError::ActiveRegionWithoutCapital(
                *id,
            ));
        } else if capitals > 1 {
            return Err(LocationRulesValidationError::MultiplyCapitals(*id));
        }
    }

    // Return none because no errors were found
    Ok(())
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Ord, PartialOrd)]
pub enum RegionsValidationError {
    NoActiveRegions(ID),
    UnlistedPlayer(ID),
}

/// Validate that each active player has at least one active region
pub fn validate_regions(
    location: &Location,
    active_players: &[Player],
) -> Result<(), RegionsValidationError> {
    let mut player_is_active: HashMap<ID, bool> = HashMap::default();

    for region in location.regions().values() {
        let mut is_active = region.coordinates().len() >= MIN_CONTROLLED_REGION_SIZE;
        if !is_active {
            let unit_count = region
                .coordinates()
                .iter()
                .filter(|&c| location.tile_at(*c).unwrap().unit().is_some())
                .count();
            is_active = unit_count > 0;
        }
        let region_id = region.owner().id();
        let current_active_status = *player_is_active.get(&region_id).unwrap_or(&false);
        player_is_active.insert(region_id, current_active_status || is_active);
    }

    for player in active_players.iter() {
        if !player_is_active.contains_key(&player.id())
            || !player_is_active.contains_key(&player.id())
        {
            return Err(RegionsValidationError::NoActiveRegions(player.id()));
        }
    }

    let player_ids: HashSet<ID> = active_players.iter().map(|p| p.id()).collect();
    for (&id, &is_active) in player_is_active.iter() {
        if !is_active && player_ids.contains(&id) {
            return Err(RegionsValidationError::NoActiveRegions(id));
        } else if is_active && !player_ids.contains(&id) {
            return Err(RegionsValidationError::UnlistedPlayer(id));
        }
    }
    Ok(())
}

#[cfg(test)]
mod test {
    use std::collections::{HashMap, HashSet};

    use game::location::TileSurface::*;
    use game::location::{Coord, Location, Player, Region, Tile, TileSurface, Unit, UnitType};

    use super::{
        validate_location, validate_regions, LocationRulesValidationError, RegionsValidationError,
    };

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
        let map = test_map([Water, Water, Land, Land, Land, Water, Land]);

        let mut coords_one = HashSet::default();
        coords_one.insert(Coord::new(-1, 1));
        coords_one.insert(Coord::new(0, 0));
        let region_one = Region::new(11, Player::new(21), coords_one);

        let mut coords_two = HashSet::default();
        coords_two.insert(Coord::new(1, -1));
        coords_two.insert(Coord::new(0, -1));
        let region_two = Region::new(12, Player::new(22), coords_two);
        let mut location = Location::new(map, vec![region_one, region_two]).unwrap();
        location
            .place_unit(Unit::new(31, UnitType::Soldier), Coord::new(0, 0))
            .unwrap();
        location
            .place_unit(Unit::new(33, UnitType::Village), Coord::new(-1, 1))
            .unwrap();
        location
            .place_unit(Unit::new(32, UnitType::Tower), Coord::new(0, -1))
            .unwrap();
        location
            .place_unit(Unit::new(34, UnitType::Village), Coord::new(1, -1))
            .unwrap();

        let res = validate_location(&location);

        assert_eq!(res, Ok(()));
    }

    #[test]
    fn validate_location_no_capital() {
        let map = test_map([Water, Water, Land, Land, Land, Water, Land]);

        let mut coords_one = HashSet::default();
        coords_one.insert(Coord::new(-1, 1));
        coords_one.insert(Coord::new(0, 0));
        let region_one = Region::new(11, Player::new(21), coords_one);

        let mut coords_two = HashSet::default();
        coords_two.insert(Coord::new(1, -1));
        coords_two.insert(Coord::new(0, -1));
        let region_two = Region::new(12, Player::new(22), coords_two);
        let mut location = Location::new(map, vec![region_one, region_two]).unwrap();
        location
            .place_unit(Unit::new(31, UnitType::Soldier), Coord::new(0, 0))
            .unwrap();
        location
            .place_unit(Unit::new(32, UnitType::Tower), Coord::new(0, -1))
            .unwrap();
        location
            .place_unit(Unit::new(34, UnitType::Village), Coord::new(1, -1))
            .unwrap();

        let res = validate_location(&location);

        assert_eq!(
            res,
            Err(LocationRulesValidationError::ActiveRegionWithoutCapital(11))
        );
    }

    #[test]
    fn validate_location_two_capitals() {
        let map = test_map([Water, Water, Land, Land, Land, Water, Land]);

        let mut coords_one = HashSet::default();
        coords_one.insert(Coord::new(-1, 1));
        coords_one.insert(Coord::new(0, 0));
        let region_one = Region::new(11, Player::new(21), coords_one);

        let mut coords_two = HashSet::default();
        coords_two.insert(Coord::new(1, -1));
        coords_two.insert(Coord::new(0, -1));
        let region_two = Region::new(12, Player::new(22), coords_two);
        let mut location = Location::new(map, vec![region_one, region_two]).unwrap();
        location
            .place_unit(Unit::new(31, UnitType::Soldier), Coord::new(0, 0))
            .unwrap();
        location
            .place_unit(Unit::new(33, UnitType::Village), Coord::new(-1, 1))
            .unwrap();
        location
            .place_unit(Unit::new(32, UnitType::Village), Coord::new(0, -1))
            .unwrap();
        location
            .place_unit(Unit::new(34, UnitType::Village), Coord::new(1, -1))
            .unwrap();

        let res = validate_location(&location);

        assert_eq!(res, Err(LocationRulesValidationError::MultiplyCapitals(12)));
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
        let map = test_map([Land, Water, Land, Water, Land, Water, Land]);

        let mut coords_one = HashSet::default();
        coords_one.insert(Coord::new(-1, 1));
        coords_one.insert(Coord::new(0, 1));
        let region_one = Region::new(11, Player::new(21), coords_one);

        let mut coords_two = HashSet::default();
        coords_two.insert(Coord::new(1, -1));
        coords_two.insert(Coord::new(0, -1));
        let region_two = Region::new(12, Player::new(22), coords_two);
        let mut location = Location::new(map, vec![region_one, region_two]).unwrap();
        location
            .place_unit(Unit::new(31, UnitType::Soldier), Coord::new(0, 1))
            .unwrap();
        location
            .place_unit(Unit::new(32, UnitType::Tower), Coord::new(0, -1))
            .unwrap();

        let res = validate_location(&location);

        assert_eq!(res, Err(LocationRulesValidationError::UnconnectedLand));
    }

    #[test]
    fn validate_location_not_covered_with_region() {
        let map = test_map([Land, Water, Land, Land, Land, Water, Land]);

        let mut coords_one = HashSet::default();
        coords_one.insert(Coord::new(-1, 1));
        coords_one.insert(Coord::new(0, 1));
        let region_one = Region::new(11, Player::new(21), coords_one);

        let mut coords_two = HashSet::default();
        coords_two.insert(Coord::new(1, -1));
        coords_two.insert(Coord::new(0, -1));
        let region_two = Region::new(12, Player::new(22), coords_two);
        let mut location = Location::new(map, vec![region_one, region_two]).unwrap();
        location
            .place_unit(Unit::new(31, UnitType::Soldier), Coord::new(0, 1))
            .unwrap();
        location
            .place_unit(Unit::new(32, UnitType::Tower), Coord::new(0, -1))
            .unwrap();

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
        let map = test_map([Water, Water, Land, Land, Land, Water, Land]);

        let mut coords_one = HashSet::default();
        coords_one.insert(Coord::new(-1, 1));
        coords_one.insert(Coord::new(0, 0));
        let region_one = Region::new(11, Player::new(21), coords_one);

        let mut coords_two = HashSet::default();
        coords_two.insert(Coord::new(1, -1));
        coords_two.insert(Coord::new(0, -1));
        let region_two = Region::new(12, Player::new(22), coords_two);

        let mut location = Location::new(map, vec![region_one, region_two]).unwrap();
        location
            .place_unit(Unit::new(31, UnitType::Soldier), Coord::new(0, 1))
            .unwrap();
        location
            .place_unit(Unit::new(32, UnitType::Tower), Coord::new(0, -1))
            .unwrap();

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
        let map = test_map([Water, Water, Land, Land, Land, Water, Water]);

        let mut coords_one = HashSet::default();
        coords_one.insert(Coord::new(-1, 1));
        coords_one.insert(Coord::new(0, 0));
        let region_one = Region::new(11, Player::new(21), coords_one);

        let mut coords_two = HashSet::default();
        coords_two.insert(Coord::new(1, -1));
        let region_two = Region::new(12, Player::new(22), coords_two);

        let mut location = Location::new(map, vec![region_one, region_two]).unwrap();
        location
            .place_unit(Unit::new(31, UnitType::Soldier), Coord::new(0, 0))
            .unwrap();
        location
            .place_unit(Unit::new(32, UnitType::Tower), Coord::new(0, -1))
            .unwrap();

        let res = validate_location(&location);

        assert_eq!(res, Err(LocationRulesValidationError::InsufficientLand(42)));
    }

    #[test]
    fn validate_location_region_contains_water() {
        let map = test_map([Water, Water, Land, Land, Land, Water, Land]);

        let mut coords_one = HashSet::default();
        coords_one.insert(Coord::new(0, 1));
        coords_one.insert(Coord::new(-1, 1));
        coords_one.insert(Coord::new(0, 0));
        let region_one = Region::new(11, Player::new(21), coords_one);

        let mut coords_two = HashSet::default();
        coords_two.insert(Coord::new(1, -1));
        coords_two.insert(Coord::new(0, -1));
        let region_two = Region::new(12, Player::new(22), coords_two);
        let mut location = Location::new(map, vec![region_one, region_two]).unwrap();
        location
            .place_unit(Unit::new(31, UnitType::Soldier), Coord::new(0, 0))
            .unwrap();
        location
            .place_unit(Unit::new(32, UnitType::Tower), Coord::new(0, -1))
            .unwrap();
        let res = validate_location(&location);

        assert_eq!(
            res,
            Err(LocationRulesValidationError::RegionContainsWater(11))
        );
    }

    #[test]
    fn validate_region() {
        let map = test_map([Water, Water, Land, Land, Land, Water, Land]);

        let mut coords_one = HashSet::default();
        coords_one.insert(Coord::new(-1, 1));
        coords_one.insert(Coord::new(0, 0));
        let player_one = Player::new(21);
        let region_one = Region::new(11, player_one, coords_one);

        let mut coords_two = HashSet::default();
        coords_two.insert(Coord::new(1, -1));
        coords_two.insert(Coord::new(0, -1));
        let player_two = Player::new(22);
        let region_two = Region::new(12, player_two, coords_two);
        let location = Location::new(map, vec![region_one, region_two]).unwrap();

        let players = [player_one, player_two];
        let res = validate_regions(&location, &players);

        assert!(res.is_ok());
    }

    #[test]
    fn validate_regions_error_small_region() {
        let map = test_map([Water, Water, Land, Land, Land, Water, Land]);

        let mut coords_one = HashSet::default();
        coords_one.insert(Coord::new(-1, 1));
        coords_one.insert(Coord::new(0, 0));
        let player_one = Player::new(21);
        let region_one = Region::new(11, player_one, coords_one);

        let mut coords_two = HashSet::default();
        coords_two.insert(Coord::new(1, -1));
        let player_two = Player::new(22);
        let region_two = Region::new(12, player_two, coords_two);
        let location = Location::new(map, vec![region_one, region_two]).unwrap();

        let players = [player_one, player_two];
        let res = validate_regions(&location, &players);

        assert_eq!(
            res,
            Err(RegionsValidationError::NoActiveRegions(player_two.id()))
        );
    }

    #[test]
    fn validate_regions_error_unlisted_player() {
        let map = test_map([Water, Water, Land, Land, Land, Water, Land]);

        let mut coords_one = HashSet::default();
        coords_one.insert(Coord::new(-1, 1));
        coords_one.insert(Coord::new(0, 0));
        let player_one = Player::new(21);
        let region_one = Region::new(11, player_one, coords_one);

        let mut coords_two = HashSet::default();
        coords_two.insert(Coord::new(1, -1));
        coords_two.insert(Coord::new(0, -1));
        let player_two = Player::new(22);
        let region_two = Region::new(12, player_two, coords_two);
        let location = Location::new(map, vec![region_one, region_two]).unwrap();

        let players = [player_one];
        let res = validate_regions(&location, &players);

        assert_eq!(
            res,
            Err(RegionsValidationError::UnlistedPlayer(player_two.id()))
        );
    }

    #[test]
    fn validate_regions_error_no_region() {
        let map = test_map([Water, Water, Land, Land, Land, Water, Land]);

        let mut coords_one = HashSet::default();
        coords_one.insert(Coord::new(-1, 1));
        coords_one.insert(Coord::new(0, 0));
        let player_one = Player::new(21);
        let region_one = Region::new(11, player_one, coords_one);

        let mut coords_two = HashSet::default();
        coords_two.insert(Coord::new(1, -1));
        coords_two.insert(Coord::new(0, -1));
        let player_two = Player::new(22);
        let region_two = Region::new(12, player_two, coords_two);
        let location = Location::new(map, vec![region_one, region_two]).unwrap();

        let player_three = Player::new(23);
        let players = [player_one, player_two, player_three];
        let res = validate_regions(&location, &players);

        assert_eq!(
            res,
            Err(RegionsValidationError::NoActiveRegions(player_three.id()))
        );
    }
}
