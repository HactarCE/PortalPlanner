use egui::NumExt;
use serde::{Deserialize, Serialize};

use crate::util::{max_range_distance_to, min_range_distance_to, min_range_distance_to_pos};
use crate::{Axis, BlockPos, ConvertDimension, WorldPos};

/// Cuboid of block coordinates.
#[derive(Serialize, Deserialize, Debug, Default, Copy, Clone, PartialEq, Eq, Hash)]
pub struct BlockRegion {
    /// Minimum coordinate (inclusive)
    pub min: BlockPos,
    /// Maximum coordinate (inclusive)
    pub max: BlockPos,
}

impl<A: Into<BlockPos>, B: Into<BlockPos>> From<(A, B)> for BlockRegion {
    fn from((min, max): (A, B)) -> Self {
        BlockRegion {
            min: min.into(),
            max: max.into(),
        }
    }
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

    /// Returns the **minimum** possible squared Euclidean distance between any
    /// point in `self` and the closest point in `other`.
    ///
    /// This chooses the **closest** point in both regions.
    pub fn min_euclidean_distance_sq_to(self, other: Self) -> i64 {
        let dx = min_range_distance_to(self.min.x..=self.max.x, other.min.x..=other.max.x);
        let dy = min_range_distance_to(self.min.y..=self.max.y, other.min.y..=other.max.y);
        let dz = min_range_distance_to(self.min.z..=self.max.z, other.min.z..=other.max.z);
        dx * dx + dy * dy + dz * dz
    }

    /// Returns the **maximum** possible squared Euclidean distance between any
    /// point in `self` and the closest point in `other`.
    ///
    /// This chooses the **farthest** point in `self` and the **closest** point
    /// in `other`.
    pub fn max_euclidean_distance_sq_to(self, other: Self) -> i64 {
        let dx = max_range_distance_to(self.min.x..=self.max.x, other.min.x..=other.max.x);
        let dy = max_range_distance_to(self.min.y..=self.max.y, other.min.y..=other.max.y);
        let dz = max_range_distance_to(self.min.z..=self.max.z, other.min.z..=other.max.z);
        dx * dx + dy * dy + dz * dz
    }

    /// Returns the **minimum** possible squared Euclidean distance between any
    /// point in `self` and `other`.
    ///
    /// This chooses the **closest** point in `self`.
    pub fn min_euclidean_distance_sq_to_point(self, pos: BlockPos) -> i64 {
        let dx = min_range_distance_to_pos(self.min.x..=self.max.x, pos.x);
        let dy = min_range_distance_to_pos(self.min.y..=self.max.y, pos.y);
        let dz = min_range_distance_to_pos(self.min.z..=self.max.z, pos.z);
        dx * dx + dy * dy + dz * dz
    }

    /// Returns an iterator over all positions in the block.
    pub fn iter(self) -> impl Iterator<Item = BlockPos> {
        itertools::iproduct!(
            self.min.z..=self.max.z,
            self.min.y..=self.max.y,
            self.min.x..=self.max.x,
        )
        .map(|(z, y, x)| BlockPos { x, y, z })
    }

    fn is_valid_on_axis(self, axis: Axis) -> bool {
        self.min[axis] <= self.max[axis]
    }

    /// Returns the 8 corners of the region in the following order:
    ///
    /// ```
    /// [-, -, -]
    /// [+, -, -]
    /// [-, +, -]
    /// [+, +, -]
    /// [-, -, +]
    /// [+, -, +]
    /// [-, +, +]
    /// [+, +, +]
    /// ```
    ///
    /// The X axis is represented by the least significant bit of the index; the
    /// Z axis is represented by the most significant bit.
    pub fn corners(self) -> [BlockPos; 8] {
        let BlockRegion { min, max } = self;
        let [x1, y1, z1] = min.into();
        let [x2, y2, z2] = max.into();

        [
            [x1, y1, z1],
            [x2, y1, z1],
            [x1, y2, z1],
            [x2, y2, z1],
            [x1, y1, z2],
            [x2, y1, z2],
            [x1, y2, z2],
            [x2, y2, z2],
        ]
        .map(BlockPos::from)
    }
    /// Splits a region in half along `axis`, excluding the ends along that
    /// axis.
    #[must_use]
    pub fn split_excluding_corners(self, axis: Axis) -> [Option<BlockRegion>; 2] {
        let mut lo = self;
        let mut hi = self;
        lo.min[axis] += 1;
        hi.max[axis] -= 1;
        let half_size = (self.max[axis] - self.min[axis]) / 2;
        let halfway = self.min[axis] + half_size;
        lo.max[axis] = halfway;
        hi.min[axis] = halfway + 1;
        [
            lo.is_valid_on_axis(axis).then_some(lo),
            hi.is_valid_on_axis(axis).then_some(hi),
        ]
    }
    /// Splits a region in half along `axis`, excluding the ends along that
    /// axis.
    #[must_use]
    pub fn split_excluding_corners_at(
        mut self,
        axis: Axis,
        coordinate: i64,
    ) -> [Option<BlockRegion>; 2] {
        self.min[axis] += 1;
        self.max[axis] -= 1;
        self.split_at(axis, coordinate)
    }
    /// Splits a region in half at `coordinate + 0.5` along `axis`.
    #[must_use]
    pub fn split_at(self, axis: Axis, coordinate: i64) -> [Option<BlockRegion>; 2] {
        let mut lo = self;
        let mut hi = self;
        lo.max[axis] = std::cmp::min(lo.max[axis], coordinate);
        hi.min[axis] = std::cmp::max(hi.min[axis], coordinate + 1);
        [
            lo.is_valid_on_axis(axis).then_some(lo),
            hi.is_valid_on_axis(axis).then_some(hi),
        ]
    }
}

/// Cuboid of world coordinates.
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
    /// Returns the position at the center of the region.
    pub fn center(self) -> WorldPos {
        WorldPos {
            x: (self.min.x + self.max.x) * 0.5,
            y: (self.min.y + self.max.y) * 0.5,
            z: (self.min.z + self.max.z) * 0.5,
        }
    }

    /// Returns the smallest block region that contains `self`.
    pub fn block_region_containing(self) -> BlockRegion {
        BlockRegion {
            min: self.min.into(), // floor
            max: self.max.into(), // floor
        }
    }

    /// Returns whether the minimum coordinate is less than or equal to the
    /// maximum coordinate along each axis.
    pub fn is_valid(self) -> bool {
        self.min.x <= self.max.x && self.min.y <= self.max.y && self.min.z <= self.max.z
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_excluding_corners() {
        let min = BlockPos { x: 1, y: 2, z: 3 };
        let max = BlockPos {
            x: 10,
            y: 20,
            z: 30,
        };
        let mut block = BlockRegion { min, max };
        let replace_z = |z1, z2| BlockRegion {
            min: BlockPos { z: z1, ..min },
            max: BlockPos { z: z2, ..max },
        };

        assert_eq!(
            block.split_excluding_corners(Axis::Z),
            [Some(replace_z(4, 16)), Some(replace_z(17, 29))],
        );

        block.min.z = 4;
        assert_eq!(
            block.split_excluding_corners(Axis::Z),
            [Some(replace_z(5, 17)), Some(replace_z(18, 29))],
        );

        block.max.z = 29;
        assert_eq!(
            block.split_excluding_corners(Axis::Z),
            [Some(replace_z(5, 16)), Some(replace_z(17, 28))],
        );

        block.min.z = 5;
        assert_eq!(
            block.split_excluding_corners(Axis::Z),
            [Some(replace_z(6, 17)), Some(replace_z(18, 28))],
        );

        block.max.z = 10;
        assert_eq!(
            block.split_excluding_corners(Axis::Z),
            [Some(replace_z(6, 7)), Some(replace_z(8, 9))],
        );

        block.max.z = 9;
        assert_eq!(
            block.split_excluding_corners(Axis::Z),
            [Some(replace_z(6, 7)), Some(replace_z(8, 8))],
        );

        block.max.z = 8;
        assert_eq!(
            block.split_excluding_corners(Axis::Z),
            [Some(replace_z(6, 6)), Some(replace_z(7, 7))],
        );

        block.max.z = 7;
        assert_eq!(
            block.split_excluding_corners(Axis::Z),
            [Some(replace_z(6, 6)), None],
        );

        block.max.z = 6;
        assert_eq!(block.split_excluding_corners(Axis::Z), [None, None],);
    }
}
