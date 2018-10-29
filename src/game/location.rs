use std::collections::{HashMap, HashSet, VecDeque};
use std::iter::FromIterator;
use std::rc::Rc;

use hex2d::Coordinate;

use super::ids::ID;
use super::unit::Unit;

pub type Coord = Coordinate<i32>;

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
    pub fn is_land(&self) -> bool {
        *self == TileSurface::Land
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
    pub fn is_water(&self) -> bool {
        *self == TileSurface::Water
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

    pub fn unit(&self) -> &Option<Unit> {
        &self.unit
    }

    /// Remove unit from this tile and return it
    ///
    /// # Examples
    ///
    /// ```
    /// use yasc::game::location::{Tile,TileSurface};
    /// use yasc::game::unit::{Unit,UnitType};
    ///
    /// let unit = Unit::new(1, UnitType::Soldier);
    /// let mut tile = Tile::new(1, TileSurface::Land);
    /// tile.place_unit(unit.clone());
    ///
    /// let taken_unit = tile.take_unit();
    /// assert_eq!(taken_unit, Some(unit));
    /// assert_eq!(tile.unit(), &None);
    /// ```
    ///
    pub fn take_unit(&mut self) -> Option<Unit> {
        self.unit.take()
    }

    /// Place unit on this tile
    ///
    /// # Examples
    ///
    /// ```
    /// use yasc::game::location::{Tile,TileSurface};
    /// use yasc::game::unit::{Unit,UnitType};
    ///
    /// let unit = Unit::new(1, UnitType::Soldier);
    /// let mut tile = Tile::new(1, TileSurface::Land);
    /// tile.place_unit(unit.clone());
    /// assert_eq!(tile.unit(), &Some(unit));
    ///
    /// // Unit will be replaced with new one
    /// let other_unit = Unit::new(1, UnitType::Militia);
    /// tile.place_unit(other_unit.clone());
    /// assert_eq!(tile.unit(), &Some(other_unit));
    /// ```
    ///
    pub fn place_unit(&mut self, unit: Unit) {
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

    pub fn id(&self) -> ID {
        self.id
    }
}

/// This represent some connected set of tiles on a hexagonal map. It should be always not empty and
/// always owned by somebody.
#[derive(Debug)]
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

#[derive(Debug)]
pub struct Location {
    map: HashMap<Coord, Tile>,
    regions: HashMap<u32, Rc<Region>>,
    coordinate_to_region: HashMap<Coord, Rc<Region>>,
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Ord, PartialOrd)]
pub enum LocationInitiationError {
    SplitRegions(ID),
    IntersectingRegions(Coord),
}

impl Location {
    /// Create new location represented by specified map and regions.
    /// Return error if resulting location is not valid. See validness description in `validate`
    /// method docs
    pub fn new(
        map: HashMap<Coord, Tile>,
        regions_vec: Vec<Region>,
    ) -> Result<Self, LocationInitiationError> {
        let mut coordinate_to_region = HashMap::default();
        let mut regions = HashMap::default();
        for region in regions_vec.into_iter() {
            let region = Rc::new(region);
            regions.insert(region.id, Rc::clone(&region));
            for &coordinate in region.coordinates.iter() {
                coordinate_to_region.insert(coordinate, Rc::clone(&region));
            }
        }

        let location = Self {
            map,
            regions,
            coordinate_to_region,
        };
        match Self::validate(&location) {
            None => Ok(location),
            Some(e) => Err(e),
        }
    }

    /// Validate if location provided does not contain any errors. This method only ensures there
    /// are no general error, but does not check if location is ok by game rules.
    /// Returns `None` is everything is ok and `Some(LocationInitiationError)` if there were error
    /// found:
    ///
    /// - SplitRegions means that there is at least one region that contains of separate parts.
    ///   This parts does not have any common borders. Id of region with problem is provided
    /// - IntersectingRegions means that there are two regions that share the same coordinate
    pub fn validate(location: &Self) -> Option<LocationInitiationError> {
        // Check if there are intersecting regions
        let mut already_processed: HashSet<Coord> = HashSet::default();
        for (_, region) in location.regions.iter() {
            for &coordinate in region.coordinates.iter() {
                if already_processed.contains(&coordinate) {
                    return Some(LocationInitiationError::IntersectingRegions(coordinate));
                }
                already_processed.insert(coordinate);
            }
        }

        // Check if there are regions with unconnected land
        for (id, region) in location.regions.iter() {
            if let Some(c) = region.coordinates.iter().next() {
                let result = location.bfs_iter(c, |c| region.coordinates.contains(c));
                let result: HashSet<Coord> = HashSet::from_iter(result);
                let wrong = region.coordinates.iter().find(|c| !result.contains(c));
                if wrong.is_some() {
                    return Some(LocationInitiationError::SplitRegions(region.id));
                }
            }
        }

        // Return none because no errors were found
        None
    }

    pub fn map(&self) -> &HashMap<Coord, Tile> {
        &self.map
    }

    pub fn regions(&self) -> &HashMap<u32, Rc<Region>> {
        &self.regions
    }

    pub fn region_at(&self, coordinate: &Coord) -> Option<&Rc<Region>> {
        self.coordinate_to_region.get(&coordinate)
    }

    pub fn tile_at(&self, coordinate: &Coord) -> Option<&Tile> {
        self.map.get(&coordinate)
    }

    /// Perform a BFS on the location, starting from provided coordinate. Return a vector
    /// containing all coordinates that matched a predicate.
    ///
    /// This method will return empty vec if starting coordinate is out of location or does
    /// not match the predicate.
    pub fn bfs_all<P>(&self, coordinate: &Coord, predicate: P) -> Vec<Coord>
    where
        P: Fn(&Coord) -> bool,
    {
        self.bfs_iter(coordinate, predicate).collect()
    }

    /// Return an iterator that performs a BFS on the location, starting from provided coordinate.
    ///
    /// This method will return empty iterator if starting coordinate is out of location or does
    /// not match the predicate.
    pub fn bfs_iter<P>(&self, coordinate: &Coord, predicate: P) -> BfsIter<P>
    where
        P: Fn(&Coord) -> bool,
    {
        BfsIter::new(&self, coordinate.clone(), predicate)
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
    P: Fn(&Coord) -> bool,
{
    fn new(location: &'a Location, start_coordinate: Coord, predicate: P) -> BfsIter<P> {
        let mut processed = HashSet::default();
        let mut queue = VecDeque::new();

        if predicate(&start_coordinate) && location.tile_at(&start_coordinate).is_some() {
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

    fn process_and_return(&mut self, step: u32, coordinate: Coord) -> Coord {
        for neighbor in coordinate.neighbors().iter() {
            if !self.processed.contains(neighbor)
                && self.location.tile_at(neighbor).is_some()
                && (self.predicate)(neighbor)
            {
                self.queue.push_back((step + 1, neighbor.clone()));
            }
            self.processed.insert(neighbor.clone());
        }
        coordinate
    }
}

impl<'a, P> Iterator for BfsIter<'a, P>
where
    P: Fn(&Coord) -> bool,
{
    type Item = Coord;

    fn next(&mut self) -> Option<Coord> {
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
    use super::{Coord, Location, LocationInitiationError, Player, Region, Tile, TileSurface};

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
            LocationInitiationError::IntersectingRegions(Coord::new(-1, 1))
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
            LocationInitiationError::SplitRegions(12)
        )
    }

    #[test]
    fn bfs_returns_everything() {
        let map = test_map([Water, Land, Water, Land, Water, Land, Water]);
        let location = Location::new(map, Vec::new()).unwrap();
        let coords = location.bfs_all(&Coord::new(0, 1), |c| true);
        assert_eq!(coords.len(), location.map().len());
        for (c, _) in location.map().iter() {
            assert!(coords.contains(c));
        }
    }

    #[test]
    fn bfs_returns_filtered() {
        let map = test_map([Water, Land, Water, Land, Water, Land, Water]);
        let location = Location::new(map, Vec::new()).unwrap();
        let coords = location.bfs_all(&Coord::new(0, 1), |c| {
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
        let coords = location.bfs_all(&Coord::new(2, 1), |c| true);
        assert!(coords.is_empty());
    }

    #[test]
    fn bfs_returns_nothing_start_coord_fails_predicate() {
        let map = test_map([Water, Land, Water, Land, Water, Land, Water]);
        let location = Location::new(map, Vec::new()).unwrap();
        let coords = location.bfs_all(&Coord::new(0, 1), |c| {
            location.tile_at(c).map_or(false, |t| t.surface().is_land())
        });
        assert!(coords.is_empty());
    }
}
