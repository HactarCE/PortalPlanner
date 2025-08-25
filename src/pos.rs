use std::fmt;
use std::ops::{Index, IndexMut};

use serde::{Deserialize, Serialize};

use crate::{ConvertDimension, Dimension};

/// Axis in the world
#[derive(Serialize, Deserialize, Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum Axis {
    /// EAST/WEST
    X,
    /// UP/DOWN
    Y,
    /// NORTH/SOUTH
    Z,
}
impl Axis {
    /// Array of all axes.
    pub const ALL: [Axis; 3] = [Axis::X, Axis::Y, Axis::Z];
}

/// Block coordinates.
///
/// Note that block coordinates cannot be converted directly between dimensions;
/// they must be converted to world coordinates first.
#[derive(Serialize, Deserialize, Debug, Default, Copy, Clone, PartialEq, Eq, Hash)]
pub struct BlockPos {
    /// EAST/WEST
    pub x: i64,
    /// UP/DOWN
    pub y: i64,
    /// NORTH/SOUTH
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
impl BlockPos {
    /// Returns the squared Euclidean distance between `self` and `other`.
    pub fn euclidean_distance_sq(&self, other: &Self) -> i64 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        let dz = self.z - other.z;
        dx * dx + dy * dy + dz * dz
    }
}

/// Coordinates within a dimension.
#[derive(Serialize, Deserialize, Debug, Default, Copy, Clone, PartialEq)]
pub struct WorldPos {
    /// EAST/WEST
    pub x: f64,
    /// UP/DOWN
    pub y: f64,
    /// NORTH/SOUTH
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
impl ConvertDimension for WorldPos {
    fn nether_to_overworld(self) -> Self {
        WorldPos {
            x: self.x * Dimension::Nether.scale(),
            y: self.y,
            z: self.z * Dimension::Nether.scale(),
        }
    }
    fn overworld_to_nether(self) -> Self {
        WorldPos {
            x: self.x / Dimension::Nether.scale(),
            y: self.y,
            z: self.z / Dimension::Nether.scale(),
        }
    }
}
impl fmt::Display for WorldPos {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.x.fmt(f)?;
        write!(f, ", ")?;
        self.y.fmt(f)?;
        write!(f, ", ")?;
        self.z.fmt(f)?;
        Ok(())
    }
}
