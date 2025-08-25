use serde::{Deserialize, Serialize};

use crate::{ConvertDimension, Dimension, WorldPos};

/// Plane of the world to view.
#[derive(Serialize, Deserialize, Debug, Default, Copy, Clone, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub enum Plane {
    #[default]
    XY,
    XZ,
    ZY,
}

/// Plot camera location.
#[derive(Serialize, Deserialize, Debug, Copy, Clone, PartialEq)]
pub struct Camera {
    /// Dimension being viewed.
    pub dimension: Dimension,
    /// Position of the center of the viewport.
    pub pos: WorldPos,
    /// Width of viewport, measured in overworld coordinates.
    pub width: f64,
    /// Height of viewport, measured in overworld coordinates.
    pub height: f64,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            dimension: Dimension::Overworld,
            pos: WorldPos {
                x: 0.0,
                y: 64.0,
                z: 0.0,
            },
            width: 1024.0,
            height: 1024.0,
        }
    }
}

impl Camera {
    /// Returns the position of the camera in the given dimension.
    pub fn pos_in(self, dimension: Dimension) -> WorldPos {
        self.pos.convert_dimension(self.dimension, dimension)
    }

    /// Sets the dimension of the camera, converting its position accordingly.
    pub fn set_dimension(&mut self, dimension: Dimension) {
        self.pos = self.pos_in(dimension);
        self.dimension = dimension;
    }
}
