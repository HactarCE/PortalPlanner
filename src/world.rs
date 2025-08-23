use std::fmt;
use std::ops::{Index, IndexMut, RangeInclusive};

use serde::{Deserialize, Serialize};

use crate::Portal;

#[derive(Serialize, Deserialize, Debug, Default, Copy, Clone, PartialEq, Eq, Hash)]
pub enum Dimension {
    #[default]
    Overworld,
    Nether,
}

impl fmt::Display for Dimension {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Dimension::Overworld => write!(f, "Overworld"),
            Dimension::Nether => write!(f, "Nether"),
        }
    }
}

impl Dimension {
    pub fn scale(self) -> f64 {
        match self {
            Dimension::Overworld => 1.0,
            Dimension::Nether => 8.0,
        }
    }

    pub fn y_min(self) -> i64 {
        match self {
            Dimension::Overworld => -64,
            Dimension::Nether => 0,
        }
    }

    pub fn y_max(self) -> i64 {
        match self {
            Dimension::Overworld => 319,
            Dimension::Nether => 255,
        }
    }

    pub fn y_range(self) -> RangeInclusive<i64> {
        self.y_min()..=self.y_max()
    }

    /// Returns the other dimension.
    pub fn other(self) -> Dimension {
        match self {
            Dimension::Overworld => Dimension::Nether,
            Dimension::Nether => Dimension::Overworld,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Default, Clone)]
pub struct World {
    pub portals: WorldPortals,
}

#[derive(Serialize, Deserialize, Debug, Default, Clone)]
pub struct WorldPortals {
    pub overworld: Vec<Portal>,
    pub nether: Vec<Portal>,
}

impl Index<Dimension> for WorldPortals {
    type Output = Vec<Portal>;

    fn index(&self, index: Dimension) -> &Self::Output {
        match index {
            Dimension::Overworld => &self.overworld,
            Dimension::Nether => &self.nether,
        }
    }
}

impl IndexMut<Dimension> for WorldPortals {
    fn index_mut(&mut self, index: Dimension) -> &mut Self::Output {
        match index {
            Dimension::Overworld => &mut self.overworld,
            Dimension::Nether => &mut self.nether,
        }
    }
}

/// Trait for types that can be converted between dimensions.
pub trait ConvertDimension: Sized {
    #[must_use]
    fn nether_to_overworld(self) -> Self;

    #[must_use]
    fn overworld_to_nether(self) -> Self;

    #[must_use]
    fn convert_dimension(self, from: Dimension, to: Dimension) -> Self {
        match (from, to) {
            (Dimension::Overworld, Dimension::Overworld) => self,
            (Dimension::Overworld, Dimension::Nether) => self.overworld_to_nether(),
            (Dimension::Nether, Dimension::Overworld) => self.nether_to_overworld(),
            (Dimension::Nether, Dimension::Nether) => self,
        }
    }
}
