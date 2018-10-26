use super::ids::IdProducer;
use super::location::{Player,Location};


pub struct GameEngine {
    players: Vec<Player>,
    current_turn: u32,
    active_player_num: usize,
    location: Location,
    id_producer: IdProducer,
}


impl GameEngine {
    pub fn new(location: Location, players: Vec<Player>) -> Self {
        Self {location, players, current_turn: 1, active_player_num: 0, id_producer: IdProducer::new(),}
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
        self.players.get(self.active_player_num).unwrap()
    }
}
