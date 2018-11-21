use std::cmp::max;
use std::collections::{HashMap, HashSet};

use super::consts::*;
use super::ids::{IdProducer, ID};
use super::location::{
    Coord, Location, LocationModificationError, LocationValidationError, Player, Region,
    RegionTransformation, Unit, UnitType,
};
use super::rules::{
    validate_location, validate_regions, LocationRulesValidationError, RegionsValidationError,
};
use super::unit::{can_defeat, can_step_on, description, merge_result, UnitInfo};

/// An error that can be returned as a result of game engine self validation process.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Ord, PartialOrd)]
pub enum EngineValidationError {
    LocationError(LocationRulesValidationError),
    RegionsError(RegionsValidationError),
    RegionWithoutInfo(ID),
    UnitWithoutInfo(ID),
    UnlinkedRegionInfo(ID),
    UnlinkedUnitInfo(ID),
}

impl From<LocationRulesValidationError> for EngineValidationError {
    fn from(e: LocationRulesValidationError) -> Self {
        EngineValidationError::LocationError(e)
    }
}

impl From<LocationValidationError> for EngineValidationError {
    fn from(e: LocationValidationError) -> Self {
        EngineValidationError::LocationError(LocationRulesValidationError::InitiationError(e))
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
    CannotBePlacedByPlayer(UnitType),
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
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Ord, PartialOrd, Serialize, Deserialize)]
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
#[derive(Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
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
    pub fn new(
        location: Location,
        players: Vec<Player>,
        id_producer: IdProducer,
    ) -> Result<Self, EngineValidationError> {
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
            id_producer,
            winner: None,
            current_turn: 1,
            active_player_num: 0,
        };
        engine.recount_region_info();
        engine.validate()?;

        // Refill all units' moves before first turn
        engine.refill_moves();

        Ok(engine)
    }

    /// Fix all countable fields
    pub fn repair(&mut self) {
        self.recount_region_info();
        let mut to_fix = Vec::new();
        for tile in self.location().map().values() {
            if let Some(unit) = tile.unit() {
                to_fix.push((unit.id(), unit.unit_type()));
            }
        }

        for (id, unit_type) in to_fix.into_iter() {
            let info = self.unit_info.get_mut(&id).unwrap();
            info.change_description(description(unit_type));
        }
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
        self.validate_internal_consistency()?;

        Ok(())
    }

    fn validate_internal_consistency(&self) -> Result<(), EngineValidationError> {
        let mut region_ids: HashSet<ID> = self.region_info.keys().cloned().collect();
        for id in self.location.regions().keys() {
            if !region_ids.contains(id) {
                return Err(EngineValidationError::RegionWithoutInfo(*id));
            }
            region_ids.remove(id);
        }
        if !region_ids.is_empty() {
            return Err(EngineValidationError::UnlinkedRegionInfo(
                *region_ids.iter().next().unwrap(),
            ));
        }

        let mut unit_ids: HashSet<ID> = self.unit_info.keys().cloned().collect();
        for tile in self.location.map().values() {
            let unit = tile.unit();
            if unit.is_none() {
                continue;
            }
            let unit = unit.unwrap();
            if !unit_ids.contains(&unit.id()) {
                return Err(EngineValidationError::UnitWithoutInfo(unit.id()));
            }
            unit_ids.remove(&unit.id());
        }
        if !unit_ids.is_empty() {
            return Err(EngineValidationError::UnlinkedUnitInfo(
                *unit_ids.iter().next().unwrap(),
            ));
        }
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

    pub fn winner(&self) -> Option<ID> {
        self.winner
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
    fn prepare_placing_unit(
        &self,
        player_id: ID,
        originating_region_id: ID,
        unit_type: UnitType,
        dst: Coord,
    ) -> Result<(bool, Option<ID>, Option<UnitType>), PlayerActionError> {
        if !self.unit_can_step_on_coord(unit_type, dst, originating_region_id, true) {
            return Err(PlayerActionError::InaccessibleLocation(dst));
        }
        let dst_region = self.region_at(dst)?;
        let need_relocation = dst_region.id() != originating_region_id;

        let tile = self.location.tile_at(dst).unwrap();
        let mut upgrade_to: Option<UnitType> = None;
        let old_unit_to_remove = if let Some(current_unit) = tile.unit() {
            // We cannot replace unit of the same owner
            if dst_region.owner().id() == player_id {
                let possible_merge_result = merge_result(unit_type, current_unit.unit_type());
                if possible_merge_result.is_none() {
                    return Err(PlayerActionError::AlreadyOccupied(dst));
                }
                if possible_merge_result.unwrap() != unit_type {
                    upgrade_to = possible_merge_result;
                }
            } else if !can_defeat(unit_type, current_unit.unit_type()) {
                return Err(PlayerActionError::CannotAttack(dst));
            }

            Some(current_unit.id())
        } else {
            None
        };

        Ok((need_relocation, old_unit_to_remove, upgrade_to))
    }

    fn prepare_buying_unit(
        &self,
        player_id: ID,
        originating_region_id: ID,
        unit_type: UnitType,
        dst: Coord,
    ) -> Result<(bool, Option<ID>), PlayerActionError> {
        let unit_description = description(unit_type);
        if !unit_description.is_purchasable {
            return Err(PlayerActionError::CannotBePlacedByPlayer(unit_type));
        }
        let (need_relocation, old_unit_to_remove, upgrade_to) =
            self.prepare_placing_unit(player_id, originating_region_id, unit_type, dst)?;

        // You cannot place unit to merge it
        if upgrade_to.is_some() {
            return Err(PlayerActionError::InaccessibleLocation(dst));
        }
        let region_info = self.region_info[&originating_region_id];
        if !region_info.can_afford(unit_description.purchase_cost) {
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
        let (need_relocation, old_unit_to_remove) =
            self.prepare_buying_unit(player_id, originating_region_id, unit_type, dst)?;

        self.create_and_place_unit(unit_type, dst)?;
        if need_relocation {
            self.add_tile_to_region(dst, originating_region_id)?;
        }

        if let Some(old_unit_id) = old_unit_to_remove {
            self.unit_info.remove(&old_unit_id);
        }
        self.modify_money(
            originating_region_id,
            0 - description(unit_type).purchase_cost,
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

    fn maybe_remove_unit(&mut self, coordinate: Coord) -> Option<(Unit, UnitInfo)> {
        let unit = self.location.remove_unit(coordinate).unwrap()?;
        let info = self.unit_info.remove(&unit.id()).unwrap();

        Some((unit, info))
    }

    fn create_and_place_unit(
        &mut self,
        unit_type: UnitType,
        coordinate: Coord,
    ) -> Result<ID, LocationModificationError> {
        let (unit, info) = UnitInfo::new(self.id_producer.next_id(), unit_type);
        self.unit_info.insert(unit.id(), info);

        self.location.place_unit(unit, coordinate)?;

        Ok(unit.id())
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
            self.maybe_remove_unit(c).unwrap();
        } else if capitals.is_empty() {
            // TODO: now capital to create is somehow random. We need to make selection predictable one day
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

            self.maybe_remove_unit(coord);
            self.create_and_place_unit(UnitType::Village, coord)
                .unwrap();
        } else if capitals.len() > 1 {
            // TODO: now capital to keep is somehow random. We need to make selection predictable one day
            // The best way is to keep a capital of biggest and richest region.
            for &c in capitals.iter().skip(1) {
                self.maybe_remove_unit(c).unwrap();
            }
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
        unit_type: UnitType,
        coordinate: Coord,
        original_region_id: ID,
        is_last_step: bool,
    ) -> bool {
        let tile = self.location.tile_at(coordinate);
        if tile.is_none() || !can_step_on(unit_type, tile.unwrap()) {
            return false;
        }
        let tile = tile.unwrap();
        let dst_region = self.region_at(coordinate).unwrap();

        if dst_region.id() == original_region_id {
            return !is_last_step
                || tile.unit().is_none()
                || merge_result(unit_type, tile.unit().unwrap().unit_type()).is_some();
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

        max(max_defence, unit_defence) < description(unit_type).attack
    }

    fn prepare_moving_unit(
        &self,
        player_id: ID,
        src: Coord,
        dst: Coord,
    ) -> Result<(ID, u32, ID, bool, Option<ID>, Option<UnitType>), PlayerActionError> {
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

        let (need_relocation, old_unit_id_to_remove, upgrade_to) =
            self.prepare_placing_unit(player_id, region.id(), unit.unit_type(), dst)?;

        let distance = self.location.bfs_distance(src, dst, |c| {
            self.unit_can_step_on_coord(unit.unit_type(), c, region.id(), c == dst)
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
        let moves_to_subtract = if need_relocation || old_unit_id_to_remove.is_some() {
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
            upgrade_to,
        ))
    }

    fn move_unit(
        &mut self,
        player_id: ID,
        src: Coord,
        dst: Coord,
    ) -> Result<(), PlayerActionError> {
        let (unit_id, moves_num, region_id, need_relocation, old_unit_id_to_remove, upgrade_to) =
            self.prepare_moving_unit(player_id, src, dst)?;

        self.location.move_unit(src, dst)?;
        if need_relocation {
            self.add_tile_to_region(dst, region_id)?;
        }
        self.unit_info
            .get_mut(&unit_id)
            .unwrap()
            .subtract_moves(moves_num);
        let old_unit_info = if let Some(old_unit_id) = old_unit_id_to_remove {
            self.unit_info.remove(&old_unit_id)
        } else {
            None
        };
        if let Some(unit_type) = upgrade_to {
            self.maybe_remove_unit(dst).unwrap();
            let unit_id = self.create_and_place_unit(unit_type, dst).unwrap();
            let old_info = old_unit_info.unwrap();
            if old_info.moves_left() == old_info.description().max_moves {
                self.unit_info.get_mut(&unit_id).unwrap().refill_moves();
            }
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

        self.maybe_remove_unit(dst).unwrap();
        self.create_and_place_unit(upgraded_unit_type, dst)?;
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

    fn replace_graves_with_pine_trees(&mut self) {
        let mut existing_graves = Vec::new();
        for (&coord, tile) in self.location.map().iter() {
            if tile
                .unit()
                .map_or(false, |u| u.unit_type() == UnitType::Grave)
            {
                existing_graves.push(coord);
            }
        }
        for coordinate in existing_graves.into_iter() {
            self.maybe_remove_unit(coordinate).unwrap();
            self.create_and_place_unit(UnitType::PineTree, coordinate)
                .unwrap();
        }
    }

    fn apply_income(&mut self) {
        for (id, region) in self.location.regions() {
            if region.coordinates().len() < MIN_CONTROLLED_REGION_SIZE {
                let c = *region.coordinates().iter().next().unwrap();
                if self.location().tile_at(c).unwrap().unit().is_none() {
                    continue;
                }
            }
            let info = self.region_info.get_mut(id).unwrap();
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
            .filter_map(|&c| self.location.tile_at(c).unwrap().unit().map(|u| (c, u)))
            .filter(|(_, u)| {
                let d = description(u.unit_type());
                // We don't kill units that are not owned by player and the ones that have no turn cost
                !d.is_unownable && d.turn_cost > 0
            }).map(|(c, _)| c)
            .collect();
        for coordinate in kill_coordinates.into_iter() {
            self.maybe_remove_unit(coordinate).unwrap();
            self.create_and_place_unit(UnitType::Grave, coordinate)
                .unwrap();
        }
    }

    fn tree_for(&self, coordinate: Coord) -> Option<UnitType> {
        if self.current_turn % 2 != 0 && self.current_turn % 5 == 0 {
            return None;
        }
        let neighbours = coordinate.neighbors();
        let (water_num, trees_num) = neighbours
            .iter()
            .map(|n| {
                let tile = self.location.tile_at(*n);
                let is_water = tile.is_none() || tile.unwrap().surface().is_water();
                let has_tree = tile.is_some() && tile.unwrap().unit().map_or(false, |u| {
                    u.unit_type() == UnitType::PalmTree || u.unit_type() == UnitType::PineTree
                });
                (if is_water { 1 } else { 0 }, if has_tree { 1 } else { 0 })
            }).fold((0, 0), |(aa, ab), (ca, cb)| (aa + ca, ab + cb));
        if water_num > 0 && trees_num >= 1 && self.current_turn % 5 != 0 {
            Some(UnitType::PalmTree)
        } else if trees_num >= 2 && self.current_turn % 2 == 0 {
            Some(UnitType::PineTree)
        } else {
            None
        }
    }

    fn add_tree(&mut self, coordinates: Vec<Coord>, unit_type: UnitType) {
        for c in coordinates {
            self.create_and_place_unit(unit_type, c).unwrap();
        }
    }

    fn spread_forests(&mut self) {
        // Each second turn a pine tree grows on each tile that has two or more neighbours with trees
        // Each turn (skipping each fifths turn) a palm tree grows on each tile that has one
        // neighbour with tree and one neighbor with water.
        // Everything over the maps border is assumed to be water BTW
        let mut coordinates_for_palms: Vec<Coord> = Vec::new();
        let mut coordinates_for_pines: Vec<Coord> = Vec::new();
        for (&c, tile) in self.location.map() {
            if tile.surface().is_water() || tile.unit().is_some() {
                continue;
            }
            if let Some(tree_type) = self.tree_for(c) {
                match tree_type {
                    UnitType::PineTree => coordinates_for_pines.push(c),
                    UnitType::PalmTree => coordinates_for_palms.push(c),
                    _ => (),
                }
            }
        }
        self.add_tree(coordinates_for_palms, UnitType::PalmTree);
        self.add_tree(coordinates_for_pines, UnitType::PineTree);
    }

    fn end_turn(&mut self) {
        // Set of end-of-turn actions. Order is important.
        self.apply_income();
        self.refill_moves();
        self.spread_forests();
        self.replace_graves_with_pine_trees();
        self.kill_starving_units();
        self.check_for_active_players();
        self.check_for_winner();

        // Now we can change turn number and find next active player to move
        self.current_turn += 1;
        self.active_player_num = 0;
        self.rewind_to_active_player();
    }
}

#[cfg(test)]
mod test {
    use super::{GameEngine, PlayerAction, PlayerActionError};
    use game::consts::*;
    use game::ids::ID;
    use game::location::{Coord, Player, UnitType};
    use game::test_util::create_valid_engine;
    use game::unit::description;

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
    fn place_new_unit_simple_tile_not_placeable() {
        let (pl, ri, mut game_engine) = create_valid_engine();
        let coordinate = Coord::new(2, -1);

        let action = PlayerAction::PlaceNewUnit(ri[0], UnitType::Grave, coordinate);
        let res = game_engine.act(pl[0].id(), action);

        assert_eq!(
            res,
            Err(PlayerActionError::CannotBePlacedByPlayer(UnitType::Grave))
        );
        assert_eq!(
            game_engine.region_money(ri[0]),
            Some(CONTROLLED_REGION_STARTING_MONEY)
        );
    }

    #[test]
    fn place_new_unit_simple_tile_has_other_replaceable_unit() {
        let (pl, ri, mut game_engine) = create_valid_engine();
        let coordinate = Coord::new(2, -1);
        game_engine
            .create_and_place_unit(UnitType::Grave, coordinate)
            .unwrap();

        let action = PlayerAction::PlaceNewUnit(ri[0], UnitType::Militia, coordinate);
        let res = game_engine.act(pl[0].id(), action);

        assert_eq!(res, Ok(()));
        assert_eq!(
            game_engine.region_money(ri[0]),
            Some(CONTROLLED_REGION_STARTING_MONEY - description(UnitType::Militia).purchase_cost)
        );
    }

    #[test]
    fn place_new_unit_simple_tile_has_other_non_replaceable_unit() {
        let (pl, ri, mut game_engine) = create_valid_engine();
        let coordinate = Coord::new(1, -1);

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

        // And one more, so we have no more moves after
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

        // And now we will get error, because there are no moves left
        let (src, dst) = (Coord::new(0, -1), Coord::new(1, 0));
        let action = PlayerAction::MoveUnit { src, dst };
        let res = game_engine.act(pl[0].id(), action);

        assert_eq!(res, Err(PlayerActionError::NotEnoughMoves(1, 2)));
        assert_eq!(game_engine.location().tile_at(dst).unwrap().unit(), None);

        let unit = game_engine.location().tile_at(src).unwrap().unit().unwrap();
        let info = game_engine.unit_info(unit.id());
        assert_eq!(unit.unit_type(), UnitType::Soldier);
        assert_eq!(info.moves_left(), 1);
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

    #[test]
    fn move_unit_and_merge_all_ok_goal_not_moved_before() {
        let (pl, _, mut game_engine) = create_valid_engine();
        let dst = Coord::new(2, -1);
        let unit_id = game_engine
            .create_and_place_unit(UnitType::Militia, dst)
            .unwrap();
        game_engine
            .unit_info
            .get_mut(&unit_id)
            .unwrap()
            .refill_moves();

        let src = Coord::new(1, 0);
        let action = PlayerAction::MoveUnit { src, dst };
        let res = game_engine.act(pl[0].id(), action);

        assert_eq!(res, Ok(()));

        let unit = game_engine.location().tile_at(dst).unwrap().unit().unwrap();
        let info = game_engine.unit_info(unit.id());
        assert_eq!(unit.unit_type(), UnitType::Knight);
        assert_eq!(info.moves_left(), STANDARD_MOVES_NUM);
    }

    #[test]
    fn move_unit_and_merge_all_ok_goal_moved_before() {
        let (pl, _, mut game_engine) = create_valid_engine();
        let dst = Coord::new(2, -1);
        game_engine
            .create_and_place_unit(UnitType::Militia, dst)
            .unwrap();

        let src = Coord::new(1, 0);
        let action = PlayerAction::MoveUnit { src, dst };
        let res = game_engine.act(pl[0].id(), action);

        assert_eq!(res, Ok(()));

        let unit = game_engine.location().tile_at(dst).unwrap().unit().unwrap();
        let info = game_engine.unit_info(unit.id());
        assert_eq!(unit.unit_type(), UnitType::Knight);
        assert_eq!(info.moves_left(), 0);
    }

    #[test]
    fn move_unit_and_merge_no_result_error() {
        let (pl, _, mut game_engine) = create_valid_engine();
        let dst = Coord::new(2, -1);
        game_engine
            .create_and_place_unit(UnitType::Knight, dst)
            .unwrap();

        let src = Coord::new(1, 0);
        let action = PlayerAction::MoveUnit { src, dst };
        let res = game_engine.act(pl[0].id(), action);

        assert_eq!(res, Err(PlayerActionError::InaccessibleLocation(dst)));

        let unit = game_engine.location().tile_at(dst).unwrap().unit().unwrap();
        let info = game_engine.unit_info(unit.id());
        assert_eq!(unit.unit_type(), UnitType::Knight);
        assert_eq!(info.moves_left(), 0);

        let unit = game_engine.location().tile_at(src).unwrap().unit().unwrap();
        let info = game_engine.unit_info(unit.id());
        assert_eq!(unit.unit_type(), UnitType::Soldier);
        assert_eq!(info.moves_left(), STANDARD_MOVES_NUM);
    }

    #[test]
    fn move_unit_on_grave() {
        let (pl, _, mut game_engine) = create_valid_engine();
        let dst = Coord::new(2, -1);
        game_engine
            .create_and_place_unit(UnitType::Grave, dst)
            .unwrap();

        let src = Coord::new(1, 0);
        let action = PlayerAction::MoveUnit { src, dst };
        let res = game_engine.act(pl[0].id(), action);

        assert_eq!(res, Ok(()));

        let unit = game_engine.location().tile_at(dst).unwrap().unit().unwrap();
        let info = game_engine.unit_info(unit.id());
        assert_eq!(unit.unit_type(), UnitType::Soldier);
        assert_eq!(info.moves_left(), 0);
    }

    #[test]
    fn move_unit_on_tree() {
        let (pl, _, mut game_engine) = create_valid_engine();
        let dst = Coord::new(2, -1);
        game_engine
            .create_and_place_unit(UnitType::PineTree, dst)
            .unwrap();

        let src = Coord::new(1, 0);
        let action = PlayerAction::MoveUnit { src, dst };
        let res = game_engine.act(pl[0].id(), action);

        assert_eq!(res, Ok(()));

        let unit = game_engine.location().tile_at(dst).unwrap().unit().unwrap();
        let info = game_engine.unit_info(unit.id());
        assert_eq!(unit.unit_type(), UnitType::Soldier);
        assert_eq!(info.moves_left(), 0);
    }

    #[test]
    fn end_players_turn_changes_player() {
        let (pl, _, mut game_engine) = create_valid_engine();
        let action = PlayerAction::EndTurn;
        let res = game_engine.act(pl[0].id(), action);

        assert_eq!(res, Ok(()));
        assert_eq!(game_engine.current_turn(), 1);
        assert_eq!(*game_engine.active_player(), pl[1]);
        assert_eq!(
            game_engine.act(pl[0].id(), action),
            Err(PlayerActionError::OtherPlayersTurn(pl[1].id()))
        );
    }

    #[test]
    fn end_players_turn_skips_inactive() {
        let (pl, ri, mut game_engine) = create_valid_engine();
        game_engine.modify_money(ri[0], description(UnitType::Soldier).purchase_cost);
        game_engine
            .act(
                pl[0].id(),
                PlayerAction::MoveUnit {
                    src: Coord::new(1, 0),
                    dst: Coord::new(0, 1),
                },
            ).unwrap();
        game_engine
            .act(
                pl[0].id(),
                PlayerAction::PlaceNewUnit(ri[0], UnitType::Soldier, Coord::new(2, 0)),
            ).unwrap();
        let res = game_engine.act(pl[0].id(), PlayerAction::EndTurn);

        assert_eq!(res, Ok(()));
        assert_eq!(game_engine.current_turn(), 1);
        assert_eq!(*game_engine.active_player(), pl[2]);
    }

    #[test]
    fn end_players_turn_from_last_player_changes_turn() {
        let (pl, ri, mut game_engine) = create_valid_engine();
        game_engine.act(pl[0].id(), PlayerAction::EndTurn).unwrap();
        game_engine.act(pl[1].id(), PlayerAction::EndTurn).unwrap();
        game_engine.act(pl[2].id(), PlayerAction::EndTurn).unwrap();

        assert_eq!(game_engine.current_turn(), 2);
        assert_eq!(*game_engine.active_player(), pl[0]);

        let soldier = game_engine
            .location()
            .tile_at(Coord::new(1, 0))
            .unwrap()
            .unit()
            .unwrap();
        let info = game_engine.unit_info(soldier.id());
        assert_eq!(info.moves_left(), description(UnitType::Soldier).max_moves);

        let militia = game_engine
            .location()
            .tile_at(Coord::new(0, 1))
            .unwrap()
            .unit()
            .unwrap();
        let info = game_engine.unit_info(militia.id());
        assert_eq!(info.moves_left(), description(UnitType::Militia).max_moves);

        assert_eq!(game_engine.region_money(ri[0]), Some(8));
        assert_eq!(game_engine.region_money(ri[1]), Some(11));
        assert_eq!(game_engine.region_money(ri[2]), Some(0));
        assert_eq!(game_engine.region_money(ri[3]), Some(12));
    }

    #[test]
    fn end_turn_makes_active_player_inactive_if_nothing_left() {
        let (pl, _ri, mut game_engine) = create_valid_engine();
        game_engine
            .act(
                pl[0].id(),
                PlayerAction::MoveUnit {
                    src: Coord::new(1, 0),
                    dst: Coord::new(1, 1),
                },
            ).unwrap();
        game_engine.act(pl[0].id(), PlayerAction::EndTurn).unwrap();
        game_engine.act(pl[1].id(), PlayerAction::EndTurn).unwrap();

        // Before changing turn pl[1] should be active
        assert_eq!(game_engine.player_activity[&pl[1].id()], true);

        game_engine.act(pl[2].id(), PlayerAction::EndTurn).unwrap();

        assert_eq!(game_engine.current_turn(), 2);
        assert_eq!(*game_engine.active_player(), pl[0]);
        assert_eq!(game_engine.player_activity[&pl[1].id()], false);

        let grave = game_engine
            .location
            .tile_at(Coord::new(0, 1))
            .unwrap()
            .unit();
        assert!(grave.is_some());
        assert_eq!(grave.unwrap().unit_type(), UnitType::Grave);
    }

    #[test]
    fn end_turn_skips_first_player_if_he_is_inactive() {
        let (pl, ri, mut game_engine) = create_valid_engine();
        game_engine.modify_money(ri[1], description(UnitType::Knight).purchase_cost * 2);

        game_engine.act(pl[0].id(), PlayerAction::EndTurn).unwrap();
        game_engine
            .act(
                pl[1].id(),
                PlayerAction::PlaceNewUnit(ri[1], UnitType::Knight, Coord::new(1, 0)),
            ).unwrap();
        game_engine
            .act(
                pl[1].id(),
                PlayerAction::PlaceNewUnit(ri[1], UnitType::Knight, Coord::new(1, -1)),
            ).unwrap();
        game_engine.act(pl[1].id(), PlayerAction::EndTurn).unwrap();
        game_engine.act(pl[2].id(), PlayerAction::EndTurn).unwrap();

        assert_eq!(game_engine.current_turn(), 2);
        assert_eq!(*game_engine.active_player(), pl[1]);
    }

    #[test]
    fn end_turn_selects_winner_if_any() {
        let (pl, ri, mut game_engine) = create_valid_engine();
        game_engine.modify_money(ri[0], description(UnitType::Soldier).purchase_cost);
        game_engine
            .act(
                pl[0].id(),
                PlayerAction::MoveUnit {
                    src: Coord::new(1, 0),
                    dst: Coord::new(1, 1),
                },
            ).unwrap();
        game_engine
            .act(
                pl[0].id(),
                PlayerAction::PlaceNewUnit(ri[0], UnitType::Soldier, Coord::new(-1, 0)),
            ).unwrap();
        game_engine.act(pl[0].id(), PlayerAction::EndTurn).unwrap();
        game_engine.act(pl[1].id(), PlayerAction::EndTurn).unwrap();

        assert_eq!(game_engine.current_turn(), 2);
        assert_eq!(*game_engine.active_player(), pl[0]);
        assert_eq!(game_engine.winner(), Some(pl[0].id()));
    }

    #[test]
    fn end_turn_spawns_graves_if_units_die_from_starvation() {
        let (pl, ri, mut game_engine) = create_valid_engine();
        game_engine.modify_money(ri[0], -CONTROLLED_REGION_STARTING_MONEY);
        game_engine.act(pl[0].id(), PlayerAction::EndTurn).unwrap();
        game_engine.act(pl[1].id(), PlayerAction::EndTurn).unwrap();
        game_engine.act(pl[2].id(), PlayerAction::EndTurn).unwrap();

        assert_eq!(game_engine.current_turn(), 2);
        assert_eq!(*game_engine.active_player(), pl[0]);

        let grave = game_engine
            .location
            .tile_at(Coord::new(1, 0))
            .unwrap()
            .unit();
        assert!(grave.is_some());
        assert_eq!(grave.unwrap().unit_type(), UnitType::Grave);
    }

    #[test]
    fn end_turn_spawns_trees_on_top_of_graves() {
        let (pl, _ri, mut game_engine) = create_valid_engine();
        let coordinate = Coord::new(2, -1);
        game_engine
            .create_and_place_unit(UnitType::Grave, coordinate)
            .unwrap();
        game_engine.act(pl[0].id(), PlayerAction::EndTurn).unwrap();
        game_engine.act(pl[1].id(), PlayerAction::EndTurn).unwrap();
        game_engine.act(pl[2].id(), PlayerAction::EndTurn).unwrap();

        assert_eq!(game_engine.current_turn(), 2);
        assert_eq!(*game_engine.active_player(), pl[0]);

        let tree = game_engine.location.tile_at(coordinate).unwrap().unit();
        assert!(tree.is_some());
        assert_eq!(tree.unwrap().unit_type(), UnitType::PineTree);
    }

    #[test]
    fn end_turn_dont_spreads_trees_on_existing_units() {
        let (pl, _ri, mut game_engine) = create_valid_engine();
        let coordinate = Coord::new(1, 0);
        game_engine.maybe_remove_unit(coordinate).unwrap();
        game_engine
            .create_and_place_unit(UnitType::PalmTree, coordinate)
            .unwrap();

        game_engine.act(pl[0].id(), PlayerAction::EndTurn).unwrap();
        game_engine.act(pl[1].id(), PlayerAction::EndTurn).unwrap();
        game_engine.act(pl[2].id(), PlayerAction::EndTurn).unwrap();

        assert_eq!(game_engine.current_turn(), 2);
        assert_eq!(*game_engine.active_player(), pl[0]);

        let tree = game_engine
            .location()
            .tile_at(coordinate)
            .unwrap()
            .unit()
            .unwrap();
        assert_eq!(tree.unit_type(), UnitType::PalmTree);

        let tree = game_engine
            .location()
            .tile_at(Coord::new(2, -1))
            .unwrap()
            .unit()
            .unwrap();
        assert_eq!(tree.unit_type(), UnitType::PalmTree);

        let tree = game_engine
            .location()
            .tile_at(Coord::new(1, 1))
            .unwrap()
            .unit()
            .unwrap();
        assert_eq!(tree.unit_type(), UnitType::PalmTree);

        let unit = game_engine
            .location()
            .tile_at(Coord::new(0, 1))
            .unwrap()
            .unit()
            .unwrap();
        assert_eq!(unit.unit_type(), UnitType::Militia);
    }

    #[test]
    fn capital_replaces_unit_if_old_one_is_removed_and_there_is_no_space_left() {
        let (pl, _ri, mut game_engine) = create_valid_engine();
        game_engine
            .create_and_place_unit(UnitType::Militia, Coord::new(1, 1))
            .unwrap();
        game_engine
            .act(
                pl[0].id(),
                PlayerAction::MoveUnit {
                    src: Coord::new(1, 0),
                    dst: Coord::new(2, 0),
                },
            ).unwrap();

        let unit_one = game_engine
            .location()
            .tile_at(Coord::new(1, 1))
            .unwrap()
            .unit()
            .unwrap();
        let unit_two = game_engine
            .location()
            .tile_at(Coord::new(0, 1))
            .unwrap()
            .unit()
            .unwrap();
        assert!(
            unit_one.unit_type() == UnitType::Village || unit_two.unit_type() == UnitType::Village
        );
        assert!(
            unit_one.unit_type() == UnitType::Militia || unit_two.unit_type() == UnitType::Militia
        );
    }

    #[test]
    fn upgrade_unit_all_ok() {
        let (pl, ri, mut game_engine) = create_valid_engine();
        let coordinate = Coord::new(1, 0);
        let action = PlayerAction::UpgradeUnit(coordinate);
        let res = game_engine.act(pl[0].id(), action);

        assert_eq!(res, Ok(()));
        assert_eq!(game_engine.region_money(ri[0]), Some(0));

        let unit = game_engine
            .location()
            .tile_at(coordinate)
            .unwrap()
            .unit()
            .unwrap();
        let info = game_engine.unit_info(unit.id());
        assert_eq!(unit.unit_type(), UnitType::Knight);
        assert_eq!(info.moves_left(), 0);
    }

    #[test]
    fn upgrade_unit_not_upgradable_error() {
        let (pl, ri, mut game_engine) = create_valid_engine();
        let coordinate = Coord::new(1, -1);
        let action = PlayerAction::UpgradeUnit(coordinate);
        let res = game_engine.act(pl[0].id(), action);

        assert_eq!(res, Err(PlayerActionError::NoUpgrade(UnitType::Village)));
        assert_eq!(
            game_engine.region_money(ri[0]),
            Some(CONTROLLED_REGION_STARTING_MONEY)
        );

        let unit = game_engine
            .location()
            .tile_at(coordinate)
            .unwrap()
            .unit()
            .unwrap();
        assert_eq!(unit.unit_type(), UnitType::Village);
    }

    #[test]
    fn upgrade_unit_of_other_player_error() {
        let (pl, ri, mut game_engine) = create_valid_engine();

        let coordinate = Coord::new(0, 1);
        let action = PlayerAction::UpgradeUnit(coordinate);
        let res = game_engine.act(pl[0].id(), action);

        assert_eq!(res, Err(PlayerActionError::NotOwned(coordinate)));
        assert_eq!(
            game_engine.region_money(ri[0]),
            Some(CONTROLLED_REGION_STARTING_MONEY)
        );
        assert_eq!(
            game_engine.region_money(ri[1]),
            Some(CONTROLLED_REGION_STARTING_MONEY)
        );

        let unit = game_engine
            .location()
            .tile_at(coordinate)
            .unwrap()
            .unit()
            .unwrap();
        assert_eq!(unit.unit_type(), UnitType::Militia);
    }

    #[test]
    fn upgrade_unit_no_unit_error() {
        let (pl, ri, mut game_engine) = create_valid_engine();
        let coordinate = Coord::new(2, -1);
        let action = PlayerAction::UpgradeUnit(coordinate);
        let res = game_engine.act(pl[0].id(), action);

        assert_eq!(res, Err(PlayerActionError::NoUnit(coordinate)));
        assert_eq!(
            game_engine.region_money(ri[0]),
            Some(CONTROLLED_REGION_STARTING_MONEY)
        );

        let unit = game_engine.location().tile_at(coordinate).unwrap().unit();
        assert_eq!(unit, None);
    }

    #[test]
    fn upgrade_unit_no_money_error() {
        let (pl, ri, mut game_engine) = create_valid_engine();
        game_engine.modify_money(ri[0], -5);

        let coordinate = Coord::new(1, 0);
        let action = PlayerAction::UpgradeUnit(coordinate);
        let res = game_engine.act(pl[0].id(), action);

        assert_eq!(res, Err(PlayerActionError::NotEnoughMoney(ri[0])));
        assert_eq!(
            game_engine.region_money(ri[0]),
            Some(CONTROLLED_REGION_STARTING_MONEY - 5)
        );

        let unit = game_engine
            .location()
            .tile_at(coordinate)
            .unwrap()
            .unit()
            .unwrap();
        assert_eq!(unit.unit_type(), UnitType::Soldier);
    }
}
