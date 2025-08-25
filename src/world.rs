use std::fmt;
use std::ops::{Index, IndexMut, RangeInclusive};

use itertools::Itertools;
use serde::{Deserialize, Serialize};
use smallvec::{SmallVec, smallvec};

use crate::{Axis, BlockRegion, Portal};

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

    /// Returns the number of blocks away from a destination block that a portal
    /// block can be while still being found by the portal search algorithm.
    ///
    /// - In the overworld, portals are searched within 257x257, so this method
    ///   returns 128.
    /// - In the nether, portals are searched within 33x33, so this method
    ///   returns 16.
    pub fn portal_search_range(self) -> i64 {
        match self {
            Dimension::Overworld => 128,
            Dimension::Nether => 16,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Default, Clone, PartialEq, Eq)]
pub struct World {
    pub portals: WorldPortals,
}

#[derive(Serialize, Deserialize, Debug, Default, Clone, PartialEq, Eq)]
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

impl WorldPortals {
    pub fn portals_in_range(
        &self,
        destination_dimension: Dimension,
        destination_region: BlockRegion,
    ) -> impl Iterator<Item = &Portal> {
        self[destination_dimension]
            .iter()
            .filter(move |p| p.is_in_range_of_region(destination_region, destination_dimension))
    }

    pub fn reachable_portals(
        &self,
        destination_dimension: Dimension,
        destination_region: BlockRegion,
    ) -> ReachablePortals<'_> {
        let candidates = &self[destination_dimension];

        let mut confirmed_reachable = vec![false; candidates.len()];
        let mut may_generate_new_portal = false;

        let mut steps = 0;

        mark_reachable_portals(
            destination_dimension,
            destination_region,
            &candidates,
            (0..candidates.len()).collect(),
            &mut confirmed_reachable,
            &mut may_generate_new_portal,
            &mut steps,
        );

        ReachablePortals {
            existing_portals: confirmed_reachable
                .iter()
                .positions(|b| *b)
                .map(|i| &candidates[i])
                .collect(),
            new_portal: may_generate_new_portal,
        }
    }
}

fn mark_reachable_portals(
    destination_dimension: Dimension,
    destination_region: BlockRegion,
    candidates: &[Portal],
    mut candidates_that_might_be_reachable: SmallVec<[usize; 8]>,
    confirmed_reachable: &mut [bool],
    may_generate_new_portal: &mut bool,
    steps: &mut usize,
) {
    *steps += 1;

    // Filter for portals within the search range
    candidates_that_might_be_reachable.retain(|&mut p| {
        candidates[p].is_in_range_of_region(destination_region, destination_dimension)
    });

    // Filter for portals that are not strictly farther than another portal
    let smallest_max_distance = candidates_that_might_be_reachable
        .iter()
        .map(|&p| destination_region.max_euclidean_distance_sq_to(candidates[p].region))
        .min()
        .unwrap_or(0);
    candidates_that_might_be_reachable.retain(|&mut p| {
        destination_region.min_euclidean_distance_sq_to(candidates[p].region)
            <= smallest_max_distance
    });

    let corners = destination_region.corners();
    let closest_at_each_corner = corners.map(|corner| {
        minima_by_opt_key(candidates_that_might_be_reachable.iter().copied(), |&p| {
            candidates[p]
                .is_in_range_of_point(corner, destination_dimension)
                .then(|| {
                    candidates[p]
                        .region
                        .min_euclidean_distance_sq_to_point(corner)
                })
        })
    });

    *may_generate_new_portal |= closest_at_each_corner
        .iter()
        .any(|closest_at_corner| closest_at_corner.is_empty());

    for &p in closest_at_each_corner.iter().flatten() {
        confirmed_reachable[p] = true;
    }

    let mut unconfirmed_candidates = candidates_that_might_be_reachable
        .iter()
        .copied()
        .filter(|&p| !confirmed_reachable[p]);

    if unconfirmed_candidates.next().is_none() {
        return; // done! confirmed reachability for all
    }

    // Split along an axis that has a difference.
    let axes_to_split_along = Axis::ALL.map(|axis| {
        let should_split_along_axis = (0..8).any(|corner1| {
            let corner2 = corner1 ^ (1 << axis as usize);
            corner1 < corner2 && closest_at_each_corner[corner1] != closest_at_each_corner[corner2]
        });
        if should_split_along_axis {
            for opt_destination_subregion in destination_region.split_excluding_corners(axis) {
                if let Some(destination_subregion) = opt_destination_subregion {
                    mark_reachable_portals(
                        destination_dimension,
                        destination_subregion,
                        candidates,
                        candidates_that_might_be_reachable.clone(),
                        confirmed_reachable,
                        may_generate_new_portal,
                        steps,
                    );
                }
            }
        }
        should_split_along_axis
    });

    let unconfirmed_candidates = candidates_that_might_be_reachable
        .iter()
        .copied()
        .filter(|&p| !confirmed_reachable[p]);

    // Split along axis for any portal that might be reachable but hasn't yet
    // been reached.
    for p in unconfirmed_candidates {
        for axis in Axis::ALL {
            if axes_to_split_along[axis as usize] {
                continue;
            }
            let candidate_region = candidates[p].region;
            for split_point in [candidate_region.min[axis], candidate_region.max[axis]] {
                if (destination_region.min[axis]..=destination_region.max[axis])
                    .contains(&split_point)
                {
                    if let [Some(lo), Some(hi)] = destination_region.split_at(axis, split_point) {
                        for destination_subregion in [lo, hi] {
                            mark_reachable_portals(
                                destination_dimension,
                                destination_subregion,
                                candidates,
                                candidates_that_might_be_reachable.clone(),
                                confirmed_reachable,
                                may_generate_new_portal,
                                steps,
                            );
                        }
                        return;
                    }
                }
            }
        }
    }
}

fn minima_by_opt_key<I: IntoIterator, C: Ord>(
    iter: I,
    f: impl Fn(&I::Item) -> Option<C>,
) -> SmallVec<[I::Item; 2]> {
    let mut min_key = None;
    let mut ret = smallvec![];
    for item in iter {
        let Some(key) = f(&item) else {
            continue;
        };
        if min_key.as_ref().is_none_or(|m| key < *m) {
            min_key = Some(key);
            ret.clear();
            ret.push(item);
        } else if min_key == Some(key) {
            ret.push(item);
        }
    }
    ret
}

pub struct ReachablePortals<'a> {
    pub existing_portals: Vec<&'a Portal>,
    pub new_portal: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_minima_by_opt_key() {
        let xs = vec![
            ("a", Some(4)),
            ("b", Some(2)),
            ("c", Some(1)),
            ("d", None),
            ("e", None),
            ("f", Some(3)),
            ("g", Some(4)),
            ("h", Some(1)),
            ("i", Some(6)),
        ];
        assert_eq!(
            [("c", Some(1)), ("h", Some(1))].as_slice(),
            minima_by_opt_key(xs, |(_, key)| *key).as_slice(),
        );
    }
}
