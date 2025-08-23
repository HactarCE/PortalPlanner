use std::ops::{Index, IndexMut};

use serde::{Deserialize, Serialize};

use crate::Dimension;

#[derive(Serialize, Deserialize, Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum Axis {
    X,
    Y,
    Z,
}
impl Axis {
    /// Array of all axes.
    pub const ALL: [Axis; 3] = [Axis::X, Axis::Y, Axis::Z];
}

#[derive(Serialize, Deserialize, Debug, Default, Copy, Clone, PartialEq, Eq, Hash)]
pub struct BlockPos {
    pub x: i64,
    pub y: i64,
    pub z: i64,
}
impl<T: Into<Axis>> Index<T> for BlockPos {
    type Output = i64;

    fn index(&self, index: T) -> &Self::Output {
        match index.into() {
            Axis::X => &self.x,
            Axis::Y => &self.y,
            Axis::Z => &self.z,
        }
    }
}
impl<T: Into<Axis>> IndexMut<T> for BlockPos {
    fn index_mut(&mut self, index: T) -> &mut Self::Output {
        match index.into() {
            Axis::X => &mut self.x,
            Axis::Y => &mut self.y,
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
    pub x: f64,
    pub y: f64,
    pub z: f64,
}
impl<T: Into<Axis>> Index<T> for WorldPos {
    type Output = f64;

    fn index(&self, index: T) -> &Self::Output {
        match index.into() {
            Axis::X => &self.x,
            Axis::Y => &self.y,
            Axis::Z => &self.z,
        }
    }
}
impl<T: Into<Axis>> IndexMut<T> for WorldPos {
    fn index_mut(&mut self, index: T) -> &mut Self::Output {
        match index.into() {
            Axis::X => &mut self.x,
            Axis::Y => &mut self.y,
            Axis::Z => &mut self.z,
        }
    }
}
impl From<BlockPos> for WorldPos {
    fn from(value: BlockPos) -> Self {
        let BlockPos { x, y, z } = value;
        WorldPos {
            x: x as f64,
            y: y as f64,
            z: z as f64,
        }
    }
}
impl WorldPos {
    pub fn nether_to_overworld(self) -> Self {
        WorldPos {
            x: self.x * Dimension::Nether.scale(),
            y: self.y,
            z: self.z * Dimension::Nether.scale(),
        }
    }
    pub fn overworld_to_nether(self) -> Self {
        WorldPos {
            x: self.x / Dimension::Nether.scale(),
            y: self.y,
            z: self.z / Dimension::Nether.scale(),
        }
    }
}
