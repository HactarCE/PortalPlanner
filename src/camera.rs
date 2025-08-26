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
impl Plane {
    /// Converts world coordinates to plot coordinates.
    pub fn world_to_plot(self, pos: WorldPos) -> egui_plot::PlotPoint {
        let [x, y] = match self {
            Plane::XY => [pos.x, pos.y],
            Plane::XZ => [pos.x, -pos.z],
            Plane::ZY => [pos.z, pos.y],
        };
        egui_plot::PlotPoint { x, y }
    }
    /// Converts plot coordinates to world coordinates.
    pub fn plot_to_world(self, point: egui_plot::PlotPoint, camera: Camera) -> WorldPos {
        let [x, y, z] = match self {
            Plane::XY => [point.x, point.y, camera.pos.z],
            Plane::XZ => [point.x, camera.pos.y, -point.y],
            Plane::ZY => [camera.pos.x, point.y, point.x],
        };
        WorldPos { x, y, z }
    }
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
