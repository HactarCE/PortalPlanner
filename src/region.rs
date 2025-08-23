use serde::{Deserialize, Serialize};

use crate::{BlockPos, WorldPos};

#[derive(Serialize, Deserialize, Debug, Default, Copy, Clone, PartialEq, Eq, Hash)]
pub struct BlockRegion {
    /// Minimum coordinate (inclusive)
    pub min: BlockPos,
    /// Maximum coordinate (inclusive)
    pub max: BlockPos,
}

#[derive(Serialize, Deserialize, Debug, Default, Copy, Clone, PartialEq)]
pub struct WorldRegion {
    /// Minimum coordinate (inclusive)
    pub min: WorldPos,
    /// Maximum coordinate (inclusive)
    pub max: WorldPos,
}
impl From<BlockRegion> for WorldRegion {
    fn from(value: BlockRegion) -> Self {
        let min = WorldPos::from(value.min);
        let mut max = WorldPos::from(value.max);
        max.x += 1.0;
        max.y += 1.0;
        max.z += 1.0;
        Self { min, max }
    }
}
