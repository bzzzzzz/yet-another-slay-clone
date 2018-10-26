use super::ids::ID;

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Ord, PartialOrd)]
pub enum UnitType {
    Grave, Tree, Village, Tower, GreatKnight, Knight, Soldier, Militia,
}

#[derive(Eq, PartialEq, Hash, Debug, Ord, PartialOrd)]
pub struct UnitDescription {
    pub name: UnitType,
    pub is_purchasable: bool,
    pub purchase_cost: i32,
    pub turn_cost: i32,
    pub max_moves: u32,
    pub defence: u8,
    pub attack: u8,
    pub upgrades_to: Option<&'static UnitDescription>
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Ord, PartialOrd)]
pub struct Unit {
    id: ID,
    description: &'static UnitDescription,
    moves_left: u32,
}


impl Unit {
    pub fn new(id: ID, unit_type: UnitType) -> Self {
        let description = description(unit_type);
        // Unit can move only on the next turn after its creation
        let moves_left = 0;
        Self { description, moves_left, id, }
    }

    pub fn id(&self) -> ID {
        self.id
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
    /// use yasc::game::unit::{Unit,UnitType};
    ///
    /// let mut unit = Unit::new(1, UnitType::Soldier);
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
    /// use yasc::game::unit::{Unit,UnitType};
    ///
    /// let mut unit = Unit::new(1, UnitType::GreatKnight);
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
    /// use yasc::game::unit::{Unit,UnitType};
    ///
    /// let mut unit = Unit::new(1, UnitType::Soldier);
    /// assert_eq!(unit.moves_left(), 0);
    /// unit.refill_moves();
    /// assert_eq!(unit.moves_left(), 5);
    /// ```
    ///
    pub fn refill_moves(&mut self) {
        self.moves_left = self.description.max_moves;
    }


    /// Return true if this unit can defeat unit provided as argument
    ///
    /// # Examples:
    ///
    /// ```rust
    /// use yasc::game::unit::{Unit,UnitType};
    ///
    /// let soldier = Unit::new(1, UnitType::Soldier);
    /// let knight = Unit::new(1, UnitType::Knight);
    ///
    /// assert_eq!(soldier.can_defeat(&knight), false);
    /// assert_eq!(knight.can_defeat(&soldier), true);
    /// assert_eq!(soldier.can_defeat(&soldier), false);
    /// ```
    ///
    pub fn can_defeat(&self, other: &Unit) -> bool {
        self.description.attack > other.description.defence
    }
}


pub const GRAVE: UnitDescription = UnitDescription {
    name: UnitType::Grave,
    is_purchasable: false,
    purchase_cost: 0,
    turn_cost: 0,
    max_moves: 0,
    defence: 0,
    attack: 0,
    upgrades_to: None,
};

pub const TREE: UnitDescription = UnitDescription {
    name: UnitType::Tree,
    is_purchasable: false,
    purchase_cost: 0,
    turn_cost: 1,
    max_moves: 0,
    defence: 0,
    attack: 0,
    upgrades_to: None,
};

pub const VILLAGE: UnitDescription = UnitDescription {
    name: UnitType::Village,
    is_purchasable: false,
    purchase_cost: 0,
    turn_cost: 0,
    max_moves: 0,
    defence: 1,
    attack: 0,
    upgrades_to: None,
};

pub const TOWER: UnitDescription = UnitDescription {
    name: UnitType::Tower,
    is_purchasable: true,
    purchase_cost: 10,
    turn_cost: 0,
    max_moves: 0,
    defence: 2,
    attack: 0,
    upgrades_to: None,
};

pub const GREAT_KNIGHT: UnitDescription = UnitDescription {
    name: UnitType::GreatKnight,
    is_purchasable: true,
    purchase_cost: 80,
    turn_cost: 4,
    max_moves: 5,
    defence: 4,
    attack: 4,
    upgrades_to: None,
};

pub const KNIGHT: UnitDescription = UnitDescription {
    name: UnitType::Knight,
    is_purchasable: true,
    purchase_cost: 40,
    turn_cost: 3,
    max_moves: 5,
    defence: 3,
    attack: 3,
    upgrades_to: Some(&GREAT_KNIGHT),
};

pub const SOLDIER: UnitDescription = UnitDescription {
    name: UnitType::Soldier,
    is_purchasable: true,
    purchase_cost: 20,
    turn_cost: 2,
    max_moves: 5,
    defence: 2,
    attack: 2,
    upgrades_to: Some(&KNIGHT),
};

pub const MILITIA: UnitDescription = UnitDescription {
    name: UnitType::Militia,
    is_purchasable: true,
    purchase_cost: 10,
    turn_cost: 1,
    max_moves: 5,
    defence: 1,
    attack: 1,
    upgrades_to: Some(&SOLDIER),
};

/// Return a description of unit identified by enum entry
///
/// # Examples:
///
/// ```
/// use yasc::game::unit::{description,UnitType};
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
    use super::{Unit, UnitType, description};

    #[test]
    fn unit_has_no_moves_when_created() {
        let unit = Unit::new(1, UnitType::Soldier);
        assert_eq!(unit.moves_left(), 0);
        let unit = Unit::new(1, UnitType::GreatKnight);
        assert_eq!(unit.moves_left(), 0);
        let unit = Unit::new(1, UnitType::Village);
        assert_eq!(unit.moves_left(), 0);
    }

    #[test]
    fn unit_has_max_moves_when_refilled() {
        let mut unit = Unit::new(1, UnitType::Soldier);
        unit.refill_moves();
        assert_eq!(unit.moves_left(), 5);
    }

    #[test]
    fn building_unit_always_has_zero_moves() {
        let mut unit = Unit::new(1, UnitType::Tower);
        unit.refill_moves();
        assert_eq!(unit.moves_left(), 0);
    }

    #[test]
    fn subtract_moves_changes_moves_left() {
        let mut unit = Unit::new(1, UnitType::Soldier);
        unit.refill_moves();
        unit.subtract_moves(3);
        assert_eq!(unit.moves_left(), 2);
    }

    #[test]
    #[should_panic]
    fn subtract_moves_panics_when_no_moves_left() {
        let mut unit = Unit::new(1, UnitType::Soldier);
        unit.refill_moves();
        unit.subtract_moves(6);
    }

    #[test]
    fn can_defeat_when_unit_stronger() {
        let unit = Unit::new(1, UnitType::Soldier);
        let other = Unit::new(1, UnitType::Militia);
        assert!(unit.can_defeat(&other));
    }

    #[test]
    fn can_defeat_when_unit_weaker() {
        let unit = Unit::new(1, UnitType::Soldier);
        let other = Unit::new(1, UnitType::GreatKnight);
        assert!(!unit.can_defeat(&other));
    }

    #[test]
    fn can_defeat_when_unit_equal() {
        let unit = Unit::new(1, UnitType::Soldier);
        let other = Unit::new(1, UnitType::Soldier);
        assert!(!unit.can_defeat(&other));
    }

    #[test]
    fn description_is_correct() {
        let desc = description(UnitType::Grave);
        assert_eq!(desc.name, UnitType::Grave);
    }
}
