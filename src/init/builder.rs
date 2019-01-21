use std::collections::{HashMap, HashSet};

use crate::game::{
    Coord, EngineValidationError, GameEngine, IdProducer, Location, Player, Region, Tile,
    TileSurface, Unit, UnitType, ID,
};
use hex2d::Direction;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum GameEngineBuilderInitiationError {
    NotEnoughPlayers(u8),
    TooSmallMap,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum GameEngineBuilderModificationError {
    CoordinateOutOfBounds(Coord),
    CoordinateCannotBeOwned(Coord),
    NoSuchPlayer(ID),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameEngineBuilder {
    map: HashMap<Coord, Tile>,
    coodinate_to_owner: HashMap<Coord, ID>,
    id_producer: IdProducer,
    players: Vec<Player>,
    player_ids: HashSet<ID>,
}

impl GameEngineBuilder {
    fn new(
        map: HashMap<Coord, Tile>,
        players_num: u8,
        mut id_producer: IdProducer,
    ) -> Result<Self, GameEngineBuilderInitiationError> {
        if players_num <= 1 {
            return Err(GameEngineBuilderInitiationError::NotEnoughPlayers(2));
        }
        let players: Vec<Player> = (0..players_num)
            .map(|_| Player::new(id_producer.next_id()))
            .collect();
        let player_ids: HashSet<ID> = players.iter().map(|p| p.id()).collect();
        Ok(GameEngineBuilder {
            map,
            players,
            player_ids,
            id_producer,
            coodinate_to_owner: HashMap::new(),
        })
    }

    pub fn rectangle(
        width: u32,
        height: u32,
        players_num: u8,
    ) -> Result<Self, GameEngineBuilderInitiationError> {
        if width < 5 || height < 5 {
            return Err(GameEngineBuilderInitiationError::TooSmallMap);
        }
        let mut map: HashMap<Coord, Tile> = HashMap::new();
        let mut id_producer = IdProducer::default();
        let mut start = Coord::new(0, 0);
        for row in 0..height {
            start.for_each_in_line_to(Coord::new(start.x, start.y + width as i32 - 1), |c| {
                map.insert(c, Tile::new(id_producer.next_id(), TileSurface::Water));
            });
            let direction = if row % 2 == 0 {
                Direction::XY
            } else {
                Direction::XZ
            };
            start = start + direction;
        }
        Self::new(map, players_num, id_producer)
    }

    pub fn circle(radius: u32, players_num: u8) -> Result<Self, GameEngineBuilderInitiationError> {
        if radius < 3 {
            return Err(GameEngineBuilderInitiationError::TooSmallMap);
        }
        let mut map: HashMap<Coord, Tile> = HashMap::new();
        let mut id_producer = IdProducer::default();
        let start = Coord::new(0, 0);
        start.for_each_in_range(radius as i32, |c| {
            map.insert(c, Tile::new(id_producer.next_id(), TileSurface::Water));
        });
        Self::new(map, players_num, id_producer)
    }

    pub fn map(&self) -> &HashMap<Coord, Tile> {
        &self.map
    }

    pub fn players(&self) -> &Vec<Player> {
        &self.players
    }

    pub fn owners(&self) -> &HashMap<Coord, ID> {
        &self.coodinate_to_owner
    }

    pub fn set_surface(
        &mut self,
        coordinate: Coord,
        surface: TileSurface,
    ) -> Result<(), GameEngineBuilderModificationError> {
        let tile = self
            .map
            .get_mut(&coordinate)
            .ok_or_else(|| GameEngineBuilderModificationError::CoordinateOutOfBounds(coordinate))?;
        if surface == TileSurface::Water && tile.unit().is_some() {
            tile.take_unit();
        }
        self.coodinate_to_owner.remove(&coordinate);
        tile.set_surface(surface);

        Ok(())
    }

    pub fn set_owner(
        &mut self,
        coordinate: Coord,
        owner_id: ID,
    ) -> Result<(), GameEngineBuilderModificationError> {
        if !self.map.contains_key(&coordinate) {
            return Err(GameEngineBuilderModificationError::CoordinateOutOfBounds(
                coordinate,
            ));
        } else if !self.player_ids.contains(&owner_id) {
            return Err(GameEngineBuilderModificationError::NoSuchPlayer(owner_id));
        } else if !self.map[&coordinate].surface().is_land() {
            return Err(GameEngineBuilderModificationError::CoordinateCannotBeOwned(
                coordinate,
            ));
        }
        self.coodinate_to_owner.insert(coordinate, owner_id);

        Ok(())
    }

    fn build_regions(
        coordinate_to_owner: &HashMap<Coord, ID>,
        id_producer: &mut IdProducer,
    ) -> Vec<Region> {
        let mut coordinate_to_region: HashMap<Coord, ID> = HashMap::new();
        let mut regions: HashMap<ID, Region> = HashMap::new();
        for (&c, &owner_id) in coordinate_to_owner.iter() {
            let neighbours = c.neighbors();
            let same_owners: Vec<Coord> = neighbours
                .iter()
                .filter(|n| {
                    coordinate_to_region.contains_key(n) && coordinate_to_owner.contains_key(n)
                })
                .filter(|n| coordinate_to_owner[&n] == owner_id)
                .cloned()
                .collect();
            if same_owners.is_empty() {
                // No known neighbours of the same owner - we need to create new region
                let mut region_coordinates = HashSet::new();
                region_coordinates.insert(c);
                let region = Region::new(
                    id_producer.next_id(),
                    Player::new(owner_id),
                    region_coordinates,
                );
                coordinate_to_region.insert(c, region.id());
                regions.insert(region.id(), region);
            } else if same_owners.len() == 1 {
                // One neighbour with the same owner - just reuse it's region
                let region_id = coordinate_to_region[&same_owners[0]];
                let region = regions.get_mut(&region_id).unwrap();
                region.add(c);
                coordinate_to_region.insert(c, region_id);
            } else {
                let region_ids: HashSet<ID> = same_owners
                    .iter()
                    .filter_map(|so| coordinate_to_region.get(so))
                    .cloned()
                    .collect();
                if region_ids.len() == 1 {
                    let region_id = *region_ids.iter().next().unwrap();
                    let region = regions.get_mut(&region_id).unwrap();
                    region.add(c);
                    coordinate_to_region.insert(c, region_id);
                } else {
                    let region_id = *region_ids.iter().next().unwrap();
                    let mut region = regions.remove(&region_id).unwrap();
                    region.add(c);
                    coordinate_to_region.insert(c, region_id);

                    for r_id in region_ids.into_iter() {
                        if r_id == region_id {
                            continue;
                        }
                        let old_region = regions.remove(&r_id).unwrap();
                        for &c in old_region.coordinates().iter() {
                            coordinate_to_region.insert(c, region.id());
                            region.add(c)
                        }
                    }
                    regions.insert(region.id(), region);
                }
            }
        }

        regions.values().cloned().collect()
    }

    fn set_capitals(location: &mut Location, id_producer: &mut IdProducer) {
        let capitals_coordinates: Vec<Coord> = location
            .regions()
            .values()
            .map(|r| *r.coordinates().iter().next().unwrap())
            .collect();

        for coordinate in capitals_coordinates {
            location
                .place_unit(
                    Unit::new(id_producer.next_id(), UnitType::Village),
                    coordinate,
                )
                .unwrap();
        }
    }

    pub fn build(mut self) -> Result<GameEngine, EngineValidationError> {
        let regions: Vec<Region> =
            Self::build_regions(&self.coodinate_to_owner, &mut self.id_producer);
        let mut location = Location::new(self.map, regions)?;
        Self::set_capitals(&mut location, &mut self.id_producer);

        GameEngine::new(location, self.players, self.id_producer)
    }
}

#[cfg(test)]
mod test {
    use super::{
        GameEngineBuilder, GameEngineBuilderInitiationError, GameEngineBuilderModificationError,
    };
    use crate::game::{Coord, TileSurface};

    #[test]
    fn check_circle_creation_size_error() {
        let result = GameEngineBuilder::circle(1, 2);

        assert_eq!(result, Err(GameEngineBuilderInitiationError::TooSmallMap));
    }

    #[test]
    fn check_circle_creation_ok() {
        let result = GameEngineBuilder::circle(3, 2);
        assert!(result.is_ok());
    }

    #[test]
    fn check_rectangle_creation_size_error() {
        let result = GameEngineBuilder::rectangle(1, 10, 2);

        assert_eq!(result, Err(GameEngineBuilderInitiationError::TooSmallMap));
    }

    #[test]
    fn check_rectangle_creation_ok() {
        let result = GameEngineBuilder::rectangle(15, 10, 2);
        assert!(result.is_ok());
    }

    #[test]
    fn check_circle_build_ok() {
        let mut builder = GameEngineBuilder::circle(3, 2).unwrap();
        let one_id = builder.players()[0].id();
        let two_id = builder.players()[1].id();

        let start_coord = Coord::new(0, 0);
        start_coord.for_each_in_range(3, |c| {
            if c.y == 0 && c.x != 0 {
                return;
            }
            builder.set_surface(c, TileSurface::Land).unwrap();
            if c.y > 0 {
                builder.set_owner(c, one_id).unwrap();
            } else {
                builder.set_owner(c, two_id).unwrap();
            }
        });

        let result = builder.build();
        assert!(result.is_ok());
    }

    #[test]
    fn check_cannot_set_owner_for_water_tile() {
        let mut builder = GameEngineBuilder::circle(4, 2).unwrap();
        let player_id = builder.players()[0].id();
        let coord = Coord::new(0, 0);

        assert_eq!(
            builder.set_owner(coord, player_id),
            Err(GameEngineBuilderModificationError::CoordinateCannotBeOwned(
                coord
            ))
        );

        assert_eq!(builder.owners().get(&coord), None);
    }

    #[test]
    fn check_can_set_owner_for_land_tile() {
        let mut builder = GameEngineBuilder::circle(4, 2).unwrap();
        let player_id = builder.players()[0].id();
        let coord = Coord::new(0, 0);

        assert_eq!(builder.set_surface(coord, TileSurface::Land), Ok(()));
        assert_eq!(builder.set_owner(coord, player_id), Ok(()));

        assert_eq!(builder.owners().get(&coord), Some(&player_id));
    }

    #[test]
    fn check_removal_of_land_removes_ownership() {
        let mut builder = GameEngineBuilder::circle(4, 2).unwrap();
        let player_id = builder.players()[0].id();
        let coord = Coord::new(0, 0);

        assert_eq!(builder.set_surface(coord, TileSurface::Land), Ok(()));
        assert_eq!(builder.set_owner(coord, player_id), Ok(()));
        assert_eq!(builder.owners().get(&coord), Some(&player_id));

        assert_eq!(builder.set_surface(coord, TileSurface::Water), Ok(()));
        assert_eq!(builder.owners().get(&coord), None);
    }
}
