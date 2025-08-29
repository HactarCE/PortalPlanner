use egui::NumExt;
use serde::{Deserialize, Serialize};

use crate::util::max_range_distance_to;
use crate::{
    Axis, BlockPos, BlockRegion, ConvertDimension, Dimension, Entity, PortalId, WorldRegion,
};

/// Horizontal axis perpendicular to a portal's surface.
#[derive(Serialize, Deserialize, Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum PortalAxis {
    /// The portal is entered from east/west; portal width is along north/south
    /// (Z axis).
    X,
    /// The portal is entered from north/south; portal width is along east/west
    /// (X axis).
    Z,
}
impl From<PortalAxis> for Axis {
    fn from(value: PortalAxis) -> Self {
        match value {
            PortalAxis::X => Axis::X,
            PortalAxis::Z => Axis::Z,
        }
    }
}
impl PortalAxis {
    /// Returns the other horizontal axis.
    pub fn other(self) -> PortalAxis {
        match self {
            PortalAxis::X => PortalAxis::Z,
            PortalAxis::Z => PortalAxis::X,
        }
    }
}

/// Portal in an an unspecified dimension.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct Portal {
    /// Unique ID for the portal.
    #[serde(skip, default = "PortalId::new")]
    pub id: PortalId,
    /// Human-friendly name of the portal.
    #[serde(default)]
    pub name: String,
    /// Color used to represent portal in the UI.
    #[serde(default)]
    pub color: [u8; 3],
    /// Region filled with portal blocks in the source dimension.
    pub region: BlockRegion,
    /// Portal axis (opposite from what the game says).
    pub axis: PortalAxis,
}

impl Portal {
    /// Minimum width of a portal.
    pub const MIN_WIDTH: i64 = 2;
    /// Maximum height of a portal.
    pub const MIN_HEIGHT: i64 = 3;

    /// Minimum difference between the minimum and maximum coordinates along the
    /// width of a portal.
    const MIN_DW: i64 = Self::MIN_WIDTH - 1;
    /// Minimum difference between the minimum and maximum coordinates along the
    /// height of a portal.
    const MIN_DH: i64 = Self::MIN_HEIGHT - 1;

    /// Returns the region where an entity can collide with the portal and thus
    /// be teleported using it.
    ///
    /// Returns `None` if the entity won't fit in the portal.
    pub fn entity_collision_region(&self, entity: Entity) -> Option<WorldRegion> {
        let mut result = WorldRegion::from(self.region);
        result.min.x -= entity.width / 2.0;
        result.min.z -= entity.width / 2.0;
        result.max.x += entity.width / 2.0;
        result.max.z += entity.width / 2.0;
        if entity.is_projectile {
            result.min.y -= entity.height;
        }
        if !entity.is_projectile {
            // Restrict to within the portal frame.
            result.min[self.width_axis()] += entity.width;
            result.max[self.width_axis()] -= entity.width;
            result.max.y -= entity.height;
        }
        result.is_valid().then_some(result)
    }

    /// Returns the region where an entity may try to arrive.
    /// `destination_dimension` is the dimension the portal leads to, _not_ the
    /// one it is in.
    pub fn destination_region(
        &self,
        entity: Entity,
        destination_dimension: Dimension,
    ) -> Option<BlockRegion> {
        Some(
            self.entity_collision_region(entity)?
                .convert_dimension(destination_dimension.other(), destination_dimension)
                .block_region_containing(),
        )
    }

    /// Constructs a new portal at `pos` of the smallest possible size.
    pub fn new_minimal(pos: BlockPos, axis: PortalAxis, dimension: Dimension) -> Self {
        Self {
            id: PortalId::new(),
            name: String::new(),
            color: [127, 127, 127],
            region: BlockRegion {
                min: BlockPos {
                    x: pos.x,
                    y: pos.y.at_most(dimension.y_max() - Self::MIN_HEIGHT),
                    z: pos.z,
                },
                max: BlockPos {
                    x: pos.x + (axis != PortalAxis::X) as i64 * Self::MIN_DW,
                    y: pos.y + Self::MIN_DH,
                    z: pos.z + (axis != PortalAxis::Z) as i64 * Self::MIN_DW,
                },
            },
            axis,
        }
    }

    /// Constructs a portal from a region for testing. The axis is inferred from
    /// the size, which is assumed to be a valid portal size.
    #[cfg(test)]
    pub fn new_test(region: impl Into<BlockRegion>) -> Self {
        let id = PortalId::new();
        let region: BlockRegion = region.into();
        let axis = if region.min.x == region.max.x {
            PortalAxis::X
        } else if region.min.z == region.max.z {
            PortalAxis::Z
        } else {
            panic!("ambiguous axis")
        };
        Self {
            id,
            name: id.to_string(),
            color: [0; 3],
            region,
            axis,
        }
    }

    /// Returns the axis of the width of the portal.
    pub fn width_axis(&self) -> Axis {
        self.axis.other().into()
    }
    /// Returns the axis of the depth of the portal.
    pub fn depth_axis(&self) -> Axis {
        self.axis.into()
    }

    /// Returns a nonempty human-friendly name for the portal.
    pub fn display_name(&self) -> &str {
        if self.name.is_empty() {
            "<unnamed>"
        } else {
            &self.name
        }
    }

    /// Adjusts `min`, ensuring that the portal is valid. If `lock_size` is
    /// `true`, then the size is preserved; otherwise, `min` is adjusted as
    /// little as possible.
    pub fn adjust_min<R>(
        &mut self,
        f: impl FnOnce(&mut BlockPos) -> R,
        lock_size: bool,
        dimension: Dimension,
    ) -> R {
        let w = self.width_axis();
        let h = Axis::Y; // height axis
        let d = self.depth_axis();

        let min = &mut self.region.min;
        let max = &mut self.region.max;

        let dw = max[w].saturating_sub(min[w]);
        let dd = max[d].saturating_sub(min[d]);
        let dh = max[h].saturating_sub(min[h]);

        let r = f(min);

        // Leave enough room for the old height
        let lowest_min_y = dimension.y_min() + 1;
        let highest_min_y = (dimension.y_max() - 1 - dh).at_least(lowest_min_y);
        min.y = min.y.clamp(lowest_min_y, highest_min_y);

        if lock_size {
            max[w] = min[w].saturating_add(dw);
            max[h] = min[h].saturating_add(dh);
            max[d] = min[d].saturating_add(dd);
        } else {
            max[w] = max[w].at_least(min[w].saturating_add(Self::MIN_DW));
            max[h] = max[h].at_least(min[h].saturating_add(Self::MIN_DH));
            max[d] = max[d].at_least(min[d]);
        }

        r
    }

    /// Adjusts `max`, ensuring that the portal is valid. If `lock_size` is
    /// `true`, then the size is preserved; otherwise, `max` is adjusted as
    /// little as possible.
    pub fn adjust_max<R>(
        &mut self,
        f: impl FnOnce(&mut BlockPos) -> R,
        lock_size: bool,
        dimension: Dimension,
    ) -> R {
        let w = self.width_axis(); // width axis
        let h = Axis::Y; // height axis
        let d = self.depth_axis(); // depth axis

        let min = &mut self.region.min;
        let max = &mut self.region.max;

        let dw = max[w].saturating_sub(min[w]);
        let dd = max[d].saturating_sub(min[d]);
        let dh = max[h].saturating_sub(min[h]);

        let r = f(max);

        // Leave enough room for the old height
        let highest_min_y = dimension.y_max() - 1;
        let lowest_min_y = (dimension.y_min() + 1 + dh).at_most(highest_min_y);
        max.y = max.y.clamp(lowest_min_y, highest_min_y);

        if lock_size {
            min[w] = max[w].saturating_sub(dw);
            min[d] = max[d].saturating_sub(dd);
            min[h] = max[h].saturating_sub(dh);
        } else {
            min[w] = min[w].at_most(max[w].saturating_sub(Self::MIN_DW));
            min[d] = min[d].at_most(max[d]);
            min[h] = min[h].at_most(max[h].saturating_sub(Self::MIN_DH));
        }

        r
    }

    /// Adjusts the width of the portal using the provided closure, ensuring
    /// that the portal is valid. `min` is preserved.
    pub fn adjust_width<R>(&mut self, f: impl FnOnce(&mut i64) -> R) -> R {
        let w = self.width_axis();
        let mut width = self.region.max[w] - self.region.min[w] + 1;
        let r = f(&mut width);
        width = width.at_least(Self::MIN_WIDTH);
        self.region.max[w] = self.region.min[w].saturating_add(width - 1);
        r
    }

    /// Adjusts the height of the portal using the provided closure, ensuring
    /// that the portal is valid. `min` is preserved if possible.
    pub fn adjust_height<R>(&mut self, f: impl FnOnce(&mut i64) -> R, dimension: Dimension) -> R {
        // Bedrock can be broken in survival, but we can't use the full height
        // of the dimension because we need to leave room for the obsidian
        // frame.
        let mut height = self.region.max.y - self.region.min.y + 1;
        let r = f(&mut height);
        height = height.at_least(Self::MIN_HEIGHT);
        self.region.max.y = self.region.min.y.saturating_add(height - 1);
        if self.region.max.y > dimension.y_max() - 1 {
            let excess = self.region.max.y - (dimension.y_max() - 1);
            self.region.max.y -= excess;
            self.region.min.y -= excess;
            if self.region.min.y < dimension.y_min() + 1 {
                self.region.min.y = dimension.y_min() + 1;
            }
        }
        r
    }

    /// Adjusts the axis of the portal using the provided closure, ensuring the
    /// portal is valid.
    pub fn adjust_axis<R>(&mut self, f: impl FnOnce(&mut PortalAxis) -> R) -> R {
        let w = self.width_axis();

        let min = &mut self.region.min;
        let max = &mut self.region.max;
        let dw = max[w] - min[w];

        let r = f(&mut self.axis);

        let w = self.width_axis();
        let d = self.depth_axis();

        let min = &mut self.region.min;
        let max = &mut self.region.max;
        max[w] = min[w] + dw;
        max[d] = min[d];

        r
    }

    /// Returns whether `self` is within the portal search range for `pos`.
    pub fn is_in_range_of_point(&self, pos: BlockPos, dimension: Dimension) -> bool {
        // Ignore Y axis
        let r = dimension.portal_search_range();
        ((self.region.min.x - r)..=(self.region.max.x + r)).contains(&pos.x)
            && ((self.region.min.z - r)..=(self.region.max.z + r)).contains(&pos.z)
    }

    /// Returns whether `self` is within the portal search range for **any**
    /// point in `region`.
    pub fn is_in_range_of_region(&self, region: BlockRegion, dimension: Dimension) -> bool {
        // Ignore Y axis
        let r = dimension.portal_search_range();
        self.region.min.x <= region.max.x + r
            && self.region.min.z <= region.max.z + r
            && self.region.max.x >= region.min.x - r
            && self.region.max.z >= region.min.z - r
    }
    /// Returns whether `self` is within the portal search range for **all**
    /// points in `region`.
    pub fn is_always_in_range_of_region(&self, region: BlockRegion, dimension: Dimension) -> bool {
        // Ignore Y axis
        let r = dimension.portal_search_range();
        max_range_distance_to(
            region.min.x..=region.max.x,
            self.region.min.x..=self.region.max.x,
        ) <= r
            && max_range_distance_to(
                region.min.z..=region.max.z,
                self.region.min.z..=self.region.max.z,
            ) <= r
    }
}
