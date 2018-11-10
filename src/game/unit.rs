use super::consts::*;
use super::ids::ID;
use super::location::{Tile, Unit, UnitType};

#[derive(Eq, PartialEq, Hash, Debug, Ord, PartialOrd)]
pub struct UnitDescription {
    pub name: UnitType,
    pub is_purchasable: bool,
    pub purchase_cost: i32,
    pub turn_cost: i32,
    pub max_moves: u32,
    pub defence: u8,
    pub attack: u8,
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
    /// use yasc::game::unit::{UnitInfo};
    /// use yasc::game::location::{UnitType};
    ///
    /// let (_, mut unit) = UnitInfo::new(1, UnitType::Soldier);
    /// unit.refill_moves();
    /// assert_eq!(unit.moves_left(), 5);
    /// unit.subtract_moves(3);
    /// assert_eq!(unit.moves_left(), 2);
    /// ```
    ///
    /// # Panics:
    ///
    /// This method assumed that you checked that unit has enough moves before subtracting them
    /// and will panic if you will try to subtract more moves than available at the moment:
    ///
    /// ```rust,should_panic
    /// use yasc::game::unit::{UnitInfo};
    /// use yasc::game::location::{UnitType};
    ///
    /// let (_, mut unit) = UnitInfo::new(1, UnitType::GreatKnight);
    /// unit.refill_moves();
    /// assert_eq!(unit.moves_left(), 5);
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
    /// use yasc::game::unit::{UnitInfo};
    /// use yasc::game::location::{UnitType};
    ///
    /// let (_, mut unit) = UnitInfo::new(1, UnitType::Soldier);
    /// assert_eq!(unit.moves_left(), 0);
    /// unit.refill_moves();
    /// assert_eq!(unit.moves_left(), 5);
    /// ```
    ///
    pub fn refill_moves(&mut self) {
        self.moves_left = self.description.max_moves;
    }
}

/// Return true if this unit can defeat unit provided as argument
///
/// # Examples:
///
/// ```rust
/// use yasc::game::unit::{can_defeat};
/// use yasc::game::location::{Unit, UnitType};
///
/// let soldier = Unit::new(1, UnitType::Soldier);
/// let knight = Unit::new(1, UnitType::Knight);
///
/// assert_eq!(can_defeat(soldier, knight), false);
/// assert_eq!(can_defeat(knight, soldier), true);
/// assert_eq!(can_defeat(soldier, soldier), false);
/// ```
///
pub fn can_defeat(attacker: Unit, defender: Unit) -> bool {
    description(attacker.unit_type()).attack > description(defender.unit_type()).defence
}

/// Return true if unit can step on the tile
pub fn can_step_on(_unit: Unit, tile: &Tile) -> bool {
    tile.surface().is_land()
}

/// Return a description of unit identified by enum entry
///
/// # Examples:
///
/// ```
/// use yasc::game::unit::{description};
/// use yasc::game::location::{UnitType};
///
/// let desc = description(UnitType::Grave);
/// assert_eq!(desc.name, UnitType::Grave);
/// ```
///
pub fn description(unit_type: UnitType) -> &'static UnitDescription {
    match unit_type {
        UnitType::Grave => &GRAVE,
        UnitType::Tree => &TREE,
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
    use super::{can_defeat, description, Unit, UnitInfo, UnitType};

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
        assert_eq!(unit.moves_left(), 5);
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
        assert_eq!(unit.moves_left(), 2);
    }

    #[test]
    #[should_panic]
    fn subtract_moves_panics_when_no_moves_left() {
        let (_, mut unit) = UnitInfo::new(1, UnitType::Soldier);
        unit.refill_moves();
        unit.subtract_moves(6);
    }

    #[test]
    fn can_defeat_when_unit_stronger() {
        let unit = Unit::new(1, UnitType::Soldier);
        let other = Unit::new(1, UnitType::Militia);
        assert!(can_defeat(unit, other));
    }

    #[test]
    fn can_defeat_when_unit_weaker() {
        let unit = Unit::new(1, UnitType::Soldier);
        let other = Unit::new(1, UnitType::GreatKnight);
        assert!(!can_defeat(unit, other));
    }

    #[test]
    fn can_defeat_when_unit_equal() {
        let unit = Unit::new(1, UnitType::Soldier);
        let other = Unit::new(1, UnitType::Soldier);
        assert!(!can_defeat(unit, other));
    }

    #[test]
    fn description_is_correct() {
        let desc = description(UnitType::Grave);
        assert_eq!(desc.name, UnitType::Grave);
    }
}
