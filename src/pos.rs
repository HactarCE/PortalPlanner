use std::ops::{Index, IndexMut};

use serde::{Deserialize, Serialize};

/// Horizontal axis perpendicular to a portal's surface.
#[derive(Serialize, Deserialize, Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum Axis {
    X,
    Z,
}
impl Axis {
    /// Returns the other horizontal axis.
    pub fn other(self) -> Axis {
        match self {
            Axis::X => Axis::Z,
            Axis::Z => Axis::X,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Default, Copy, Clone, PartialEq, Eq, Hash)]
pub struct BlockPos {
    pub x: i64,
    pub y: i64,
    pub z: i64,
}
impl Index<Axis> for BlockPos {
    type Output = i64;

    fn index(&self, index: Axis) -> &Self::Output {
        match index {
            Axis::X => &self.x,
            Axis::Z => &self.z,
        }
    }
}
impl IndexMut<Axis> for BlockPos {
    fn index_mut(&mut self, index: Axis) -> &mut Self::Output {
        match index {
            Axis::X => &mut self.x,
            Axis::Z => &mut self.z,
        }
    }
}
impl From<WorldPos> for BlockPos {
    fn from(value: WorldPos) -> Self {
        let WorldPos { x, y, z } = value;
        BlockPos {
            x: x.floor() as i64,
            y: y.floor() as i64,
            z: z.floor() as i64,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Default, Copy, Clone, PartialEq)]
pub struct WorldPos {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}
impl Index<Axis> for WorldPos {
    type Output = f32;

    fn index(&self, index: Axis) -> &Self::Output {
        match index {
            Axis::X => &self.x,
            Axis::Z => &self.z,
        }
    }
}
impl IndexMut<Axis> for WorldPos {
    fn index_mut(&mut self, index: Axis) -> &mut Self::Output {
        match index {
            Axis::X => &mut self.x,
            Axis::Z => &mut self.z,
        }
    }
}
impl From<BlockPos> for WorldPos {
    fn from(value: BlockPos) -> Self {
        let BlockPos { x, y, z } = value;
        WorldPos {
            x: x as f32,
            y: y as f32,
            z: z as f32,
        }
    }
}
