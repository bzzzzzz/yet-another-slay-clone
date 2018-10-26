use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use hex2d::Coordinate;

use super::unit::Unit;


#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Ord, PartialOrd)]
pub enum TileSurface {
    Water, Ground,
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Ord, PartialOrd)]
pub struct Tile {
    id: u32,
    surface: TileSurface,
    unit: Option<Unit>,
}

impl Tile {
    pub fn new(id: u32, surface: TileSurface) -> Tile {
        Tile { id, surface, unit: None }
    }

    pub fn id(&self) -> u32 {
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
    /// let mut tile = Tile::new(1, TileSurface::Ground);
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
    /// let mut tile = Tile::new(1, TileSurface::Ground);
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
    id: u32,
}

impl Player {
    pub fn id(&self) -> u32 {
        self.id
    }
}

#[derive(Debug)]
pub struct Region {
    id: u32,
    owner: Player,
    money: i32,
    coordinates: HashSet<Coordinate<i32>>,
}

impl Region {
    pub fn id(&self) -> u32 {
        self.id
    }

    pub fn owner(&self) -> &Player {
        &self.owner
    }

    pub fn money(&self) -> i32 {
        self.money
    }

    pub fn coordinates(&self) -> &HashSet<Coordinate<i32>> {
        &self.coordinates
    }
}

#[derive(Debug)]
pub struct Location {
    map: HashMap<Coordinate<i32>, Tile>,
    regions: HashMap<u32, Rc<Region>>,
    coordinate_to_region: HashMap<Coordinate<i32>, Rc<Region>>,
}

impl Location {
    pub fn new(map: HashMap<Coordinate<i32>, Tile>, regions_vec: Vec<Region>) -> Location {
        let mut coordinate_to_region = HashMap::default();
        let mut regions = HashMap::default();
        for region in regions_vec.into_iter() {
            let region = Rc::new(region);
            regions.insert(region.id, Rc::clone(&region));
            for &coordinate in region.coordinates.iter() {
                coordinate_to_region.insert(coordinate, Rc::clone(&region));
            }
        }

        Location { map, regions, coordinate_to_region, }
    }

    pub fn map(&self) -> &HashMap<Coordinate<i32>, Tile> {
        &self.map
    }

    pub fn regions(&self) -> &HashMap<u32, Rc<Region>> {
        &self.regions
    }
}
