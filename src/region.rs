use egui::NumExt;
use serde::{Deserialize, Serialize};

use crate::{BlockPos, ConvertDimension, WorldPos};

#[derive(Serialize, Deserialize, Debug, Default, Copy, Clone, PartialEq, Eq, Hash)]
pub struct BlockRegion {
    /// Minimum coordinate (inclusive)
    pub min: BlockPos,
    /// Maximum coordinate (inclusive)
    pub max: BlockPos,
}

impl BlockRegion {
    /// Adjusts `max` to ensure that `min <= max` along each axis.
    pub fn adjust_max(&mut self) {
        self.max.x = self.max.x.at_least(self.min.x);
        self.max.y = self.max.y.at_least(self.min.y);
        self.max.z = self.max.z.at_least(self.min.z);
    }

    /// Adjusts `max` to ensure that `min <= max` along each axis.
    pub fn adjust_min(&mut self) {
        self.min.x = self.min.x.at_most(self.max.x);
        self.min.y = self.min.y.at_most(self.max.y);
        self.min.z = self.min.z.at_most(self.max.z);
    }
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

impl ConvertDimension for WorldRegion {
    fn nether_to_overworld(self) -> Self {
        Self {
            min: self.min.nether_to_overworld(),
            max: self.max.nether_to_overworld(),
        }
    }
    fn overworld_to_nether(self) -> Self {
        Self {
            min: self.min.overworld_to_nether(),
            max: self.max.overworld_to_nether(),
        }
    }
}

impl WorldRegion {
    pub fn center(self) -> WorldPos {
        WorldPos {
            x: (self.min.x + self.max.x) * 0.5,
            y: (self.min.y + self.max.y) * 0.5,
            z: (self.min.z + self.max.z) * 0.5,
        }
    }
}
