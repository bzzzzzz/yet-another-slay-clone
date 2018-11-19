mod consts;
mod engine;
mod ids;
mod location;
mod rules;
pub mod test_util;
mod unit;

pub use self::engine::{EngineValidationError, GameEngine, PlayerAction, PlayerActionError};
pub use self::ids::{IdProducer, ID};
pub use self::location::{
    Coord, Location, LocationModificationError, LocationValidationError, Player, Region, Tile,
    TileSurface, Unit, UnitType,
};
pub use self::rules::{validate_location, validate_regions, LocationRulesValidationError};
pub use self::unit::{UnitDescription, UnitInfo};
