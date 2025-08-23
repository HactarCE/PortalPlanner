use serde::{Deserialize, Serialize};

use crate::{Axis, BlockRegion, Entity, WorldRegion};

#[derive(Serialize, Deserialize, Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum Direction {
    OverworldToNether,
    NetherToOverworld,
}

#[derive(Serialize, Deserialize, Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct Portal {
    pub region: BlockRegion,
    pub axis: Axis,
}

impl Portal {
    /// Returns the range of possible coordinates for an entity using the
    /// portal.
    pub fn teleport_region(self, entity: Entity) -> WorldRegion {
        let mut result = WorldRegion::from(self.region);
        result.min.x -= entity.width / 2.0;
        result.min.z -= entity.width / 2.0;
        result.max.x += entity.width / 2.0;
        result.max.y += entity.height;
        result.max.z += entity.width / 2.0;
        if !entity.is_projectile {
            // Restrict to within the portal frame.
            result.min[self.axis] += entity.width;
            result.max[self.axis] -= entity.width;
        }
        result
    }
}
