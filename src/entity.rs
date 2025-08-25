use serde::{Deserialize, Serialize};

/// Description of an entity.
///
/// An entity's position is at the bottom center of its hitbox.
#[derive(Serialize, Deserialize, Debug, Default, Copy, Clone, PartialEq)]
pub struct Entity {
    /// Width of the entity's hitbox along the X and Z axes
    pub width: f64,
    /// Height of the entity's hitbox along the Y axis.
    pub height: f64,
    /// Whether the entity is a projectile, in which case it is possible to clip
    /// into the portal frame.
    pub is_projectile: bool,
}

impl Entity {
    /// Player entity
    pub const PLAYER: Self = Entity {
        width: 0.6,
        height: 1.8,
        is_projectile: false,
    };
    /// Ender pearl entity
    pub const ENDER_PEARL: Self = Entity {
        width: 0.25,
        height: 0.25,
        is_projectile: true,
    };
    /// Arrow entity
    pub const ARROW: Self = Entity {
        width: 0.5,
        height: 0.5,
        is_projectile: true,
    };
    /// Ghast or Happy Ghast
    pub const GHAST: Self = Entity {
        width: 4.0,
        height: 4.0,
        is_projectile: false,
    };
    /// Thrown item
    pub const ITEM: Self = Entity {
        width: 0.25,
        height: 0.25,
        is_projectile: false,
    };
}
