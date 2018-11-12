use std::collections::{HashMap, HashSet, VecDeque};

use hex2d::Coordinate;

use super::ids::{IdProducer, ID, NO_ID};

pub type Coord = Coordinate<i32>;

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Ord, PartialOrd)]
pub enum UnitType {
    Grave,
    PineTree,
    PalmTree,
    Village,
    Tower,
    GreatKnight,
    Knight,
    Soldier,
    Militia,
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Ord, PartialOrd)]
pub struct Unit {
    id: ID,
    unit_type: UnitType,
}

impl Unit {
    pub fn new(id: ID, unit_type: UnitType) -> Self {
        Self { id, unit_type }
    }

    pub fn id(self) -> ID {
        self.id
    }

    pub fn unit_type(self) -> UnitType {
        self.unit_type
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Ord, PartialOrd)]
pub enum TileSurface {
    Water,
    Land,
}

impl TileSurface {
    /// Returns true if surface is land
    ///
    /// # Examples
    ///
    /// ```
    /// use yasc::game::location::TileSurface;
    ///
    /// assert_eq!(TileSurface::Water.is_land(), false);
    /// assert_eq!(TileSurface::Land.is_land(), true);
    /// ```
    ///
    pub fn is_land(self) -> bool {
        self == TileSurface::Land
    }

    /// Returns true if surface is land
    ///
    /// # Examples
    ///
    /// ```
    /// use yasc::game::location::TileSurface;
    ///
    /// assert_eq!(TileSurface::Water.is_water(), true);
    /// assert_eq!(TileSurface::Land.is_water(), false);
    /// ```
    ///
    pub fn is_water(self) -> bool {
        self == TileSurface::Water
    }
}

/// This struct represents contents of one tile of the hexagonal map
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Ord, PartialOrd)]
pub struct Tile {
    id: ID,
    surface: TileSurface,
    unit: Option<Unit>,
}

impl Tile {
    pub fn new(id: ID, surface: TileSurface) -> Self {
        Self {
            id,
            surface,
            unit: None,
        }
    }

    pub fn id(&self) -> ID {
        self.id
    }

    pub fn surface(&self) -> &TileSurface {
        &self.surface
    }

    pub fn unit(&self) -> Option<&Unit> {
        self.unit.as_ref()
    }

    /// Remove unit from this tile and return it
    fn take_unit(&mut self) -> Option<Unit> {
        self.unit.take()
    }

    /// Place unit on this tile
    fn place_unit(&mut self, unit: Unit) {
        self.unit = Some(unit);
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Ord, PartialOrd)]
pub struct Player {
    id: ID,
}

impl Player {
    pub fn new(id: ID) -> Self {
        Self { id }
    }

    pub fn id(self) -> ID {
        self.id
    }
}

/// This represent some connected set of tiles on a hexagonal map. It should be always not empty and
/// always owned by somebody.
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct Region {
    id: ID,
    owner: Player,
    coordinates: HashSet<Coord>,
}

impl Region {
    pub fn new(id: ID, owner: Player, coordinates: HashSet<Coord>) -> Self {
        if coordinates.is_empty() {
            panic!("Coordinates should never be empty");
        }
        Self {
            id,
            owner,
            coordinates,
        }
    }

    pub fn id(&self) -> ID {
        self.id
    }

    pub fn owner(&self) -> &Player {
        &self.owner
    }

    pub fn coordinates(&self) -> &HashSet<Coord> {
        &self.coordinates
    }
}

#[derive(Clone, Eq, PartialEq, Hash, Debug, Ord, PartialOrd)]
pub enum RegionTransformation {
    Merge { from: ID, into: ID },
    Delete(ID),
    Split { from: ID, into: Vec<ID> },
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Ord, PartialOrd)]
pub enum LocationValidationError {
    DuplicateRegionId(ID),
    SplitRegions(ID),
    IntersectingRegions(Coord),
    SameOwnerBorderingRegions(ID, ID),
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Ord, PartialOrd)]
pub enum LocationModificationError {
    CoordinateOutOfLocation(Coord),
    NoUnitAtCoordinate(Coord),
    CoordinateNotAdjacentToRegion(Coord),
    NoSuchRegion(ID),
    InvalidResult(LocationValidationError),
}

#[derive(Eq, PartialEq, Debug)]
pub struct Location {
    map: HashMap<Coord, Tile>,
    regions: HashMap<ID, Region>,
    coordinate_to_region: HashMap<Coord, ID>,
}

impl From<LocationValidationError> for LocationModificationError {
    fn from(e: LocationValidationError) -> Self {
        LocationModificationError::InvalidResult(e)
    }
}

impl Location {
    /// Create new location represented by specified map and regions.
    /// Return error if resulting location is not valid. See validness description in `validate`
    /// method docs
    pub fn new(
        map: HashMap<Coord, Tile>,
        regions_vec: Vec<Region>,
    ) -> Result<Self, LocationValidationError> {
        let mut coordinate_to_region = HashMap::default();
        let mut regions = HashMap::default();
        for region in regions_vec.into_iter() {
            if regions.contains_key(&region.id) {
                return Err(LocationValidationError::DuplicateRegionId(region.id));
            }

            for &coordinate in region.coordinates.iter() {
                coordinate_to_region.insert(coordinate, region.id);
            }
            regions.insert(region.id, region);
        }

        let location = Self {
            map,
            regions,
            coordinate_to_region,
        };
        Self::validate(&location)?;

        Ok(location)
    }

    /// Validate if location provided does not contain any errors. This method only ensures there
    /// are no general error, but does not check if location is ok by game rules.
    /// Returns nothing is everything is ok and `LocationInitiationError` if there were error
    /// found:
    ///
    /// - SplitRegions means that there is at least one region that contains of separate parts.
    ///   This parts does not have any common borders. Id of region with problem is provided
    /// - IntersectingRegions means that there are two regions that share the same coordinate
    pub fn validate(location: &Self) -> Result<(), LocationValidationError> {
        // Check if there are intersecting regions or empty regions
        let mut already_processed: HashSet<Coord> = HashSet::default();
        for (_, region) in location.regions.iter() {
            for &coordinate in region.coordinates.iter() {
                if already_processed.contains(&coordinate) {
                    return Err(LocationValidationError::IntersectingRegions(coordinate));
                }
                already_processed.insert(coordinate);
            }
        }

        // Check if there are regions with unconnected land
        for (_, region) in location.regions.iter() {
            if let Some(c) = region.coordinates.iter().next() {
                let result = location.bfs_set(*c, |c| region.coordinates.contains(&c));
                let wrong = region.coordinates.iter().find(|c| !result.contains(c));
                if wrong.is_some() {
                    return Err(LocationValidationError::SplitRegions(region.id));
                }
            }
        }

        // Check if there are no regions of the same owner sharing the border
        let (&start, _) = location.map.iter().next().unwrap();
        for (_, coord) in location.bfs_iter(start, |_| true) {
            let region = location.region_at(coord);
            if region.is_none() {
                continue;
            }
            let region = region.unwrap();

            let neighbours = coord.neighbors();
            for neighbour in neighbours.iter() {
                let n_region = location.region_at(*neighbour);
                if n_region.is_none() {
                    continue;
                }
                let n_region = n_region.unwrap();

                if region.id != n_region.id && region.owner.id == n_region.owner.id {
                    // Just to make order predictable
                    let (i1, i2) = if region.id > n_region.id {
                        (n_region.id, region.id)
                    } else {
                        (region.id, n_region.id)
                    };
                    return Err(LocationValidationError::SameOwnerBorderingRegions(i1, i2));
                }
            }
        }

        // Return ok because no errors were found
        Ok(())
    }

    pub fn map(&self) -> &HashMap<Coord, Tile> {
        &self.map
    }

    pub fn regions(&self) -> &HashMap<u32, Region> {
        &self.regions
    }

    pub fn region_at(&self, coordinate: Coord) -> Option<&Region> {
        self.coordinate_to_region
            .get(&coordinate)
            .and_then(|id| self.regions.get(id))
    }

    pub fn tile_at(&self, coordinate: Coord) -> Option<&Tile> {
        self.map.get(&coordinate)
    }

    /// Removes a unit from tile with provided coordinate
    ///
    /// Will return `LocationModificationError::CoordinateOutOfLocation` if coordinate is out of
    /// location borders
    ///
    /// If this method returns any kind of error, no changes to locations were made
    pub fn remove_unit(
        &mut self,
        coordinate: Coord,
    ) -> Result<Option<Unit>, LocationModificationError> {
        let unit = self
            .map
            .get_mut(&coordinate)
            .ok_or_else(|| LocationModificationError::CoordinateOutOfLocation(coordinate))?
            .take_unit();
        Ok(unit)
    }

    /// Places a provided unit on a tile with specified coordinate. If that tile already has
    /// unit on it, it will be replaced
    ///
    /// Will return `LocationModificationError::CoordinateOutOfLocation` if coordinate is out of
    /// location borders
    ///
    /// If this method returns any kind of error, no changes to locations were made
    pub fn place_unit(&mut self, unit: Unit, dst: Coord) -> Result<(), LocationModificationError> {
        self.map
            .get_mut(&dst)
            .ok_or_else(|| LocationModificationError::CoordinateOutOfLocation(dst))?
            .place_unit(unit);
        Ok(())
    }

    /// Move a unit from one tile to another. If another tile already has unit on it, it will be replaced
    ///
    /// Will return `LocationModificationError::CoordinateOutOfLocation` if one of coordinates is
    /// out of location borders.
    /// Will return `LocationModificationError::NoUnitAtCoordinate` is there is no unit to move
    ///
    /// If this method returns any kind of error, no changes to locations were made
    pub fn move_unit(&mut self, from: Coord, to: Coord) -> Result<(), LocationModificationError> {
        // Check if destination exists before performing changes
        if !self.map.contains_key(&to) {
            return Err(LocationModificationError::CoordinateOutOfLocation(to));
        }

        let unit = self
            .map
            .get_mut(&from)
            .ok_or_else(|| LocationModificationError::CoordinateOutOfLocation(from))?
            .take_unit()
            .ok_or_else(|| LocationModificationError::NoUnitAtCoordinate(from))?;

        self.place_unit(unit, to)
    }

    /// Add a tile with specified coordinate to a region with specified ID. This method expects
    /// coordinate to be adjacent to a region, otherwise it will return error containing
    /// `LocationModificationError::CoordinateNotAdjacentToRegion`.
    /// If adding a tile to a region makes this region to border with other regions of the same
    /// owner, those regions become one, with the ID provided to this method.
    /// If coordinate was a part of other region, it is removed from an old region. If removing tile
    /// from old region makes it separated, it is split into several regions.
    ///
    /// This method can return error with `LocationModificationError::NoSuchRegion` if there is no
    /// region with provided ID, or `LocationModificationError::CoordinateOutOfLocation` if
    /// coordinate is not inside the location bounds.
    ///
    /// If this method returns any kind of error, no changes to locations were made.
    /// If everything went ok, this method will return a list of changes made into regions structure.
    /// This includes only merging, splitting or deleting a region
    pub fn add_tile_to_region(
        &mut self,
        coordinate: Coord,
        region_id: ID,
        id_producer: &mut IdProducer,
    ) -> Result<Vec<RegionTransformation>, LocationModificationError> {
        let (old_region_id, merge_ids) =
            self.validate_and_prepare_add_tile(coordinate, region_id)?;

        let mut performed_actions = Vec::new();

        // Then we need to remove coordinate from old region
        // If region was split into parts by this action, we need to create new regions for those
        // parts
        if old_region_id != NO_ID {
            self.remove_coordinate_from_region(old_region_id, coordinate);
            if let Some(action) = self.maybe_remove_region(old_region_id) {
                performed_actions.push(action);
            }
            if let Some(action) = self.maybe_split_region(old_region_id, id_producer) {
                performed_actions.push(action);
            }
        }
        // Then we can insert coordinate into new region
        self.add_coordinate_to_region(region_id, coordinate);

        // Finally, we need to check if region can be merged with another region of the same player
        // If regions have common border - they should be merged
        for id in merge_ids.iter() {
            performed_actions.push(RegionTransformation::Merge {
                from: *id,
                into: region_id,
            });
        }
        self.merge_regions(merge_ids, region_id);

        Location::validate(self).expect("Adding region never should make location invalid");

        Ok(performed_actions)
    }

    fn validate_and_prepare_add_tile(
        &self,
        coordinate: Coord,
        region_id: ID,
    ) -> Result<(ID, HashSet<ID>), LocationModificationError> {
        // First we check if everything is ok with coordinates
        if !self.map.contains_key(&coordinate) {
            return Err(LocationModificationError::CoordinateOutOfLocation(
                coordinate,
            ));
        }

        let neighbours = coordinate.neighbors();
        let region = self
            .regions
            .get(&region_id)
            .ok_or_else(|| LocationModificationError::NoSuchRegion(region_id))?;

        if region.coordinates.contains(&coordinate) {
            return Err(LocationModificationError::CoordinateNotAdjacentToRegion(
                coordinate,
            ));
        }

        let region_neighbour = neighbours.iter().find(|c| region.coordinates.contains(c));
        if region_neighbour.is_none() {
            return Err(LocationModificationError::CoordinateNotAdjacentToRegion(
                coordinate,
            ));
        }
        let old_region_id = *self.coordinate_to_region.get(&coordinate).unwrap_or(&NO_ID);

        let merge_ids: HashSet<ID> = neighbours
            .iter()
            .filter_map(|c| self.region_at(*c))
            .filter(|r| region_id != r.id)
            .filter(|r| region.owner.id == r.owner.id)
            .map(|r| r.id)
            .collect();

        Ok((old_region_id, merge_ids))
    }

    /// Merge region with `src_ids` into region with `dst_id`.
    /// This will panic if IDs are bad.
    fn merge_regions(&mut self, src_ids: HashSet<ID>, dst_id: ID) {
        if src_ids.is_empty() {
            return;
        }

        for src_id in src_ids.into_iter() {
            let region = self.regions.remove(&src_id).unwrap();
            for coordinate in region.coordinates.into_iter() {
                self.add_coordinate_to_region(dst_id, coordinate);
            }
        }
    }

    fn add_coordinate_to_region(&mut self, region_id: ID, coordinate: Coord) {
        self.regions
            .get_mut(&region_id)
            .expect("Region ID should be verified before providing them")
            .coordinates
            .insert(coordinate);
        self.coordinate_to_region.insert(coordinate, region_id);
    }

    fn remove_coordinate_from_region(&mut self, region_id: ID, coordinate: Coord) {
        self.regions
            .get_mut(&region_id)
            .expect("Region ID should be verified before providing them")
            .coordinates
            .remove(&coordinate);
        self.coordinate_to_region.remove(&coordinate);
    }

    /// Remove region with provided ID if region is empty
    fn maybe_remove_region(&mut self, region_id: ID) -> Option<RegionTransformation> {
        if self.regions[&region_id].coordinates.is_empty() {
            self.regions.remove(&region_id);

            Some(RegionTransformation::Delete(region_id))
        } else {
            None
        }
    }

    /// Split region into part regions if it has became unconnected
    fn maybe_split_region(
        &mut self,
        region_id: ID,
        id_producer: &mut IdProducer,
    ) -> Option<RegionTransformation> {
        if !self.regions.contains_key(&region_id) {
            return None;
        }

        let mut results = Vec::new();
        results.push(region_id);

        let owner_id = self.regions[&region_id].owner.id;
        while let Some(coordinates) = self.region_part_to_remove(region_id) {
            let new_id = id_producer.next();
            results.push(new_id);

            for coordinate in coordinates.iter() {
                self.remove_coordinate_from_region(region_id, *coordinate);
                self.coordinate_to_region.insert(*coordinate, new_id);
            }
            self.regions.insert(
                new_id,
                Region::new(new_id, Player::new(owner_id), coordinates),
            );
        }
        if results.len() <= 1 {
            None
        } else {
            Some(RegionTransformation::Split {
                from: region_id,
                into: results,
            })
        }
    }

    /// Return a set with coordinates of regions that can be removed from region because they are
    /// not connected to other region. If there are no such parts, return None
    fn region_part_to_remove(&self, region_id: ID) -> Option<HashSet<Coord>> {
        let region = &self.regions[&region_id];
        let start = *region.coordinates.iter().next().unwrap();
        let coords = self.bfs_set(start, |c| {
            self.coordinate_to_region.contains_key(&c)
                && self.coordinate_to_region[&c].eq(&region_id)
        });

        if coords.eq(&region.coordinates) {
            None
        } else {
            Some(coords)
        }
    }

    /// Perform a BFS on the location, starting from provided coordinate. Return a vector
    /// containing all coordinates that matched a predicate.
    ///
    /// This method will return empty vec if starting coordinate is out of location or does
    /// not match the predicate.
    pub fn bfs_all<P>(&self, coordinate: Coord, predicate: P) -> Vec<Coord>
    where
        P: Fn(Coord) -> bool,
    {
        self.bfs_iter(coordinate, predicate)
            .map(|(_, c)| c)
            .collect()
    }

    /// Perform a BFS on the location, starting from provided coordinate. Return a set
    /// containing all coordinates that matched a predicate.
    ///
    /// This method will return empty set if starting coordinate is out of location or does
    /// not match the predicate.
    pub fn bfs_set<P>(&self, coordinate: Coord, predicate: P) -> HashSet<Coord>
    where
        P: Fn(Coord) -> bool,
    {
        self.bfs_iter(coordinate, predicate)
            .map(|(_, c)| c)
            .collect()
    }

    /// Return an iterator that performs a BFS on the location, starting from provided coordinate.
    ///
    /// This method will return empty iterator if starting coordinate is out of location or does
    /// not match the predicate.
    pub fn bfs_iter<P>(&self, coordinate: Coord, predicate: P) -> BfsIter<P>
    where
        P: Fn(Coord) -> bool,
    {
        BfsIter::new(&self, coordinate, predicate)
    }

    /// Perform BFS to return shortest distance between two coordinates using only coordinates that
    /// match the predicate.
    /// Returns `None` if there is no path between coordinates
    pub fn bfs_distance<P>(&self, from: Coord, to: Coord, predicate: P) -> Option<u32>
    where
        P: Fn(Coord) -> bool,
    {
        self.bfs_iter(from, predicate)
            .find(|(_, coord)| *coord == to)
            .map(|(dist, _)| dist)
    }
}

pub struct BfsIter<'a, P> {
    processed: HashSet<Coord>,
    queue: VecDeque<(u32, Coord)>,
    predicate: P,
    location: &'a Location,
}

impl<'a, P> BfsIter<'a, P>
where
    P: Fn(Coord) -> bool,
{
    fn new(location: &'a Location, start_coordinate: Coord, predicate: P) -> BfsIter<P> {
        let mut processed = HashSet::default();
        let mut queue = VecDeque::new();

        if predicate(start_coordinate) && location.tile_at(start_coordinate).is_some() {
            queue.push_back((0, start_coordinate));
            processed.insert(start_coordinate);
        }
        Self {
            processed,
            queue,
            location,
            predicate,
        }
    }

    fn process_and_return(&mut self, distance: u32, coordinate: Coord) -> (u32, Coord) {
        for neighbor in coordinate.neighbors().iter() {
            if !self.processed.contains(neighbor)
                && self.location.tile_at(*neighbor).is_some()
                && (self.predicate)(*neighbor)
            {
                self.queue.push_back((distance + 1, *neighbor));
            }
            self.processed.insert(*neighbor);
        }
        (distance, coordinate)
    }
}

impl<'a, P> Iterator for BfsIter<'a, P>
where
    P: Fn(Coord) -> bool,
{
    type Item = (u32, Coord);

    fn next(&mut self) -> Option<(u32, Coord)> {
        self.queue
            .pop_front()
            .map(|(step, coordinate)| self.process_and_return(step, coordinate))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, Some(self.location.map.len()))
    }
}

#[cfg(test)]
mod test {
    use std::collections::{HashMap, HashSet};

    use super::TileSurface::*;
    use super::{
        Coord, Location, LocationModificationError, LocationValidationError, Player, Region,
        RegionTransformation, Tile, TileSurface, Unit, UnitType,
    };
    use game::ids::IdProducer;

    #[test]
    fn tile_place_unit() {
        let unit = Unit::new(1, UnitType::Soldier);
        let mut tile = Tile::new(1, TileSurface::Land);
        tile.place_unit(unit.clone());
        assert_eq!(tile.unit(), Some(&unit));

        // Unit will be replaced with new one
        let other_unit = Unit::new(1, UnitType::Militia);
        tile.place_unit(other_unit.clone());
        assert_eq!(tile.unit(), Some(&other_unit));
    }

    #[test]
    fn tile_take_unit() {
        let unit = Unit::new(1, UnitType::Soldier);
        let mut tile = Tile::new(1, TileSurface::Land);
        tile.place_unit(unit.clone());

        let taken_unit = tile.take_unit();
        assert_eq!(taken_unit, Some(unit));
        assert_eq!(tile.unit(), None);
    }

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
    fn correct_init() {
        let map = test_map([Water, Water, Water, Water, Water, Water, Water]);
        let location = Location::new(map, Vec::new());
        assert!(location.is_ok());
        assert_eq!(location.unwrap().map().len(), 7);
    }

    #[test]
    fn correct_init_has_valid_regions() {
        let map = test_map([Water, Land, Water, Land, Water, Land, Water]);

        let mut coords_one = HashSet::default();
        coords_one.insert(Coord::new(0, 1));
        coords_one.insert(Coord::new(1, 0));
        coords_one.insert(Coord::new(-1, 1));
        let region_one = Region::new(11, Player::new(21), coords_one);

        let mut coords_two = HashSet::default();
        coords_two.insert(Coord::new(-1, 0));
        coords_two.insert(Coord::new(0, -1));
        let region_two = Region::new(12, Player::new(22), coords_two);
        let location = Location::new(map, vec![region_one, region_two]);
        assert!(location.is_ok());
    }

    #[test]
    fn error_init_has_duplicate_id_regions() {
        let map = test_map([Water, Land, Water, Land, Water, Land, Water]);

        let mut coords_one = HashSet::default();
        coords_one.insert(Coord::new(0, 1));
        let region_one = Region::new(11, Player::new(21), coords_one);

        let mut coords_two = HashSet::default();
        coords_two.insert(Coord::new(-1, 0));
        let region_two = Region::new(11, Player::new(22), coords_two);
        let location = Location::new(map, vec![region_one, region_two]);
        assert!(location.is_err());
        assert_eq!(
            location.unwrap_err(),
            LocationValidationError::DuplicateRegionId(11)
        )
    }

    #[test]
    fn error_init_has_intersecting_regions() {
        let map = test_map([Water, Land, Water, Land, Water, Land, Water]);

        let mut coords_one = HashSet::default();
        coords_one.insert(Coord::new(0, 1));
        coords_one.insert(Coord::new(1, 0));
        coords_one.insert(Coord::new(-1, 1));
        let region_one = Region::new(11, Player::new(21), coords_one);

        let mut coords_two = HashSet::default();
        coords_two.insert(Coord::new(-1, 1));
        coords_two.insert(Coord::new(-1, 0));
        coords_two.insert(Coord::new(0, -1));
        let region_two = Region::new(12, Player::new(22), coords_two);
        let location = Location::new(map, vec![region_one, region_two]);
        assert!(location.is_err());
        assert_eq!(
            location.unwrap_err(),
            LocationValidationError::IntersectingRegions(Coord::new(-1, 1))
        )
    }

    #[test]
    fn error_init_has_split_regions() {
        let map = test_map([Water, Land, Water, Land, Water, Land, Water]);

        let mut coords_one = HashSet::default();
        coords_one.insert(Coord::new(0, 1));
        coords_one.insert(Coord::new(1, 0));
        let region_one = Region::new(11, Player::new(21), coords_one);

        let mut coords_two = HashSet::default();
        coords_two.insert(Coord::new(-1, 1));
        coords_two.insert(Coord::new(0, -1));
        let region_two = Region::new(12, Player::new(22), coords_two);
        let location = Location::new(map, vec![region_one, region_two]);
        assert!(location.is_err());
        assert_eq!(
            location.unwrap_err(),
            LocationValidationError::SplitRegions(12)
        )
    }

    #[test]
    fn error_init_has_bordering_regions_of_same_owner() {
        let map = test_map([Water, Water, Land, Land, Land, Water, Land]);

        let player_id = 21;
        let mut coords_one = HashSet::default();
        coords_one.insert(Coord::new(-1, 1));
        coords_one.insert(Coord::new(0, 0));
        let region_one = Region::new(11, Player::new(player_id), coords_one);

        let mut coords_two = HashSet::default();
        coords_two.insert(Coord::new(1, -1));
        coords_two.insert(Coord::new(0, -1));
        let region_two = Region::new(12, Player::new(player_id), coords_two);
        let location = Location::new(map, vec![region_one, region_two]);
        assert!(location.is_err());
        assert_eq!(
            location.unwrap_err(),
            LocationValidationError::SameOwnerBorderingRegions(11, 12)
        )
    }

    fn create_valid_location() -> Location {
        let map = test_map([Water, Land, Land, Land, Land, Land, Water]);

        let mut coords_one = HashSet::default();
        coords_one.insert(Coord::new(0, 1));
        coords_one.insert(Coord::new(1, 0));
        let region_one = Region::new(11, Player::new(21), coords_one);

        let mut coords_two = HashSet::default();
        coords_two.insert(Coord::new(-1, 1));
        let region_two = Region::new(12, Player::new(22), coords_two);

        let mut coords_three = HashSet::default();
        coords_three.insert(Coord::new(0, 0));
        coords_three.insert(Coord::new(1, -1));
        let region_three = Region::new(13, Player::new(23), coords_three);

        let mut coords_four = HashSet::default();
        coords_four.insert(Coord::new(-1, 0));
        coords_four.insert(Coord::new(0, -1));
        let region_four = Region::new(14, Player::new(21), coords_four);
        let location = Location::new(map, vec![region_one, region_two, region_three, region_four]);
        assert!(location.is_ok());

        location.unwrap()
    }

    #[test]
    fn location_remove_unit_correct() {
        let mut location = create_valid_location();
        let c = Coord::new(-1, 1);

        assert_eq!(location.tile_at(c).unwrap().unit(), None);
        let unit = Unit::new(22, UnitType::Grave);
        location.place_unit(unit.clone(), c).unwrap();

        assert_eq!(location.tile_at(c).unwrap().unit(), Some(&unit));

        location.remove_unit(c).unwrap();
        assert_eq!(location.tile_at(c).unwrap().unit(), None);
    }

    #[test]
    fn location_remove_unit_error_out_of_border() {
        let mut location = create_valid_location();
        let c = Coord::new(-2, 1);
        let res = location.remove_unit(c);

        assert_eq!(
            res,
            Err(LocationModificationError::CoordinateOutOfLocation(c))
        );
        assert!(location.tile_at(c).is_none());
    }

    #[test]
    fn location_place_unit_correct() {
        let mut location = create_valid_location();
        let c = Coord::new(-1, 1);

        assert_eq!(location.tile_at(c).unwrap().unit(), None);
        let unit = Unit::new(22, UnitType::Grave);
        location.place_unit(unit.clone(), c).unwrap();

        assert_eq!(location.tile_at(c).unwrap().unit(), Some(&unit));
    }

    #[test]
    fn location_place_unit_error_out_of_border() {
        let mut location = create_valid_location();
        let c = Coord::new(-2, 1);

        let unit = Unit::new(22, UnitType::Grave);
        let res = location.place_unit(unit.clone(), c);

        assert_eq!(
            res,
            Err(LocationModificationError::CoordinateOutOfLocation(c))
        );
        assert!(location.tile_at(c).is_none());
    }

    #[test]
    fn location_move_unit_correct() {
        let mut location = create_valid_location();
        let src = Coord::new(-1, 1);
        let dst = Coord::new(1, -1);
        let unit = Unit::new(22, UnitType::Grave);
        location.place_unit(unit.clone(), src).unwrap();

        assert_eq!(location.tile_at(src).unwrap().unit(), Some(&unit));
        assert_eq!(location.tile_at(dst).unwrap().unit(), None);

        location.move_unit(src, dst).unwrap();

        assert_eq!(location.tile_at(src).unwrap().unit(), None);
        assert_eq!(location.tile_at(dst).unwrap().unit(), Some(&unit));
    }

    #[test]
    fn location_move_unit_error_no_dst() {
        let mut location = create_valid_location();
        let src = Coord::new(-1, 1);
        let dst = Coord::new(2, -1);
        let unit = Unit::new(22, UnitType::Grave);
        location.place_unit(unit.clone(), src).unwrap();

        assert_eq!(location.tile_at(src).unwrap().unit(), Some(&unit));
        assert_eq!(location.tile_at(dst), None);

        let res = location.move_unit(src, dst);

        assert_eq!(
            res,
            Err(LocationModificationError::CoordinateOutOfLocation(dst))
        );
        assert_eq!(location.tile_at(src).unwrap().unit(), Some(&unit));
        assert_eq!(location.tile_at(dst), None);
    }

    #[test]
    fn location_move_unit_error_no_src() {
        let mut location = create_valid_location();
        let src = Coord::new(-1, 3);
        let dst = Coord::new(1, -1);

        assert_eq!(location.tile_at(src), None);

        let res = location.move_unit(src, dst);

        assert_eq!(
            res,
            Err(LocationModificationError::CoordinateOutOfLocation(src))
        );
        assert_eq!(location.tile_at(src), None);
    }

    #[test]
    fn location_move_unit_error_no_unit() {
        let mut location = create_valid_location();
        let src = Coord::new(-1, 1);
        let dst = Coord::new(1, -1);

        assert_eq!(location.tile_at(src).unwrap().unit(), None);
        assert_eq!(location.tile_at(dst).unwrap().unit(), None);

        let res = location.move_unit(src, dst);

        assert_eq!(res, Err(LocationModificationError::NoUnitAtCoordinate(src)));
        assert_eq!(location.tile_at(src).unwrap().unit(), None);
        assert_eq!(location.tile_at(dst).unwrap().unit(), None);
    }

    #[test]
    fn location_coord_to_region_correct_basic() {
        let mut location = create_valid_location();
        let mut id_producer = IdProducer::default();
        let c = Coord::new(0, 0);
        let actions = location
            .add_tile_to_region(c, 12, &mut id_producer)
            .unwrap();

        assert_eq!(actions.len(), 0);

        let region = &location.regions[&12];
        assert_eq!(region.coordinates.len(), 2);
        assert!(region.coordinates.contains(&c));
        assert!(region.coordinates.contains(&Coord::new(-1, 1)));

        let region = &location.regions[&13];
        assert_eq!(region.coordinates.len(), 1);
        assert!(region.coordinates.contains(&Coord::new(1, -1)));
    }

    #[test]
    fn location_coord_to_region_correct_remove() {
        let mut location = create_valid_location();
        let mut id_producer = IdProducer::default();
        let c = Coord::new(-1, 1);
        let actions = location
            .add_tile_to_region(c, 13, &mut id_producer)
            .unwrap();

        assert_eq!(actions, vec!(RegionTransformation::Delete(12)));
        // This region should be deleted when processing
        assert!(!location.regions.contains_key(&12));

        let region = &location.regions[&13];
        assert_eq!(region.coordinates.len(), 3);
        assert!(region.coordinates.contains(&c));
        assert!(region.coordinates.contains(&Coord::new(0, 0)));
        assert!(region.coordinates.contains(&Coord::new(1, -1)));
    }

    #[test]
    fn location_coord_to_region_correct_merge_and_remove() {
        let mut location = create_valid_location();
        let mut id_producer = IdProducer::default();
        let c = Coord::new(-1, 1);
        let actions = location
            .add_tile_to_region(c, 11, &mut id_producer)
            .unwrap();

        assert_eq!(
            actions,
            vec!(
                RegionTransformation::Delete(12),
                RegionTransformation::Merge { from: 14, into: 11 }
            )
        );
        // This regions should be deleted when processing
        assert!(!location.regions.contains_key(&12));
        assert!(!location.regions.contains_key(&14));

        let region = &location.regions[&11];
        assert_eq!(region.coordinates.len(), 5);
        assert!(region.coordinates.contains(&c));
        assert!(region.coordinates.contains(&Coord::new(0, 1)));
        assert!(region.coordinates.contains(&Coord::new(1, 0)));
        assert!(region.coordinates.contains(&Coord::new(0, -1)));
        assert!(region.coordinates.contains(&Coord::new(-1, 0)));
    }

    #[test]
    fn location_coord_to_region_correct_split() {
        let mut location = create_valid_location();
        let mut id_producer = IdProducer::default();
        let actions_one = location
            .add_tile_to_region(Coord::new(-1, 1), 13, &mut id_producer)
            .unwrap();
        let actions_two = location
            .add_tile_to_region(Coord::new(0, 0), 11, &mut id_producer)
            .unwrap();

        assert_eq!(actions_one, vec!(RegionTransformation::Delete(12),));
        assert_eq!(
            actions_two,
            vec!(
                RegionTransformation::Split {
                    from: 13,
                    into: vec!(13, 1)
                },
                RegionTransformation::Merge { from: 14, into: 11 }
            )
        );

        // This regions should be deleted when processing
        assert!(!location.regions.contains_key(&12));
        assert!(!location.regions.contains_key(&14));

        // This one should merge from 14
        let region = &location.regions[&11];
        assert_eq!(region.coordinates.len(), 5);
        assert!(region.coordinates.contains(&Coord::new(0, 0)));
        assert!(region.coordinates.contains(&Coord::new(0, 1)));
        assert!(region.coordinates.contains(&Coord::new(1, 0)));
        assert!(region.coordinates.contains(&Coord::new(0, -1)));
        assert!(region.coordinates.contains(&Coord::new(-1, 0)));

        // Other two regions should be split
        let region = &location.regions[&13];
        assert_eq!(region.coordinates.len(), 1);

        let region = &location.regions[&1];
        assert_eq!(region.coordinates.len(), 1);
    }

    #[test]
    fn location_coord_to_region_error_out_of_border() {
        let mut location = create_valid_location();
        let mut id_producer = IdProducer::default();
        let c = Coord::new(1, 1);
        let res = location.add_tile_to_region(c, 11, &mut id_producer);

        assert_eq!(
            res,
            Err(LocationModificationError::CoordinateOutOfLocation(c))
        );
        assert!(!location.regions()[&11].coordinates().contains(&c));
    }

    #[test]
    fn location_coord_to_region_error_no_region() {
        let mut location = create_valid_location();
        let mut id_producer = IdProducer::default();
        let c = Coord::new(-1, 0);
        let region = 19;
        let res = location.add_tile_to_region(c, region, &mut id_producer);

        assert_eq!(res, Err(LocationModificationError::NoSuchRegion(region)));
        assert_ne!(location.region_at(c).unwrap().id(), region);
        assert!(!location.regions().contains_key(&region));
    }

    #[test]
    fn location_coord_to_region_error_region_far_from_coord() {
        let mut location = create_valid_location();
        let mut id_producer = IdProducer::default();
        let c = Coord::new(1, -1);
        let region = 12;
        let res = location.add_tile_to_region(c, region, &mut id_producer);

        assert_eq!(
            res,
            Err(LocationModificationError::CoordinateNotAdjacentToRegion(c))
        );
        assert_ne!(location.region_at(c).unwrap().id(), region);
        assert!(!location.regions()[&region].coordinates().contains(&c));
    }

    #[test]
    fn location_coord_to_region_error_region_already_contains_coord() {
        let mut location = create_valid_location();
        let mut id_producer = IdProducer::default();
        let c = Coord::new(-1, 1);
        let region = 12;
        let res = location.add_tile_to_region(c, region, &mut id_producer);

        assert_eq!(
            res,
            Err(LocationModificationError::CoordinateNotAdjacentToRegion(c))
        );
        assert!(location.regions()[&region].coordinates().contains(&c));
    }

    #[test]
    fn bfs_returns_everything() {
        let map = test_map([Water, Land, Water, Land, Water, Land, Water]);
        let location = Location::new(map, Vec::new()).unwrap();
        let coords = location.bfs_all(Coord::new(0, 1), |_| true);
        assert_eq!(coords.len(), location.map().len());
        for (c, _) in location.map().iter() {
            assert!(coords.contains(c));
        }
    }

    #[test]
    fn bfs_returns_filtered() {
        let map = test_map([Water, Land, Water, Land, Water, Land, Water]);
        let location = Location::new(map, Vec::new()).unwrap();
        let coords = location.bfs_all(Coord::new(0, 1), |c| {
            location
                .tile_at(c)
                .map_or(false, |t| t.surface().is_water())
        });
        assert_eq!(coords.len(), 2);
        for c in vec![Coord::new(0, 1), Coord::new(-1, 1)].iter() {
            assert!(coords.contains(c));
        }
    }

    #[test]
    fn bfs_returns_nothing_coord_out_of_location() {
        let map = test_map([Water, Land, Water, Land, Water, Land, Water]);
        let location = Location::new(map, Vec::new()).unwrap();
        let coords = location.bfs_all(Coord::new(2, 1), |_| true);
        assert!(coords.is_empty());
    }

    #[test]
    fn bfs_returns_nothing_start_coord_fails_predicate() {
        let map = test_map([Water, Land, Water, Land, Water, Land, Water]);
        let location = Location::new(map, Vec::new()).unwrap();
        let coords = location.bfs_all(Coord::new(0, 1), |c| {
            location.tile_at(c).map_or(false, |t| t.surface().is_land())
        });
        assert!(coords.is_empty());
    }

    #[test]
    fn bfs_distance_returns_correct_some() {
        let map = test_map([Land, Land, Water, Land, Land, Land, Water]);
        let location = Location::new(map, Vec::new()).unwrap();
        let distance = location.bfs_distance(Coord::new(-1, 0), Coord::new(0, 1), |c| {
            location.tile_at(c).map_or(false, |t| t.surface().is_land())
        });
        assert_eq!(distance, Some(2));
    }

    #[test]
    fn bfs_distance_returns_correct_to_itself() {
        let map = test_map([Land, Land, Water, Land, Land, Land, Water]);
        let location = Location::new(map, Vec::new()).unwrap();
        let distance = location.bfs_distance(Coord::new(-1, 0), Coord::new(-1, 0), |c| {
            location.tile_at(c).map_or(false, |t| t.surface().is_land())
        });
        assert_eq!(distance, Some(0));
    }

    #[test]
    fn bfs_distance_returns_correct_no_passage() {
        let map = test_map([Land, Land, Water, Water, Land, Land, Water]);
        let location = Location::new(map, Vec::new()).unwrap();
        let distance = location.bfs_distance(Coord::new(-1, 0), Coord::new(0, 1), |c| {
            location.tile_at(c).map_or(false, |t| t.surface().is_land())
        });
        assert_eq!(distance, None);
    }
}
