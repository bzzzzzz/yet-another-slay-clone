use std::collections::{HashMap, HashSet};

use super::consts::*;
use super::ids::{IdProducer, ID};
use super::location::{
    Coord, Location, LocationModificationError, Player, Region, RegionTransformation,
};
use super::rules::{
    validate_location, validate_regions, LocationRulesValidationError, RegionsValidationError,
};
use super::unit::{Unit, UnitType};

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Ord, PartialOrd)]
pub enum EngineValidationError {
    LocationError(LocationRulesValidationError),
    RegionsError(RegionsValidationError),
}

impl From<LocationRulesValidationError> for EngineValidationError {
    fn from(e: LocationRulesValidationError) -> Self {
        EngineValidationError::LocationError(e)
    }
}

impl From<RegionsValidationError> for EngineValidationError {
    fn from(e: RegionsValidationError) -> Self {
        EngineValidationError::RegionsError(e)
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Ord, PartialOrd)]
pub enum PlayerAction {
    PlaceNewUnit(UnitType, Coord),
    UpgradeUnit(Coord),
    MoveUnit(Coord, Coord),
    EndTurn,
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Ord, PartialOrd)]
pub enum PlayerActionError {
    OtherPlayersTurn(ID),
    LocationError(LocationModificationError),
    InaccessibleLocation(Coord),
    AlreadyOccupied(Coord),
    CannotAttack(Coord),
    NotEnoughMoney(ID),
    NotOwned(Coord),
    NoUnit(Coord),
    NoUpgrade(UnitType),
    GameAlreadyFinished,
}

impl From<LocationModificationError> for PlayerActionError {
    fn from(e: LocationModificationError) -> Self {
        PlayerActionError::LocationError(e)
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Ord, PartialOrd)]
struct RegionInfo {
    money_balance: i32,
    income_from_fields: i32,
    maintenance_cost: i32,
}

impl RegionInfo {
    fn new(money_balance: i32) -> Self {
        RegionInfo {
            money_balance,
            income_from_fields: 0,
            maintenance_cost: 0,
        }
    }

    fn can_afford(&self, sum: i32) -> bool {
        self.money_balance >= sum
    }

    fn change_balance(&mut self, diff: i32) {
        self.money_balance += diff;
    }

    fn recount(&mut self, region: &Region, location: &Location) {
        let mut new_income = 0;
        let mut new_maintenance = 0;
        for coordinate in region.coordinates().iter() {
            let tile = location.tile_at(*coordinate).unwrap();
            new_income += EMPTY_TILE_INCOME;
            if tile.unit().is_some() {
                new_maintenance += tile.unit().unwrap().description().turn_cost;
            }
        }
        self.income_from_fields = new_income;
        self.maintenance_cost = new_maintenance;
    }
}

pub struct GameEngine {
    players: Vec<Player>,
    player_activity: HashMap<ID, bool>,
    winner: Option<ID>,
    current_turn: u32,
    active_player_num: usize,
    region_money: HashMap<ID, RegionInfo>,
    location: Location,
    id_producer: IdProducer,
}

impl GameEngine {
    pub fn new(location: Location, players: Vec<Player>) -> Result<Self, EngineValidationError> {
        validate_location(&location)?;
        validate_regions(&location, players.as_slice())?;

        let mut region_money = HashMap::default();
        for (id, region) in location.regions().iter() {
            let money = if region.coordinates().len() > MIN_CONTROLLED_REGION_SIZE {
                RegionInfo::new(CONTROLLED_REGION_STARTING_MONEY)
            } else {
                RegionInfo::new(0)
            };
            region_money.insert(id.clone(), money);
        }
        let player_activity: HashMap<ID, bool> = players.iter().map(|p| (p.id(), true)).collect();
        let mut engine = Self {
            location,
            players,
            player_activity,
            region_money,
            winner: None,
            current_turn: 1,
            active_player_num: 0,
            id_producer: IdProducer::default(),
        };
        engine.recount_region_info();

        Ok(engine)
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

    pub fn region_money(&self, region_id: ID) -> Option<i32> {
        self.region_money.get(&region_id).map(|ri| ri.money_balance)
    }

    pub fn active_player_num(&self) -> usize {
        self.active_player_num
    }

    pub fn active_player(&self) -> &Player {
        &self.players[self.active_player_num]
    }

    pub fn act(&mut self, player_id: ID, action: PlayerAction) -> Result<(), PlayerActionError> {
        self.validate_action(player_id, &action)?;

        match action {
            PlayerAction::MoveUnit(src, dst) => self.move_unit(player_id, src, dst)?,
            PlayerAction::PlaceNewUnit(unit, dst) => self.place_new_unit(player_id, unit, dst)?,
            PlayerAction::UpgradeUnit(dst) => self.upgrade_unit(player_id, dst)?,
            PlayerAction::EndTurn => self.end_players_turn(),
        }

        self.recount_region_info();

        Ok(())
    }

    fn recount_region_info(&mut self) {
        for (id, region) in self.location.regions() {
            let mut info = self.region_money[id];
            info.recount(region, &self.location);
        }
    }

    fn modify_money(&mut self, region_id: ID, amount: i32) {
        let ri = self.region_money.get_mut(&region_id).unwrap();
        ri.change_balance(amount);
    }

    fn region_at(&self, coordinate: Coord) -> Result<&Region, PlayerActionError> {
        self.location
            .region_at(coordinate)
            // According to game rules region can be only on land, so this also checks if we're
            // trying to place unit on water. We will need to change that if rules change
            .ok_or_else(|| PlayerActionError::InaccessibleLocation(coordinate))
    }

    fn prepare_placing_unit(
        &self,
        player_id: ID,
        unit: &Unit,
        dst: Coord,
    ) -> Result<(ID, bool), PlayerActionError> {
        let dst_region = self.region_at(dst)?;
        let mut paying_region_id = dst_region.id();

        let need_relocation = if dst_region.owner().id() != player_id {
            let neighbours = dst.neighbors();
            let regions_to_check: Vec<&Region> = neighbours
                .iter()
                .filter(|&n| self.location.tile_at(*n).is_some())
                .filter_map(|&n| self.location.region_at(n))
                .filter(|r| r.owner().id() == player_id)
                .filter(|r| self.region_money[&r.id()].can_afford(unit.description().purchase_cost))
                .collect();
            if regions_to_check.is_empty() {
                return Err(PlayerActionError::CannotAttack(dst));
            }
            paying_region_id = regions_to_check[0].id();

            true
        } else {
            false
        };

        let tile = self.location.tile_at(dst).unwrap();
        if tile.unit().is_some() {
            // We cannot replace unit of the same owner
            if dst_region.owner().id() == player_id {
                return Err(PlayerActionError::AlreadyOccupied(dst));
            }
            // But we can replace other player's unit if we defeat it
            if tile.unit().unwrap().description().defence >= unit.description().attack {
                return Err(PlayerActionError::CannotAttack(dst));
            }
        }

        Ok((paying_region_id, need_relocation))
    }

    fn prepare_buying_unit(
        &self,
        player_id: ID,
        unit: &Unit,
        dst: Coord,
    ) -> Result<(ID, bool), PlayerActionError> {
        let (paying_region_id, need_relocation) =
            self.prepare_placing_unit(player_id, unit, dst)?;

        let region_info = self.region_money[&paying_region_id];
        if !region_info.can_afford(unit.description().purchase_cost) {
            return Err(PlayerActionError::NotEnoughMoney(paying_region_id));
        }

        Ok((paying_region_id, need_relocation))
    }

    fn place_new_unit(
        &mut self,
        player_id: ID,
        unit_type: UnitType,
        dst: Coord,
    ) -> Result<(), PlayerActionError> {
        let unit = Unit::new(self.id_producer.next(), unit_type);
        let (region_id, need_relocation) = self.prepare_buying_unit(player_id, &unit, dst)?;

        if need_relocation {
            self.add_tile_to_region(dst, region_id)?;
        }
        self.location.place_unit(unit, dst)?;

        self.modify_money(region_id, 0 - unit.description().purchase_cost);

        Ok(())
    }

    fn add_tile_to_region(
        &mut self,
        coordinate: Coord,
        region_id: ID,
    ) -> Result<(), PlayerActionError> {
        // We need to handle region changes after it.
        let res = self
            .location
            .add_tile_to_region(coordinate, region_id, &mut self.id_producer)?;
        for change in res.iter() {
            match change {
                RegionTransformation::Delete(id) => {
                    self.region_money.remove(&id);
                }
                RegionTransformation::Merge { from, into } => self.merge_regions(*from, *into),
                RegionTransformation::Split { from, into } => {
                    self.split_region(*from, into.clone())
                }
            }
        }

        Ok(())
    }

    fn merge_regions(&mut self, from: ID, into: ID) {
        let src = self.region_money.remove(&from).unwrap();
        let dst = self.region_money.get_mut(&into).unwrap();
        dst.money_balance += src.money_balance;
        dst.maintenance_cost += src.maintenance_cost;
        dst.income_from_fields += src.income_from_fields;
    }

    fn split_region(&mut self, from: ID, into: Vec<ID>) {
        let src = self.region_money.remove(&from).unwrap();
        let mut insert = Vec::new();
        let mut new_money_owners = Vec::new();
        for region_id in into.into_iter() {
            let region = &self.location.regions()[&region_id];
            if region.coordinates().len() < MIN_CONTROLLED_REGION_SIZE {
                insert.push((region_id, RegionInfo::new(0)));
            } else {
                new_money_owners.push(region_id);
            }
        }
        if !new_money_owners.is_empty() {
            let sum = src.money_balance;
            let part = sum / new_money_owners.len() as i32;
            let mut rest = sum - (new_money_owners.len() as i32 * part);
            for &id in new_money_owners.iter() {
                let info_part = if rest > 0 { part + 1 } else { part };
                rest -= 1;
                let info = RegionInfo::new(info_part);
                self.region_money.insert(id, info);
            }
        }
        for (id, info) in insert.into_iter() {
            self.region_money.insert(id, info);
        }
    }

    fn prepare_moving_unit(
        &self,
        player_id: ID,
        src: Coord,
        dst: Coord,
    ) -> Result<(ID, bool), PlayerActionError> {
        let unit = self
            .location
            .tile_at(src)
            .ok_or_else(|| PlayerActionError::InaccessibleLocation(src))?
            .unit();
        if unit.is_none() {
            return Err(PlayerActionError::NoUnit(dst));
        }

        self.prepare_placing_unit(player_id, unit.unwrap(), dst)
    }

    fn move_unit(
        &mut self,
        player_id: ID,
        src: Coord,
        dst: Coord,
    ) -> Result<(), PlayerActionError> {
        let (region_id, need_relocation) = self.prepare_moving_unit(player_id, src, dst)?;

        if need_relocation {
            self.add_tile_to_region(dst, region_id)?;
        }
        self.location.move_unit(src, dst)?;

        Ok(())
    }

    fn prepare_upgrading_unit(
        &self,
        player_id: ID,
        dst: Coord,
    ) -> Result<(ID, i32, UnitType), PlayerActionError> {
        let region = self.region_at(dst)?;
        if region.owner().id() != player_id {
            return Err(PlayerActionError::NotOwned(dst));
        }
        let old_unit = self.location.tile_at(dst).unwrap().unit();
        if old_unit.is_none() {
            return Err(PlayerActionError::NoUnit(dst));
        }
        let old_unit = old_unit.unwrap();

        let new_unit_description = old_unit.description().upgrades_to;
        if new_unit_description.is_none() {
            return Err(PlayerActionError::NoUpgrade(old_unit.description().name));
        }
        let new_unit_description = new_unit_description.unwrap();

        let sum = new_unit_description.purchase_cost - old_unit.description().purchase_cost;

        let region_info = self.region_money[&region.id()];
        if !region_info.can_afford(sum) {
            return Err(PlayerActionError::NotEnoughMoney(region.id()));
        }
        Ok((region.id(), sum, new_unit_description.name))
    }

    fn upgrade_unit(&mut self, player_id: ID, dst: Coord) -> Result<(), PlayerActionError> {
        let (region_id, sum, upgraded_unit_type) = self.prepare_upgrading_unit(player_id, dst)?;

        let upgraded_unit = Unit::new(self.id_producer.next(), upgraded_unit_type);
        self.location.place_unit(upgraded_unit, dst)?;
        self.modify_money(region_id, 0 - sum);

        Ok(())
    }

    fn check_for_winner(&mut self) {
        // Winner is the last player standing
        let active_players: Vec<ID> = self
            .players
            .iter()
            .filter(|p| self.player_activity[&p.id()])
            .map(|p| p.id())
            .collect();
        if active_players.len() == 1 {
            self.winner = Some(active_players[0]);
            return;
        }

        // TODO: add win condition: player, owning more than 65% of territory
    }

    fn validate_action(
        &self,
        player_id: u32,
        _action: &PlayerAction,
    ) -> Result<(), PlayerActionError> {
        if player_id != self.active_player().id() {
            return Err(PlayerActionError::OtherPlayersTurn(
                self.active_player().id(),
            ));
        } else if self.winner.is_some() {
            return Err(PlayerActionError::GameAlreadyFinished);
        }

        Ok(())
    }

    fn end_players_turn(&mut self) {
        self.active_player_num += 1;
        self.rewind_to_active_player();
        if self.active_player_num as usize >= self.players.len() {
            self.end_turn();
        }
    }

    fn rewind_to_active_player(&mut self) {
        while (self.active_player_num as usize) < self.players.len()
            && !self.player_activity[&self.active_player().id()]
        {
            self.active_player_num += 1;
        }
    }

    fn remove_graves(&mut self) {
        let mut to_delete = Vec::new();
        for (&coord, tile) in self.location.map().iter() {
            if tile
                .unit()
                .map_or(false, |u| u.description().name == UnitType::Grave)
            {
                to_delete.push(coord);
            }
        }
        for coordinate in to_delete.into_iter() {
            self.location.remove_unit(coordinate).unwrap();
        }
    }

    fn apply_income(&mut self) {
        for info in self.region_money.values_mut() {
            let sum = info.income_from_fields - info.maintenance_cost;
            info.change_balance(sum);
        }
    }

    fn kill_starving_units(&mut self) {
        let regions_to_check: Vec<ID> = self
            .region_money
            .iter()
            .filter(|(_, r)| r.money_balance < 0)
            .map(|(id, _)| *id)
            .collect();
        let kill_coordinates: Vec<Coord> = regions_to_check
            .iter()
            .filter_map(|id| self.location.regions().get(id))
            .flat_map(Region::coordinates)
            .filter(|&c| self.location.tile_at(*c).unwrap().unit().is_some())
            .cloned()
            .collect();
        for coordinate in kill_coordinates.into_iter() {
            let unit = Unit::new(self.id_producer.next(), UnitType::Grave);
            self.location.place_unit(unit, coordinate).unwrap();
        }
    }

    fn spread_forests(&mut self) {
        let coordinates_to_maybe_add_forest: HashSet<Coord> = self
            .location
            .map()
            .iter()
            .filter(|(_, t)| {
                t.unit().is_some() && t.unit().unwrap().description().name == UnitType::Tree
            }).flat_map(|(&c, _)| {
                let n = c.neighbors();
                let mut m = Vec::new();
                m.extend(n.iter());
                m.into_iter()
            }).collect();
        let coordinates_to_add_forest: HashSet<Coord> = coordinates_to_maybe_add_forest
            .into_iter()
            .filter(|c| self.location.tile_at(*c).is_some())
            .filter(|c| self.location.tile_at(*c).unwrap().surface().is_land())
            .filter(|c| self.location.tile_at(*c).unwrap().unit().is_none())
            .collect();
        for coordinate in coordinates_to_add_forest.into_iter() {
            let unit = Unit::new(self.id_producer.next(), UnitType::Tree);
            self.location.place_unit(unit, coordinate).unwrap()
        }
    }

    fn end_turn(&mut self) {
        // Set of end-of-turn actions. Order is important.
        self.remove_graves();
        self.apply_income();
        self.kill_starving_units();
        self.spread_forests();
        self.check_for_winner();

        // Now we can change turn number and find next active player to move
        self.current_turn += 1;
        self.active_player_num = 0;
        self.rewind_to_active_player();
    }
}

#[cfg(test)]
mod test {
    use std::collections::HashMap;

    use super::{GameEngine, PlayerAction, PlayerActionError};
    use game::consts::*;
    use game::ids::IdProducer;
    use game::location::TileSurface::*;
    use game::location::{Coord, Location, Player, Region, Tile};
    use game::unit::{description, Unit, UnitType};

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
    fn create_valid_engine() -> (Player, Player, GameEngine) {
        let mut id_producer = IdProducer::default();
        let mut map = HashMap::default();
        map.insert(Coord::new(0, -1), Tile::new(id_producer.next(), Land));
        map.insert(Coord::new(1, -1), Tile::new(id_producer.next(), Land));
        map.insert(Coord::new(2, -1), Tile::new(id_producer.next(), Land));
        map.insert(Coord::new(3, -1), Tile::new(id_producer.next(), Water));
        map.insert(Coord::new(-1, 0), Tile::new(id_producer.next(), Land));
        map.insert(Coord::new(0, 0), Tile::new(id_producer.next(), Water));
        map.insert(Coord::new(1, 0), Tile::new(id_producer.next(), Land));
        map.insert(Coord::new(2, 0), Tile::new(id_producer.next(), Land));
        map.insert(Coord::new(-2, 1), Tile::new(id_producer.next(), Land));
        map.insert(Coord::new(-1, 1), Tile::new(id_producer.next(), Land));
        map.insert(Coord::new(0, 1), Tile::new(id_producer.next(), Land));
        map.insert(Coord::new(1, 1), Tile::new(id_producer.next(), Land));

        let player_one = Player::new(id_producer.next());
        let player_two = Player::new(id_producer.next());
        let player_three = Player::new(id_producer.next());

        let coords = [
            Coord::new(0, -1),
            Coord::new(1, -1),
            Coord::new(2, -1),
            Coord::new(1, 0),
        ]
            .iter()
            .cloned()
            .collect();
        let region_one = Region::new(id_producer.next(), player_one, coords);

        let coords = [Coord::new(2, 0), Coord::new(1, 1), Coord::new(0, 1)]
            .iter()
            .cloned()
            .collect();
        let region_two = Region::new(id_producer.next(), player_two, coords);

        let coords = [Coord::new(-1, 1)].iter().cloned().collect();
        let region_three = Region::new(id_producer.next(), player_one, coords);

        let coords = [Coord::new(-1, 0), Coord::new(-2, 1)]
            .iter()
            .cloned()
            .collect();
        let region_four = Region::new(id_producer.next(), player_three, coords);

        let mut location =
            Location::new(map, vec![region_one, region_two, region_three, region_four]).unwrap();
        location
            .place_unit(
                Unit::new(id_producer.next(), UnitType::Village),
                Coord::new(1, -1),
            ).unwrap();
        location
            .place_unit(
                Unit::new(id_producer.next(), UnitType::Soldier),
                Coord::new(1, 0),
            ).unwrap();
        location
            .place_unit(
                Unit::new(id_producer.next(), UnitType::Militia),
                Coord::new(0, 1),
            ).unwrap();
        location
            .place_unit(
                Unit::new(id_producer.next(), UnitType::Village),
                Coord::new(2, 0),
            ).unwrap();
        location
            .place_unit(
                Unit::new(id_producer.next(), UnitType::Village),
                Coord::new(-1, 0),
            ).unwrap();

        let game_engine = GameEngine::new(location, vec![player_one, player_two]).unwrap();

        (player_one, player_two, game_engine)
    }

    #[test]
    fn create_engine_correct() {
        let (p1, _, game_engine) = create_valid_engine();

        let location = game_engine.location();
        let r1 = location.region_at(Coord::new(0, -1)).unwrap();
        let r2 = location.region_at(Coord::new(2, 0)).unwrap();
        let r3 = location.region_at(Coord::new(-1, 1)).unwrap();

        assert_eq!(*game_engine.active_player(), p1);
        assert_eq!(game_engine.current_turn(), 1);

        assert_eq!(
            game_engine.region_money(r1.id()),
            Some(CONTROLLED_REGION_STARTING_MONEY)
        );
        assert_eq!(
            game_engine.region_money(r2.id()),
            Some(CONTROLLED_REGION_STARTING_MONEY)
        );
        assert_eq!(game_engine.region_money(r3.id()), Some(0));
    }

    #[test]
    fn place_new_unit_simple_ok() {
        let (p1, _, mut game_engine) = create_valid_engine();
        let coordinate = Coord::new(2, -1);

        let action = PlayerAction::PlaceNewUnit(UnitType::Militia, coordinate);
        let res = game_engine.act(p1.id(), action);

        let region = game_engine.location().region_at(coordinate).unwrap();
        assert!(res.is_ok());
        assert_eq!(
            game_engine.region_money(region.id()),
            Some(CONTROLLED_REGION_STARTING_MONEY - description(UnitType::Militia).purchase_cost)
        );

        let unit = game_engine
            .location()
            .tile_at(coordinate)
            .unwrap()
            .unit()
            .unwrap();
        assert_eq!(unit.description().name, UnitType::Militia);
        assert_eq!(unit.moves_left(), 0);
    }

    #[test]
    fn place_new_unit_simple_no_money() {
        let (p1, _, mut game_engine) = create_valid_engine();
        let coordinate = Coord::new(2, -1);

        let action = PlayerAction::PlaceNewUnit(UnitType::Knight, coordinate);
        let res = game_engine.act(p1.id(), action);

        let region = game_engine.location().region_at(coordinate).unwrap();
        assert_eq!(res, Err(PlayerActionError::NotEnoughMoney(region.id())));
        assert_eq!(
            game_engine.region_money(region.id()),
            Some(CONTROLLED_REGION_STARTING_MONEY)
        );
        assert_eq!(
            game_engine.location().tile_at(coordinate).unwrap().unit(),
            None
        )
    }

    #[test]
    fn place_new_unit_simple_tile_out_of_border() {
        let (p1, _, mut game_engine) = create_valid_engine();
        let coordinate = Coord::new(-1, -1);

        let action = PlayerAction::PlaceNewUnit(UnitType::Militia, coordinate);
        let res = game_engine.act(p1.id(), action);

        assert_eq!(
            res,
            Err(PlayerActionError::InaccessibleLocation(coordinate))
        );
    }

    #[test]
    fn place_new_unit_simple_tile_bad_surface() {
        let (p1, _, mut game_engine) = create_valid_engine();
        let coordinate = Coord::new(0, 0);

        let action = PlayerAction::PlaceNewUnit(UnitType::Militia, coordinate);
        let res = game_engine.act(p1.id(), action);

        assert_eq!(
            res,
            Err(PlayerActionError::InaccessibleLocation(coordinate))
        );
    }

    #[test]
    fn place_new_unit_simple_tile_has_other_unit() {
        let (p1, _, mut game_engine) = create_valid_engine();
        let coordinate = Coord::new(1, 0);

        let action = PlayerAction::PlaceNewUnit(UnitType::Militia, coordinate);
        let res = game_engine.act(p1.id(), action);

        let region = game_engine.location().region_at(coordinate).unwrap();
        assert_eq!(res, Err(PlayerActionError::AlreadyOccupied(coordinate)));
        assert_eq!(
            game_engine.region_money(region.id()),
            Some(CONTROLLED_REGION_STARTING_MONEY)
        );
    }

    #[test]
    fn place_new_unit_simple_others_players_turn() {
        let (p1, p2, mut game_engine) = create_valid_engine();
        game_engine.winner = Some(p1.id());

        let coordinate = Coord::new(1, 1);
        let action = PlayerAction::PlaceNewUnit(UnitType::Knight, coordinate);
        let res = game_engine.act(p2.id(), action);

        let region = game_engine.location().region_at(coordinate).unwrap();
        assert_eq!(res, Err(PlayerActionError::OtherPlayersTurn(p1.id())));
        assert_eq!(
            game_engine.region_money(region.id()),
            Some(CONTROLLED_REGION_STARTING_MONEY)
        );
    }

    #[test]
    fn place_new_unit_simple_game_finished() {
        let (p1, _, mut game_engine) = create_valid_engine();
        game_engine.winner = Some(p1.id());

        let coordinate = Coord::new(0, -1);
        let action = PlayerAction::PlaceNewUnit(UnitType::Knight, coordinate);
        let res = game_engine.act(p1.id(), action);

        let region = game_engine.location().region_at(coordinate).unwrap();
        assert_eq!(res, Err(PlayerActionError::GameAlreadyFinished));
        assert_eq!(
            game_engine.region_money(region.id()),
            Some(CONTROLLED_REGION_STARTING_MONEY)
        );
    }

    #[test]
    fn place_new_unit_with_attack_empty_tile_all_ok() {
        let (p1, p2, mut game_engine) = create_valid_engine();
        game_engine.act(p1.id(), PlayerAction::EndTurn).unwrap();

        let coordinate = Coord::new(-1, 1);
        let old_goal_region_id = game_engine.location().region_at(coordinate).unwrap().id();

        let action = PlayerAction::PlaceNewUnit(UnitType::Militia, coordinate);
        let res = game_engine.act(p2.id(), action);

        assert_eq!(res, Ok(()));

        let region_for_purchase = game_engine.location().region_at(Coord::new(0, 1)).unwrap();
        let new_goal_region = game_engine.location().region_at(coordinate).unwrap();

        assert_eq!(
            game_engine.region_money(region_for_purchase.id()),
            Some(CONTROLLED_REGION_STARTING_MONEY - description(UnitType::Militia).purchase_cost)
        );
        assert_eq!(
            game_engine
                .location()
                .tile_at(coordinate)
                .unwrap()
                .unit()
                .unwrap()
                .description()
                .name,
            UnitType::Militia
        );
        assert_ne!(old_goal_region_id, new_goal_region.id());
        assert_eq!(*region_for_purchase, *new_goal_region);
        assert_eq!(game_engine.region_money(old_goal_region_id), None)
    }

    #[test]
    fn place_new_unit_with_attack_tile_with_unit_all_ok() {
        let (p1, p2, mut game_engine) = create_valid_engine();
        game_engine.act(p1.id(), PlayerAction::EndTurn).unwrap();

        let coordinate = Coord::new(1, 0);
        let old_goal_region_id = game_engine.location().region_at(coordinate).unwrap().id();
        let region_for_purchase_id = game_engine
            .location()
            .region_at(Coord::new(0, 1))
            .unwrap()
            .id();
        // Add some money for expensive unit
        game_engine.modify_money(
            region_for_purchase_id,
            description(UnitType::Knight).purchase_cost,
        );

        let action = PlayerAction::PlaceNewUnit(UnitType::Knight, coordinate);
        let res = game_engine.act(p2.id(), action);

        assert_eq!(res, Ok(()));

        let region_for_purchase = game_engine.location().region_at(Coord::new(0, 1)).unwrap();
        let new_goal_region = game_engine.location().region_at(coordinate).unwrap();

        assert_eq!(
            game_engine.region_money(region_for_purchase.id()),
            Some(CONTROLLED_REGION_STARTING_MONEY) // It should get back to standard
        );
        assert_eq!(
            game_engine
                .location()
                .tile_at(coordinate)
                .unwrap()
                .unit()
                .unwrap()
                .description()
                .name,
            UnitType::Knight
        );
        assert_ne!(old_goal_region_id, new_goal_region.id());
        assert_eq!(*region_for_purchase, *new_goal_region);
    }

    #[test]
    fn place_new_unit_with_attack_not_enough_attack() {
        let (p1, p2, mut game_engine) = create_valid_engine();
        game_engine.act(p1.id(), PlayerAction::EndTurn).unwrap();

        let coordinate = Coord::new(1, 0);
        let old_goal_region_id = game_engine.location().region_at(coordinate).unwrap().id();

        let action = PlayerAction::PlaceNewUnit(UnitType::Militia, coordinate);
        let res = game_engine.act(p2.id(), action);

        assert_eq!(res, Err(PlayerActionError::CannotAttack(coordinate)));

        let region_for_purchase = game_engine.location().region_at(Coord::new(0, 1)).unwrap();
        let new_goal_region = game_engine.location().region_at(coordinate).unwrap();

        assert_eq!(
            game_engine.region_money(region_for_purchase.id()),
            Some(CONTROLLED_REGION_STARTING_MONEY) // It should get back to standard
        );
        assert_eq!(
            game_engine
                .location()
                .tile_at(coordinate)
                .unwrap()
                .unit()
                .unwrap()
                .description()
                .name,
            UnitType::Soldier
        );
        assert_eq!(old_goal_region_id, new_goal_region.id());
    }

    #[test]
    fn place_new_unit_with_attack_tile_not_near_border() {
        let (p1, p2, mut game_engine) = create_valid_engine();
        game_engine.act(p1.id(), PlayerAction::EndTurn).unwrap();

        let coordinate = Coord::new(0, -1);
        let action = PlayerAction::PlaceNewUnit(UnitType::Militia, coordinate);
        let res = game_engine.act(p2.id(), action);

        assert_eq!(res, Err(PlayerActionError::CannotAttack(coordinate)));
    }
}
