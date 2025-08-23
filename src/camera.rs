use serde::{Deserialize, Serialize};

use crate::WorldPos;

/// Plane of the world to view.
#[derive(Serialize, Deserialize, Debug, Default, Copy, Clone, PartialEq, Eq, Hash)]
pub enum Plane {
    #[default]
    XY,
    XZ,
    ZY,
}

#[derive(Serialize, Deserialize, Debug, Copy, Clone, PartialEq)]
pub struct Camera {
    /// Overworld position of the center of the viewport.
    pub pos: WorldPos,
    /// Width of viewport, measured in overworld coordinates.
    pub width: f64,
    /// Height of viewport, measured in overworld coordinates.
    pub height: f64,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            pos: WorldPos {
                x: 0.0,
                y: 64.0,
                z: 0.0,
            },
            width: 100.0,
            height: 100.0,
        }
    }
}
