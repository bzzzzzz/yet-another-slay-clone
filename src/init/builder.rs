use std::collections::HashMap;

use crate::game::{
    validate_location, validate_regions, Coord, EngineValidationError, GameEngine, IdProducer,
    Location, LocationRulesValidationError, Player, Region, Tile, TileSurface, ID,
};
use hex2d::Direction;

pub enum GameEngineBuilderError {
    CoordinateOutOfBounds(Coord),
}

#[derive(Debug)]
pub struct GameEngineBuilder {
    map: HashMap<Coord, Tile>,
    coodinate_to_owner: HashMap<Coord, ID>,
    id_producer: IdProducer,
    players: Vec<Player>,
    validation_state: Option<EngineValidationError>,
}

impl GameEngineBuilder {
    fn new(map: HashMap<Coord, Tile>, players: Vec<Player>, id_producer: IdProducer) -> Self {
        GameEngineBuilder {
            map,
            players,
            id_producer,
            coodinate_to_owner: HashMap::new(),
            validation_state: Some(EngineValidationError::LocationError(
                LocationRulesValidationError::NoLand,
            )),
        }
    }

    pub fn rectangle(width: u32, height: u32, players: Vec<Player>) -> Self {
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
        // TODO: create form
        Self::new(map, players, id_producer)
    }

    pub fn circle(radius: u32, players: Vec<Player>) -> Self {
        let mut map: HashMap<Coord, Tile> = HashMap::new();
        let mut id_producer = IdProducer::default();
        let start = Coord::new(0, 0);
        start.for_each_in_range(radius as i32, |c| {
            map.insert(c, Tile::new(id_producer.next_id(), TileSurface::Water));
        });
        Self::new(map, players, id_producer)
    }

    pub fn is_valid(&self) -> bool {
        self.validation_state.is_none()
    }

    fn validate(&self) -> Result<(), EngineValidationError> {
        unimplemented!()
    }

    fn revalidate(&mut self) {
        self.validation_state = self.validate().err();
    }

    fn regions(&self) -> Vec<Region> {
        unimplemented!()
    }

    pub fn set_surface(
        &mut self,
        coordinate: Coord,
        surface: TileSurface,
    ) -> Result<(), GameEngineBuilderError> {
        {
            let tile = self
                .map
                .get_mut(&coordinate)
                .ok_or_else(|| GameEngineBuilderError::CoordinateOutOfBounds(coordinate))?;
            if surface == TileSurface::Water && tile.unit().is_some() {
                tile.take_unit();
            }
            tile.set_surface(surface);
        }
        self.revalidate();

        Ok(())
    }

    pub fn set_owner(
        &mut self,
        coordinate: Coord,
        owner_id: ID,
    ) -> Result<(), GameEngineBuilderError> {
        if !self.map.contains_key(&coordinate) {
            return Err(GameEngineBuilderError::CoordinateOutOfBounds(coordinate));
        }
        self.coodinate_to_owner.insert(coordinate, owner_id);
        self.revalidate();

        Ok(())
    }

    pub fn build(self) -> Result<GameEngine, EngineValidationError> {
        if let Some(err) = self.validation_state {
            return Err(err);
        }
        let regions: Vec<Region> = self.regions();
        let location = Location::new(self.map, regions)?;

        GameEngine::new(location, self.players)
    }
}
