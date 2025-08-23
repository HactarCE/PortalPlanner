use serde::{Deserialize, Serialize};

/// Description of an entity.
///
/// An entity's position is at the bottom center of its hitbox.
#[derive(Serialize, Deserialize, Debug, Default, Copy, Clone, PartialEq)]
pub struct Entity {
    /// Width of the entity's hitbox along the X and Z axes
    pub width: f32,
    /// Height of the entity's hitbox along the Y axis.
    pub height: f32,
    /// Whether the entity is a projectile, in which case it is possible to clip
    /// into the portal frame.
    pub is_projectile: bool,
}

impl Entity {
    pub const PLAYER: Self = Entity {
        width: 0.6,
        height: 1.8,
        is_projectile: false,
    };
    pub const ENDER_PEARL: Self = Entity {
        width: 0.25,
        height: 0.25,
        is_projectile: true,
    };
}
