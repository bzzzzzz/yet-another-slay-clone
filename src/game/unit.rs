use super::consts::*;
use super::ids::ID;
use super::location::{Tile, Unit, UnitType};

#[derive(Eq, PartialEq, Hash, Debug, Ord, PartialOrd)]
pub struct UnitDescription {
    pub name: UnitType,
    pub is_unownable: bool,
    pub is_purchasable: bool,
    pub purchase_cost: i32,
    pub turn_cost: i32,
    pub max_moves: u32,
    pub defence: u8,
    pub attack: u8,
    pub upgrade_levels: u8,
    pub upgrades_to: Option<&'static UnitDescription>,
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Ord, PartialOrd)]
pub struct UnitInfo {
    description: &'static UnitDescription,
    moves_left: u32,
}

impl UnitInfo {
    pub fn from(unit: Unit) -> Self {
        let description = description(unit.unit_type());
        // Unit can move only on the next turn after its creation
        let moves_left = 0;
        Self {
            description,
            moves_left,
        }
    }

    pub fn new(id: ID, unit_type: UnitType) -> (Unit, Self) {
        let unit = Unit::new(id, unit_type);
        let info = Self::from(unit);

        (unit, info)
    }

    pub fn moves_left(&self) -> u32 {
        self.moves_left
    }

    pub fn description(&self) -> &'static UnitDescription {
        self.description
    }

    /// Subtract moves from this unit
    ///
    /// # Examples:
    ///
    /// ```rust
    /// use yasc::game::{UnitType, UnitInfo};
    ///
    /// let (_, mut unit) = UnitInfo::new(1, UnitType::Soldier);
    /// unit.refill_moves();
    /// assert_eq!(unit.moves_left(), 4);
    /// unit.subtract_moves(3);
    /// assert_eq!(unit.moves_left(), 1);
    /// ```
    ///
    /// # Panics:
    ///
    /// This method assumed that you checked that unit has enough moves before subtracting them
    /// and will panic if you will try to subtract more moves than available at the moment:
    ///
    /// ```rust,should_panic
    /// use yasc::game::{UnitType, UnitInfo};
    ///
    /// let (_, mut unit) = UnitInfo::new(1, UnitType::GreatKnight);
    /// unit.refill_moves();
    /// assert_eq!(unit.moves_left(), 4);
    /// unit.subtract_moves(6);
    /// ```
    ///
    pub fn subtract_moves(&mut self, amount: u32) {
        if amount > self.moves_left {
            panic!("Trying to subtract more moves than unit has");
        }
        self.moves_left -= amount;
    }

    /// Refill moves to the maximal possible amount for this type of unit
    ///
    /// # Examples:
    ///
    /// ```rust
    /// use yasc::game::{UnitType, UnitInfo};
    ///
    /// let (_, mut unit) = UnitInfo::new(1, UnitType::Soldier);
    /// assert_eq!(unit.moves_left(), 0);
    /// unit.refill_moves();
    /// assert_eq!(unit.moves_left(), 4);
    /// ```
    ///
    pub fn refill_moves(&mut self) {
        self.moves_left = self.description.max_moves;
    }
}

/// Return true if this unit can defeat unit provided as argument
pub fn can_defeat(attacker: UnitType, defender: UnitType) -> bool {
    description(attacker).attack > description(defender).defence
}

/// Return true if unit can step on the tile
pub fn can_step_on(_unit_type: UnitType, tile: &Tile) -> bool {
    tile.surface().is_land()
}

/// Return a possible result of merging actor into goal (or replacing goal with actor)
/// If merge is impossible for any reason, return `None`
pub fn merge_result(actor: UnitType, goal: UnitType) -> Option<UnitType> {
    let goal_description = description(goal);

    if goal_description.is_unownable {
        Some(actor)
    } else if goal_description.upgrades_to.is_some() {
        let actor_description = description(actor);
        if actor_description.upgrade_levels == 0 {
            None
        } else {
            let mut result = Some(goal_description);
            for _ in 0..actor_description.upgrade_levels {
                result = result.and_then(|r| r.upgrades_to)
            }
            result.map(|d| d.name)
        }
    } else {
        None
    }
}

/// Return a description of unit identified by enum entry
pub fn description(unit_type: UnitType) -> &'static UnitDescription {
    match unit_type {
        UnitType::Grave => &GRAVE,
        UnitType::PineTree => &PINE_TREE,
        UnitType::PalmTree => &PALM_TREE,
        UnitType::Village => &VILLAGE,
        UnitType::Tower => &TOWER,
        UnitType::GreatKnight => &GREAT_KNIGHT,
        UnitType::Knight => &KNIGHT,
        UnitType::Soldier => &SOLDIER,
        UnitType::Militia => &MILITIA,
    }
}

#[cfg(test)]
mod test {
    use super::super::consts::*;
    use super::{can_defeat, description, merge_result, UnitInfo, UnitType};

    #[test]
    fn check_description() {
        let desc = description(UnitType::Grave);
        assert_eq!(desc.name, UnitType::Grave);
    }

    #[test]
    fn merge_result_check() {
        assert_eq!(merge_result(UnitType::Soldier, UnitType::Village), None);
        assert_eq!(
            merge_result(UnitType::Soldier, UnitType::Militia),
            Some(UnitType::Knight)
        );
        assert_eq!(
            merge_result(UnitType::Militia, UnitType::Grave),
            Some(UnitType::Militia)
        );
        assert_eq!(
            merge_result(UnitType::Soldier, UnitType::Soldier),
            Some(UnitType::GreatKnight)
        );
        assert_eq!(merge_result(UnitType::Soldier, UnitType::Knight), None);
        assert_eq!(merge_result(UnitType::Grave, UnitType::Militia), None);
    }

    #[test]
    fn check_can_defeat() {
        assert_eq!(can_defeat(UnitType::Soldier, UnitType::Knight), false);
        assert_eq!(can_defeat(UnitType::Knight, UnitType::Soldier), true);
        assert_eq!(can_defeat(UnitType::Soldier, UnitType::Soldier), false);
    }

    #[test]
    fn unit_has_no_moves_when_created() {
        let (_, unit) = UnitInfo::new(1, UnitType::Soldier);
        assert_eq!(unit.moves_left(), 0);
        let (_, unit) = UnitInfo::new(1, UnitType::GreatKnight);
        assert_eq!(unit.moves_left(), 0);
        let (_, unit) = UnitInfo::new(1, UnitType::Village);
        assert_eq!(unit.moves_left(), 0);
    }

    #[test]
    fn unit_has_max_moves_when_refilled() {
        let (_, mut unit) = UnitInfo::new(1, UnitType::Soldier);
        unit.refill_moves();
        assert_eq!(unit.moves_left(), STANDARD_MOVES_NUM);
    }

    #[test]
    fn building_unit_always_has_zero_moves() {
        let (_, mut unit) = UnitInfo::new(1, UnitType::Tower);
        unit.refill_moves();
        assert_eq!(unit.moves_left(), 0);
    }

    #[test]
    fn subtract_moves_changes_moves_left() {
        let (_, mut unit) = UnitInfo::new(1, UnitType::Soldier);
        unit.refill_moves();
        unit.subtract_moves(3);
        assert_eq!(unit.moves_left(), STANDARD_MOVES_NUM - 3);
    }

    #[test]
    #[should_panic]
    fn subtract_moves_panics_when_no_moves_left() {
        let (_, mut unit) = UnitInfo::new(1, UnitType::Soldier);
        unit.refill_moves();
        unit.subtract_moves(STANDARD_MOVES_NUM + 1);
    }

    #[test]
    fn can_defeat_when_unit_stronger() {
        assert!(can_defeat(UnitType::Soldier, UnitType::Militia));
    }

    #[test]
    fn can_defeat_when_unit_weaker() {
        assert!(!can_defeat(UnitType::Soldier, UnitType::GreatKnight));
    }

    #[test]
    fn can_defeat_when_unit_equal() {
        assert!(!can_defeat(UnitType::Soldier, UnitType::Soldier));
    }

    #[test]
    fn description_is_correct() {
        let desc = description(UnitType::Grave);
        assert_eq!(desc.name, UnitType::Grave);
    }
}
