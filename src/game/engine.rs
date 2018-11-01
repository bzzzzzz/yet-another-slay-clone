use std::collections::HashMap;

use super::consts::*;
use super::ids::{IdProducer, ID};
use super::location::{Location, Player};
use super::rules::{validate_location, LocationRulesValidationError};

pub struct GameEngine {
    players: Vec<Player>,
    current_turn: u32,
    active_player_num: usize,
    region_money: HashMap<ID, i32>,
    location: Location,
    id_producer: IdProducer,
}

pub enum EngineValidationError {
    LocationError(LocationRulesValidationError),
}

impl From<LocationRulesValidationError> for EngineValidationError {
    fn from(e: LocationRulesValidationError) -> Self {
        EngineValidationError::LocationError(e)
    }
}

impl GameEngine {
    pub fn new(location: Location, players: Vec<Player>) -> Result<Self, EngineValidationError> {
        validate_location(&location)?;

        let mut region_money = HashMap::default();
        for (id, region) in location.regions().iter() {
            let money = if region.coordinates().len() > MIN_CONTROLLED_REGION_SIZE {
                CONTROLLED_REGION_STARTING_MONEY
            } else {
                0
            };
            region_money.insert(id.clone(), money);
        }
        Ok(Self {
            location,
            players,
            region_money,
            current_turn: 1,
            active_player_num: 0,
            id_producer: IdProducer::default(),
        })
    }

    pub fn players(&self) -> &Vec<Player> {
        &self.players
    }

    pub fn location(&self) -> &Location {
        &self.location
    }

    pub fn current_turn(&self) -> u32 {
        self.current_turn
    }

    pub fn active_player_num(&self) -> usize {
        self.active_player_num
    }

    pub fn active_player(&self) -> &Player {
        &self.players[self.active_player_num]
    }
}
