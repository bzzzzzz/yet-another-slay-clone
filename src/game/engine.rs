use std::cmp::max;
use std::collections::{HashMap, HashSet};

use super::consts::*;
use super::ids::{IdProducer, ID};
use super::location::{
    Coord, Location, LocationModificationError, Player, Region, RegionTransformation, Unit,
    UnitType,
};
use super::rules::{
    validate_location, validate_regions, LocationRulesValidationError, RegionsValidationError,
};
use super::unit::{can_defeat, can_step_on, description, UnitInfo};

/// An error that can be returned as a result of game engine self validation process.
/// This is just a container for underlying errors
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

/// Description of actions that player can do
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Ord, PartialOrd)]
pub enum PlayerAction {
    PlaceNewUnit(ID, UnitType, Coord),
    UpgradeUnit(Coord),
    MoveUnit { src: Coord, dst: Coord },
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
    NotEnoughMoves(u32, u32),
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

/// Regional information that is stored on game engine level
/// money_balance value is stored only here, other values are recountable and stored only for caching purposes
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
            if let Some(unit) = tile.unit() {
                new_maintenance += description(unit.unit_type()).turn_cost;
            }
        }
        self.income_from_fields = new_income;
        self.maintenance_cost = new_maintenance;
    }
}

/// Game engine struct stores the whole state of the game and allows players to make their turns
#[derive(Eq, PartialEq, Debug)]
pub struct GameEngine {
    players: Vec<Player>,
    player_activity: HashMap<ID, bool>,
    winner: Option<ID>,
    current_turn: u32,
    active_player_num: usize,

    location: Location,
    region_info: HashMap<ID, RegionInfo>,
    unit_info: HashMap<ID, UnitInfo>,

    id_producer: IdProducer,
}

impl GameEngine {
    pub fn new(location: Location, players: Vec<Player>) -> Result<Self, EngineValidationError> {
        let mut region_info = HashMap::default();
        for (id, region) in location.regions().iter() {
            let money = if region.coordinates().len() >= MIN_CONTROLLED_REGION_SIZE {
                RegionInfo::new(CONTROLLED_REGION_STARTING_MONEY)
            } else {
                RegionInfo::new(0)
            };
            region_info.insert(id.clone(), money);
        }
        let player_activity: HashMap<ID, bool> = players.iter().map(|p| (p.id(), true)).collect();
        let unit_info: HashMap<ID, UnitInfo> = location
            .map()
            .values()
            .filter_map(|t| t.unit())
            .map(|u| (u.id(), UnitInfo::from(*u)))
            .collect();
        let mut engine = Self {
            location,
            players,
            player_activity,
            unit_info,
            region_info,
            winner: None,
            current_turn: 1,
            active_player_num: 0,
            id_producer: IdProducer::default(),
        };
        engine.recount_region_info();
        engine.validate()?;

        // Refill all units' moves before first turn
        engine.refill_moves();

        Ok(engine)
    }

    /// Check that locations is consistent and everything is placed corresponding to game rules
    pub fn validate(&self) -> Result<(), EngineValidationError> {
        let active_players: Vec<Player> = self
            .players
            .iter()
            .filter(|p| self.player_activity[&p.id()])
            .cloned()
            .collect();

        validate_location(&self.location)?;
        validate_regions(&self.location, &active_players.as_slice())?;

        Ok(())
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
        self.region_info.get(&region_id).map(|ri| ri.money_balance)
    }

    pub fn active_player_num(&self) -> usize {
        self.active_player_num
    }

    pub fn active_player(&self) -> &Player {
        &self.players[self.active_player_num]
    }

    /// Perform an action for specified player
    pub fn act(&mut self, player_id: ID, action: PlayerAction) -> Result<(), PlayerActionError> {
        self.validate_action(player_id, &action)?;

        match action {
            PlayerAction::MoveUnit { src, dst } => self.move_unit(player_id, src, dst)?,
            PlayerAction::PlaceNewUnit(orig_region_id, unit, dst) => {
                self.place_new_unit(player_id, orig_region_id, unit, dst)?
            }
            PlayerAction::UpgradeUnit(dst) => self.upgrade_unit(player_id, dst)?,
            PlayerAction::EndTurn => self.end_players_turn(),
        }

        self.recount_region_info();
        self.check_for_active_players();
        self.validate()
            .expect("Engine state should be always valid after an action");

        Ok(())
    }

    /// Check if some active players became unactive and update engine information about them
    fn check_for_active_players(&mut self) {
        let mut owner_to_active_regions_num: HashMap<ID, u32> = HashMap::new();
        for region in self.location.regions().values() {
            if region.coordinates().len() < MIN_CONTROLLED_REGION_SIZE {
                let coordinate = *region.coordinates().iter().next().unwrap();
                // If there is a moving unit on last tile of region - it is still active
                if let Some(unit) = self.location.tile_at(coordinate).unwrap().unit() {
                    if description(unit.unit_type()).max_moves == 0 {
                        continue;
                    }
                } else {
                    continue;
                }
            }
            let mut num = *owner_to_active_regions_num
                .get(&region.owner().id())
                .unwrap_or(&0);
            num += 1;
            owner_to_active_regions_num.insert(region.owner().id(), num);
        }
        let set_inactive: HashSet<ID> = self
            .player_activity
            .iter()
            .filter(|(_, &is_active)| is_active)
            .filter(|(id, _)| {
                owner_to_active_regions_num
                    .get(id)
                    .map(|&n| n == 0)
                    .unwrap_or(true)
            }).map(|(id, _)| *id)
            .collect();

        if !set_inactive.is_empty() {
            for &id in set_inactive.iter() {
                self.player_activity.insert(id, false);
            }
            for (id, region) in self.location.regions() {
                if set_inactive.contains(&region.owner().id()) {
                    let info = self.region_info.get_mut(id).unwrap();
                    info.money_balance = 0;
                }
            }
        }
    }

    /// Update region info for each region on the map
    fn recount_region_info(&mut self) {
        for (id, region) in self.location.regions() {
            let info = self.region_info.get_mut(&id).unwrap();
            info.recount(region, &self.location);
        }
    }

    /// Add provided amount of money to the region
    /// This method assumes that region exists and will panic if not
    fn modify_money(&mut self, region_id: ID, amount: i32) {
        let ri = self.region_info.get_mut(&region_id).unwrap();
        ri.change_balance(amount);
    }

    /// Return a region at specified coordinate
    fn region_at(&self, coordinate: Coord) -> Result<&Region, PlayerActionError> {
        self.location
            .region_at(coordinate)
            // According to game rules region can be only on land, so this also checks if we're
            // trying to place unit on water. We will need to change that if rules change
            .ok_or_else(|| PlayerActionError::InaccessibleLocation(coordinate))
    }

    fn unit_info(&self, unit_id: ID) -> &UnitInfo {
        &self.unit_info[&unit_id]
    }

    /// Check and prepare everything required for placing a unit on a new coordinate
    /// Returns a tuple with
    fn prepare_placing_unit(
        &self,
        player_id: ID,
        originating_region_id: ID,
        unit: Unit,
        dst: Coord,
    ) -> Result<(bool, Option<ID>), PlayerActionError> {
        if !self.unit_can_step_on_coord(unit, dst, originating_region_id, true) {
            return Err(PlayerActionError::InaccessibleLocation(dst));
        }
        let dst_region = self.region_at(dst)?;
        let need_relocation = dst_region.id() != originating_region_id;

        let tile = self.location.tile_at(dst).unwrap();
        let old_unit_to_remove = if let Some(current_unit) = tile.unit() {
            // We cannot replace unit of the same owner
            if dst_region.owner().id() == player_id {
                return Err(PlayerActionError::AlreadyOccupied(dst));
            }
            // But we can replace other player's unit if we defeat it
            if !can_defeat(unit, *current_unit) {
                return Err(PlayerActionError::CannotAttack(dst));
            }

            Some(current_unit.id())
        } else {
            None
        };

        Ok((need_relocation, old_unit_to_remove))
    }

    fn prepare_buying_unit(
        &self,
        player_id: ID,
        originating_region_id: ID,
        unit: Unit,
        dst: Coord,
    ) -> Result<(bool, Option<ID>), PlayerActionError> {
        let (need_relocation, old_unit_to_remove) =
            self.prepare_placing_unit(player_id, originating_region_id, unit, dst)?;

        let region_info = self.region_info[&originating_region_id];
        if !region_info.can_afford(description(unit.unit_type()).purchase_cost) {
            return Err(PlayerActionError::NotEnoughMoney(originating_region_id));
        }

        Ok((need_relocation, old_unit_to_remove))
    }

    fn place_new_unit(
        &mut self,
        player_id: ID,
        originating_region_id: ID,
        unit_type: UnitType,
        dst: Coord,
    ) -> Result<(), PlayerActionError> {
        let (unit, unit_info) = UnitInfo::new(self.id_producer.next(), unit_type);
        let (need_relocation, old_unit_to_remove) =
            self.prepare_buying_unit(player_id, originating_region_id, unit, dst)?;

        if need_relocation {
            self.add_tile_to_region(dst, originating_region_id)?;
        }
        self.location.place_unit(unit, dst)?;

        if let Some(old_unit_id) = old_unit_to_remove {
            self.unit_info.remove(&old_unit_id);
        }
        self.unit_info.insert(unit.id(), unit_info);
        self.modify_money(
            originating_region_id,
            0 - description(unit.unit_type()).purchase_cost,
        );

        Ok(())
    }

    fn add_tile_to_region(
        &mut self,
        coordinate: Coord,
        region_id: ID,
    ) -> Result<(), PlayerActionError> {
        let old_region_id = self.location.region_at(coordinate).unwrap().id();
        // We need to handle region changes after it.
        let res = self
            .location
            .add_tile_to_region(coordinate, region_id, &mut self.id_producer)?;
        if res.is_empty() {
            self.fix_capital(old_region_id);
        }
        for change in res.iter() {
            match change {
                RegionTransformation::Delete(id) => {
                    self.region_info.remove(&id);
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
        self.fix_capital(into);
        let src = self.region_info.remove(&from).unwrap();
        let dst = self.region_info.get_mut(&into).unwrap();
        dst.change_balance(src.money_balance);
        dst.maintenance_cost += src.maintenance_cost;
        dst.income_from_fields += src.income_from_fields;
    }

    fn split_region(&mut self, from: ID, into: Vec<ID>) {
        let src = self.region_info.remove(&from).unwrap();
        let mut insert = Vec::new();
        let mut new_money_owners = Vec::new();
        for region_id in into.into_iter() {
            self.fix_capital(region_id);
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
                self.region_info.insert(id, info);
            }
        }
        for (id, info) in insert.into_iter() {
            self.region_info.insert(id, info);
        }
    }

    fn fix_capital(&mut self, region_id: ID) {
        let capitals: Vec<Coord> = self
            .location
            .regions()
            .get(&region_id)
            .unwrap()
            .coordinates()
            .iter()
            .map(|c| (c, self.location.tile_at(*c).unwrap()))
            .filter(|(_, tile)| tile.unit().is_some())
            .filter(|(_, tile)| tile.unit().unwrap().unit_type() == UnitType::Village)
            .map(|(c, _)| *c)
            .collect();
        let size = self
            .location
            .regions()
            .get(&region_id)
            .unwrap()
            .coordinates()
            .len();
        if size == 1 {
            if capitals.is_empty() {
                return;
            }
            let c = *self
                .location
                .regions()
                .get(&region_id)
                .unwrap()
                .coordinates()
                .iter()
                .next()
                .unwrap();
            self.location.remove_unit(c).unwrap();
        } else if capitals.is_empty() {
            let coord = self
                .location
                .regions()
                .get(&region_id)
                .unwrap()
                .coordinates()
                .iter()
                .map(|c| (c, self.location.tile_at(*c).unwrap()))
                .find(|(_, tile)| tile.unit().is_none())
                .map_or_else(
                    || {
                        *self
                            .location
                            .regions()
                            .get(&region_id)
                            .unwrap()
                            .coordinates()
                            .iter()
                            .next()
                            .unwrap()
                    },
                    |(c, _)| *c,
                );

            let (unit, info) = UnitInfo::new(self.id_producer.next(), UnitType::Village);
            self.location.place_unit(unit, coord).unwrap();
            self.unit_info.insert(unit.id(), info);
        } else if capitals.len() > 1 {
            let c = capitals[0];
            self.location.remove_unit(c).unwrap();
        }
    }

    /// Return true if unit can step on tile with specified coordinate
    ///
    /// Unit can step on tile if tile's surface is land and one of the following is true:
    ///
    /// - tile is a part of region unit belongs to and there is no unit on tile
    /// - tile is adjacent to the region unit belongs to and tile defence is lower than unit attack
    ///   (tile defence is the defence of unit on this tile or max defence of neighbour tile that
    ///   belongs to the same region)
    ///
    fn unit_can_step_on_coord(
        &self,
        unit: Unit,
        coordinate: Coord,
        original_region_id: ID,
        is_last_step: bool,
    ) -> bool {
        let tile = self.location.tile_at(coordinate);
        if tile.is_none() || !can_step_on(unit, tile.unwrap()) {
            return false;
        }
        let tile = tile.unwrap();
        let dst_region = self.region_at(coordinate).unwrap();

        if dst_region.id() == original_region_id {
            return !is_last_step || tile.unit().is_none() || tile.unit().unwrap().id() == unit.id();
        }
        if !is_last_step {
            return false;
        }
        let neighbours = coordinate.neighbors();
        let original_region = &self.location.regions()[&original_region_id];
        let neighbour_from_original_region = neighbours
            .iter()
            .find(|c| original_region.coordinates().contains(c));

        if neighbour_from_original_region.is_none() {
            return false;
        }
        let unit_defence = tile
            .unit()
            .map_or(EMPTY_TILE_DEFENCE, |u| description(u.unit_type()).defence);
        let max_defence = neighbours
            .iter()
            .filter(|&n| {
                self.location
                    .region_at(*n)
                    .map_or(false, |r| r.id() == dst_region.id())
            }).filter_map(|&n| self.location.tile_at(n))
            .filter_map(|t| t.unit())
            .map(|u| description(u.unit_type()).defence)
            .max()
            .unwrap_or(EMPTY_TILE_DEFENCE);

        max(max_defence, unit_defence) < description(unit.unit_type()).attack
    }

    fn prepare_moving_unit(
        &self,
        player_id: ID,
        src: Coord,
        dst: Coord,
    ) -> Result<(ID, u32, ID, bool, Option<ID>), PlayerActionError> {
        let unit = self
            .location
            .tile_at(src)
            .ok_or_else(|| PlayerActionError::InaccessibleLocation(src))?
            .unit();
        let region = self.region_at(src)?;
        if region.owner().id() != player_id {
            return Err(PlayerActionError::NotOwned(src));
        }
        if unit.is_none() {
            return Err(PlayerActionError::NoUnit(dst));
        }
        let unit = unit.unwrap();

        let (need_relocation, old_unit_id_to_remove) =
            self.prepare_placing_unit(player_id, region.id(), *unit, dst)?;

        let distance = self.location.bfs_distance(src, dst, |c| {
            self.unit_can_step_on_coord(*unit, c, region.id(), c == dst)
        });
        let unit_info = self.unit_info(unit.id());
        if distance.is_none() {
            return Err(PlayerActionError::InaccessibleLocation(dst));
        } else if unit_info.moves_left() < distance.unwrap() {
            return Err(PlayerActionError::NotEnoughMoves(
                unit_info.moves_left(),
                distance.unwrap(),
            ));
        }
        let moves_to_subtract = if need_relocation {
            unit_info.moves_left()
        } else {
            distance.unwrap()
        };

        Ok((
            unit.id(),
            moves_to_subtract,
            region.id(),
            need_relocation,
            old_unit_id_to_remove,
        ))
    }

    fn move_unit(
        &mut self,
        player_id: ID,
        src: Coord,
        dst: Coord,
    ) -> Result<(), PlayerActionError> {
        let (unit_id, moves_num, region_id, need_relocation, old_unit_id_to_remove) =
            self.prepare_moving_unit(player_id, src, dst)?;

        if need_relocation {
            self.add_tile_to_region(dst, region_id)?;
        }
        self.location.move_unit(src, dst)?;
        self.unit_info
            .get_mut(&unit_id)
            .unwrap()
            .subtract_moves(moves_num);
        if let Some(old_unit_id) = old_unit_id_to_remove {
            self.unit_info.remove(&old_unit_id);
        }

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

        let old_unit_description = description(old_unit.unit_type());
        let new_unit_description = old_unit_description.upgrades_to;
        if new_unit_description.is_none() {
            return Err(PlayerActionError::NoUpgrade(old_unit_description.name));
        }
        let new_unit_description = new_unit_description.unwrap();

        let sum = new_unit_description.purchase_cost - old_unit_description.purchase_cost;

        let region_info = self.region_info[&region.id()];
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
                .map_or(false, |u| u.unit_type() == UnitType::Grave)
            {
                to_delete.push(coord);
            }
        }
        for coordinate in to_delete.into_iter() {
            self.location.remove_unit(coordinate).unwrap();
        }
    }

    fn apply_income(&mut self) {
        for info in self.region_info.values_mut() {
            let sum = info.income_from_fields - info.maintenance_cost;
            info.change_balance(sum);
        }
    }

    fn refill_moves(&mut self) {
        for info in self.unit_info.values_mut() {
            info.refill_moves();
        }
    }

    fn kill_starving_units(&mut self) {
        let regions_to_check: Vec<ID> = self
            .region_info
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
            .filter(|(_, t)| t.unit().is_some() && t.unit().unwrap().unit_type() == UnitType::Tree)
            .flat_map(|(&c, _)| {
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
        self.refill_moves();
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
    use game::ids::{IdProducer, ID};
    use game::location::TileSurface::*;
    use game::location::{Coord, Location, Player, Region, Tile, Unit, UnitType};
    use game::unit::description;

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
    fn create_valid_engine() -> (Vec<Player>, Vec<ID>, GameEngine) {
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

        let players = vec![
            Player::new(id_producer.next()),
            Player::new(id_producer.next()),
            Player::new(id_producer.next()),
        ];
        let region_ids = vec![
            id_producer.next(),
            id_producer.next(),
            id_producer.next(),
            id_producer.next(),
        ];

        let coords = [
            Coord::new(0, -1),
            Coord::new(1, -1),
            Coord::new(2, -1),
            Coord::new(1, 0),
        ]
            .iter()
            .cloned()
            .collect();
        let region_one = Region::new(region_ids[0], players[0], coords);

        let coords = [Coord::new(2, 0), Coord::new(1, 1), Coord::new(0, 1)]
            .iter()
            .cloned()
            .collect();
        let region_two = Region::new(region_ids[1], players[1], coords);

        let coords = [Coord::new(-1, 1)].iter().cloned().collect();
        let region_three = Region::new(region_ids[2], players[0], coords);

        let coords = [Coord::new(-1, 0), Coord::new(-2, 1)]
            .iter()
            .cloned()
            .collect();
        let region_four = Region::new(region_ids[3], players[2], coords);

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

        let game_engine =
            GameEngine::new(location, vec![players[0], players[1], players[2]]).unwrap();

        (players, region_ids, game_engine)
    }

    #[test]
    fn create_engine_correct() {
        let (pl, ri, game_engine) = create_valid_engine();

        assert_eq!(*game_engine.active_player(), pl[0]);
        assert_eq!(game_engine.current_turn(), 1);

        assert_eq!(
            game_engine.region_money(ri[0]),
            Some(CONTROLLED_REGION_STARTING_MONEY)
        );
        assert_eq!(
            game_engine.region_money(ri[1]),
            Some(CONTROLLED_REGION_STARTING_MONEY)
        );
        assert_eq!(game_engine.region_money(ri[2]), Some(0));
        assert_eq!(
            game_engine.region_money(ri[3]),
            Some(CONTROLLED_REGION_STARTING_MONEY)
        );
    }

    #[test]
    fn place_new_unit_simple_ok() {
        let (pl, ri, mut game_engine) = create_valid_engine();
        let coordinate = Coord::new(2, -1);

        let action = PlayerAction::PlaceNewUnit(ri[0], UnitType::Militia, coordinate);
        let res = game_engine.act(pl[0].id(), action);

        let region = game_engine.location().region_at(coordinate).unwrap();
        assert_eq!(res, Ok(()));
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
        assert_eq!(unit.unit_type(), UnitType::Militia);
        assert_eq!(game_engine.unit_info(unit.id()).moves_left(), 0);
    }

    #[test]
    fn place_new_unit_simple_no_money() {
        let (pl, ri, mut game_engine) = create_valid_engine();
        let coordinate = Coord::new(2, -1);

        let action = PlayerAction::PlaceNewUnit(ri[0], UnitType::Knight, coordinate);
        let res = game_engine.act(pl[0].id(), action);

        assert_eq!(res, Err(PlayerActionError::NotEnoughMoney(ri[0])));
        assert_eq!(
            game_engine.region_money(ri[0]),
            Some(CONTROLLED_REGION_STARTING_MONEY)
        );
        assert_eq!(
            game_engine.location().tile_at(coordinate).unwrap().unit(),
            None
        )
    }

    #[test]
    fn place_new_unit_simple_tile_out_of_border() {
        let (pl, ri, mut game_engine) = create_valid_engine();
        let coordinate = Coord::new(-1, -1);

        let action = PlayerAction::PlaceNewUnit(ri[0], UnitType::Militia, coordinate);
        let res = game_engine.act(pl[0].id(), action);

        assert_eq!(
            res,
            Err(PlayerActionError::InaccessibleLocation(coordinate))
        );
        assert_eq!(
            game_engine.region_money(ri[0]),
            Some(CONTROLLED_REGION_STARTING_MONEY)
        );
    }

    #[test]
    fn place_new_unit_simple_tile_bad_surface() {
        let (pl, ri, mut game_engine) = create_valid_engine();
        let coordinate = Coord::new(0, 0);

        let action = PlayerAction::PlaceNewUnit(ri[0], UnitType::Militia, coordinate);
        let res = game_engine.act(pl[0].id(), action);

        assert_eq!(
            res,
            Err(PlayerActionError::InaccessibleLocation(coordinate))
        );
        assert_eq!(
            game_engine.region_money(ri[0]),
            Some(CONTROLLED_REGION_STARTING_MONEY)
        );
    }

    #[test]
    fn place_new_unit_simple_tile_has_other_unit() {
        let (pl, ri, mut game_engine) = create_valid_engine();
        let coordinate = Coord::new(1, 0);

        let action = PlayerAction::PlaceNewUnit(ri[0], UnitType::Militia, coordinate);
        let res = game_engine.act(pl[0].id(), action);

        assert_eq!(
            res,
            Err(PlayerActionError::InaccessibleLocation(coordinate))
        );
        assert_eq!(
            game_engine.region_money(ri[0]),
            Some(CONTROLLED_REGION_STARTING_MONEY)
        );
    }

    #[test]
    fn place_new_unit_simple_others_players_turn() {
        let (pl, ri, mut game_engine) = create_valid_engine();

        let coordinate = Coord::new(1, 1);
        let action = PlayerAction::PlaceNewUnit(ri[1], UnitType::Knight, coordinate);
        let res = game_engine.act(pl[1].id(), action);

        assert_eq!(res, Err(PlayerActionError::OtherPlayersTurn(pl[0].id())));
        assert_eq!(
            game_engine.region_money(ri[1]),
            Some(CONTROLLED_REGION_STARTING_MONEY)
        );
    }

    #[test]
    fn place_new_unit_simple_game_finished() {
        let (pl, ri, mut game_engine) = create_valid_engine();
        game_engine.winner = Some(pl[0].id());

        let coordinate = Coord::new(0, -1);
        let action = PlayerAction::PlaceNewUnit(ri[0], UnitType::Knight, coordinate);
        let res = game_engine.act(pl[0].id(), action);

        assert_eq!(res, Err(PlayerActionError::GameAlreadyFinished));
        assert_eq!(
            game_engine.region_money(ri[0]),
            Some(CONTROLLED_REGION_STARTING_MONEY)
        );
    }

    #[test]
    fn place_new_unit_with_attack_empty_tile_all_ok() {
        let (pl, ri, mut game_engine) = create_valid_engine();
        game_engine.act(pl[0].id(), PlayerAction::EndTurn).unwrap();

        let coordinate = Coord::new(-1, 1);
        let old_goal_region_id = game_engine.location().region_at(coordinate).unwrap().id();

        let action = PlayerAction::PlaceNewUnit(ri[1], UnitType::Militia, coordinate);
        let res = game_engine.act(pl[1].id(), action);

        assert_eq!(res, Ok(()));

        let region_for_purchase = game_engine.location().region_at(Coord::new(0, 1)).unwrap();
        let new_goal_region = game_engine.location().region_at(coordinate).unwrap();
        let unit = game_engine
            .location()
            .tile_at(coordinate)
            .unwrap()
            .unit()
            .unwrap();

        assert_eq!(
            game_engine.region_money(region_for_purchase.id()),
            Some(CONTROLLED_REGION_STARTING_MONEY - description(UnitType::Militia).purchase_cost)
        );
        assert_eq!(game_engine.unit_info(unit.id()).moves_left(), 0);
        assert_eq!(
            game_engine
                .location()
                .tile_at(coordinate)
                .unwrap()
                .unit()
                .unwrap()
                .unit_type(),
            UnitType::Militia
        );
        assert_ne!(old_goal_region_id, new_goal_region.id());
        assert_eq!(*region_for_purchase, *new_goal_region);
        assert_eq!(game_engine.region_money(old_goal_region_id), None)
    }

    #[test]
    fn place_new_unit_with_attack_tile_with_unit_all_ok() {
        let (pl, ri, mut game_engine) = create_valid_engine();
        game_engine.act(pl[0].id(), PlayerAction::EndTurn).unwrap();

        let coordinate = Coord::new(1, 0);
        let old_goal_region_id = ri[0];
        // Add some money for expensive unit
        game_engine.modify_money(ri[1], description(UnitType::Knight).purchase_cost);

        let action = PlayerAction::PlaceNewUnit(ri[1], UnitType::Knight, coordinate);
        let res = game_engine.act(pl[1].id(), action);

        assert_eq!(res, Ok(()));

        let region_for_purchase = game_engine.location().region_at(Coord::new(0, 1)).unwrap();
        let new_goal_region = game_engine.location().region_at(coordinate).unwrap();

        assert_eq!(
            game_engine.region_money(ri[1]),
            Some(CONTROLLED_REGION_STARTING_MONEY) // It should get back to standard
        );
        assert_eq!(
            game_engine
                .location()
                .tile_at(coordinate)
                .unwrap()
                .unit()
                .unwrap()
                .unit_type(),
            UnitType::Knight
        );
        assert_ne!(old_goal_region_id, new_goal_region.id());
        assert_eq!(*region_for_purchase, *new_goal_region);
    }

    #[test]
    fn place_new_unit_with_attack_not_enough_attack() {
        let (pl, ri, mut game_engine) = create_valid_engine();
        game_engine.act(pl[0].id(), PlayerAction::EndTurn).unwrap();

        let coordinate = Coord::new(1, 0);
        let old_goal_region_id = ri[0];

        let action = PlayerAction::PlaceNewUnit(ri[1], UnitType::Militia, coordinate);
        let res = game_engine.act(pl[1].id(), action);

        assert_eq!(
            res,
            Err(PlayerActionError::InaccessibleLocation(coordinate))
        );

        let new_goal_region = game_engine.location().region_at(coordinate).unwrap();

        assert_eq!(
            game_engine.region_money(ri[1]),
            Some(CONTROLLED_REGION_STARTING_MONEY) // It should get back to standard
        );
        assert_eq!(
            game_engine
                .location()
                .tile_at(coordinate)
                .unwrap()
                .unit()
                .unwrap()
                .unit_type(),
            UnitType::Soldier
        );
        assert_eq!(old_goal_region_id, new_goal_region.id());
    }

    #[test]
    fn place_new_unit_with_attack_tile_not_near_border() {
        let (pl, ri, mut game_engine) = create_valid_engine();
        game_engine.act(pl[0].id(), PlayerAction::EndTurn).unwrap();

        let coordinate = Coord::new(0, -1);
        let action = PlayerAction::PlaceNewUnit(ri[1], UnitType::Militia, coordinate);
        let res = game_engine.act(pl[1].id(), action);

        assert_eq!(
            res,
            Err(PlayerActionError::InaccessibleLocation(coordinate))
        );
    }

    #[test]
    fn move_unit_inside_region_ok() {
        let (pl, _, mut game_engine) = create_valid_engine();

        let (src, dst) = (Coord::new(1, 0), Coord::new(2, -1));
        let action = PlayerAction::MoveUnit { src, dst };
        let res = game_engine.act(pl[0].id(), action);

        assert_eq!(res, Ok(()));
        assert_eq!(game_engine.location().tile_at(src).unwrap().unit(), None);

        {
            let unit = game_engine.location().tile_at(dst).unwrap().unit().unwrap();
            let info = game_engine.unit_info(unit.id());
            assert_eq!(unit.unit_type(), UnitType::Soldier);
            assert_eq!(info.moves_left(), info.description().max_moves - 1);
        }

        // And one more
        let (src, dst) = (Coord::new(2, -1), Coord::new(0, -1));
        let action = PlayerAction::MoveUnit { src, dst };
        let res = game_engine.act(pl[0].id(), action);

        assert_eq!(res, Ok(()));
        assert_eq!(game_engine.location().tile_at(src).unwrap().unit(), None);

        {
            let unit = game_engine.location().tile_at(dst).unwrap().unit().unwrap();
            let info = game_engine.unit_info(unit.id());
            assert_eq!(unit.unit_type(), UnitType::Soldier);
            assert_eq!(info.moves_left(), info.description().max_moves - 3);
        }

        // And back, so we have no more moves after
        let (src, dst) = (Coord::new(0, -1), Coord::new(1, 0));
        let action = PlayerAction::MoveUnit { src, dst };
        let res = game_engine.act(pl[0].id(), action);

        assert_eq!(res, Ok(()));
        assert_eq!(game_engine.location().tile_at(src).unwrap().unit(), None);

        {
            let unit = game_engine.location().tile_at(dst).unwrap().unit().unwrap();
            let info = game_engine.unit_info(unit.id());
            assert_eq!(unit.unit_type(), UnitType::Soldier);
            assert_eq!(info.moves_left(), 0);
        }

        // And now we will get error, because there are no moves left
        let (src, dst) = (Coord::new(1, 0), Coord::new(2, -1));
        let action = PlayerAction::MoveUnit { src, dst };
        let res = game_engine.act(pl[0].id(), action);

        assert_eq!(res, Err(PlayerActionError::NotEnoughMoves(0, 1)));
        assert_eq!(game_engine.location().tile_at(dst).unwrap().unit(), None);

        let unit = game_engine.location().tile_at(src).unwrap().unit().unwrap();
        let info = game_engine.unit_info(unit.id());
        assert_eq!(unit.unit_type(), UnitType::Soldier);
        assert_eq!(info.moves_left(), 0);
    }

    #[test]
    fn move_unit_inside_region_error_already_has_unit() {
        let (pl, _, mut game_engine) = create_valid_engine();

        let (src, dst) = (Coord::new(1, 0), Coord::new(1, -1));
        let action = PlayerAction::MoveUnit { src, dst };
        let res = game_engine.act(pl[0].id(), action);

        assert_eq!(res, Err(PlayerActionError::InaccessibleLocation(dst)));

        let village = game_engine.location().tile_at(dst).unwrap().unit().unwrap();
        let unit = game_engine.location().tile_at(src).unwrap().unit().unwrap();
        let info = game_engine.unit_info(unit.id());
        assert_eq!(unit.unit_type(), UnitType::Soldier);
        assert_eq!(village.unit_type(), UnitType::Village);
        assert_eq!(info.moves_left(), info.description().max_moves);
    }

    #[test]
    fn move_unit_inside_region_error_dst_outside_location() {
        let (pl, _, mut game_engine) = create_valid_engine();

        let (src, dst) = (Coord::new(1, 0), Coord::new(3, -2));
        let action = PlayerAction::MoveUnit { src, dst };
        let res = game_engine.act(pl[0].id(), action);

        assert_eq!(res, Err(PlayerActionError::InaccessibleLocation(dst)));

        let unit = game_engine.location().tile_at(src).unwrap().unit().unwrap();
        let info = game_engine.unit_info(unit.id());
        assert_eq!(unit.unit_type(), UnitType::Soldier);
        assert_eq!(info.moves_left(), info.description().max_moves);
    }

    #[test]
    fn move_unit_inside_region_error_dst_is_water() {
        let (pl, _, mut game_engine) = create_valid_engine();

        let (src, dst) = (Coord::new(1, 0), Coord::new(0, 0));
        let action = PlayerAction::MoveUnit { src, dst };
        let res = game_engine.act(pl[0].id(), action);

        assert_eq!(res, Err(PlayerActionError::InaccessibleLocation(dst)));

        let unit = game_engine.location().tile_at(src).unwrap().unit().unwrap();
        let info = game_engine.unit_info(unit.id());
        assert_eq!(unit.unit_type(), UnitType::Soldier);
        assert_eq!(info.moves_left(), info.description().max_moves);
    }

    fn successful_attack(src: Coord, dst: Coord) -> (Vec<Player>, Vec<ID>, GameEngine) {
        let (pl, ri, mut game_engine) = create_valid_engine();

        let old_dst_region_id = game_engine.location().region_at(dst).unwrap().id();
        let old_dst_unit = game_engine.location().tile_at(dst).unwrap().unit().cloned();
        let action = PlayerAction::MoveUnit { src, dst };
        let res = game_engine.act(pl[0].id(), action);

        assert_eq!(res, Ok(()));
        assert_eq!(game_engine.location().tile_at(src).unwrap().unit(), None);

        assert!(
            old_dst_unit.is_none() || !game_engine
                .unit_info
                .contains_key(&old_dst_unit.unwrap().id())
        );

        {
            let unit = game_engine.location().tile_at(dst).unwrap().unit().unwrap();
            let info = game_engine.unit_info(unit.id());
            assert_eq!(unit.unit_type(), UnitType::Soldier);

            // Unit cannot move after attack
            assert_eq!(info.moves_left(), 0);
            let src_region = game_engine.location().region_at(src).unwrap();
            let dst_region = game_engine.location().region_at(dst).unwrap();
            assert_eq!(src_region, dst_region);

            let old_region = game_engine.location().regions().get(&old_dst_region_id);
            assert!(old_region.is_none() || !old_region.unwrap().coordinates().contains(&dst));
        }

        (pl, ri, game_engine)
    }

    #[test]
    fn move_unit_outside_region_no_unit_all_ok() {
        let (_, _, game_engine) = successful_attack(Coord::new(1, 0), Coord::new(1, 1));

        let new_split_reg_one = game_engine.location().region_at(Coord::new(2, 0)).unwrap();
        let new_split_reg_two = game_engine.location().region_at(Coord::new(0, 1)).unwrap();
        assert_ne!(new_split_reg_one, new_split_reg_two);

        assert_eq!(game_engine.region_money(new_split_reg_one.id()), Some(0));
        assert_eq!(game_engine.region_money(new_split_reg_two.id()), Some(0));

        let old_capital = game_engine
            .location()
            .tile_at(Coord::new(2, 0))
            .unwrap()
            .unit();
        assert_eq!(old_capital, None);

        let militia = game_engine
            .location()
            .tile_at(Coord::new(0, 1))
            .unwrap()
            .unit();
        assert!(militia.is_some());
    }

    #[test]
    fn move_unit_outside_region_has_unit_all_ok() {
        let (_, ri, game_engine) = successful_attack(Coord::new(1, 0), Coord::new(0, 1));

        assert_eq!(
            game_engine.region_money(ri[1]),
            Some(CONTROLLED_REGION_STARTING_MONEY)
        );
        assert!(
            game_engine.location().regions()[&ri[0]]
                .coordinates()
                .contains(&Coord::new(-1, 1))
        );

        let region_info = &game_engine.region_info[&ri[0]];
        assert_eq!(region_info.money_balance, CONTROLLED_REGION_STARTING_MONEY);
        assert_eq!(region_info.income_from_fields, 6);
        assert_eq!(game_engine.region_money(ri[2]), None);
    }

    #[test]
    fn move_unit_outside_region_has_capital_capital_moves_all_ok() {
        let (_, ri, game_engine) = successful_attack(Coord::new(1, 0), Coord::new(2, 0));

        assert_eq!(
            game_engine.region_money(ri[1]),
            Some(CONTROLLED_REGION_STARTING_MONEY)
        );
        let new_capital = game_engine
            .location()
            .tile_at(Coord::new(1, 1))
            .unwrap()
            .unit();
        assert!(new_capital.is_some());
        assert_eq!(new_capital.unwrap().unit_type(), UnitType::Village);
    }

    #[test]
    fn move_unit_outside_region_has_capital_region_destroyed_all_ok() {
        let (pl, ri, game_engine) = successful_attack(Coord::new(1, 0), Coord::new(-1, 0));

        assert_eq!(
            game_engine
                .location()
                .tile_at(Coord::new(-2, 1))
                .unwrap()
                .unit(),
            None
        );
        assert_eq!(game_engine.region_money(ri[3]), Some(0));
        assert_eq!(game_engine.player_activity[&pl[2].id()], false);
    }
}
