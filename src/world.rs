use std::fmt;
use std::ops::{Index, IndexMut, RangeInclusive};

use itertools::Itertools;
use serde::{Deserialize, Serialize};
use smallvec::{SmallVec, smallvec};

use crate::{Axis, BlockRegion, Portal};

/// Overworld or nether.
#[derive(Serialize, Deserialize, Debug, Default, Copy, Clone, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
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
    /// Returns the scale of the dimension, which provides the conversion rate
    /// between dimensions.
    pub fn scale(self) -> f64 {
        match self {
            Dimension::Overworld => 1.0,
            Dimension::Nether => 8.0,
        }
    }

    /// Returns the lowest Y coordinate at which a block can be placed.
    pub fn y_min(self) -> i64 {
        match self {
            Dimension::Overworld => -64,
            Dimension::Nether => 0,
        }
    }

    /// Returns the highest Y coordinate at which a block can be placed.
    pub fn y_max(self) -> i64 {
        match self {
            Dimension::Overworld => 319,
            Dimension::Nether => 255,
        }
    }

    /// Returns the range of Y coordinates at which blocks can be placed.
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

/// Minecraft world.
#[derive(Serialize, Deserialize, Debug, Default, Clone, PartialEq, Eq)]
pub struct World {
    /// Portals in each dimension.
    pub portals: WorldPortals,
}

/// Portals in a Minecraft world.
#[derive(Serialize, Deserialize, Default, Clone, PartialEq, Eq)]
#[cfg_attr(not(test), derive(Debug))]
pub struct WorldPortals {
    /// Portals in the overworld.
    pub overworld: Vec<Portal>,
    /// Portals in the nether.
    pub nether: Vec<Portal>,
}
#[cfg(test)]
impl fmt::Debug for WorldPortals {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", serde_json::to_string_pretty(self).unwrap())
    }
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
    /// Converts from nether coordinates to overworld coordinates.
    #[must_use]
    fn nether_to_overworld(self) -> Self;

    /// Converts from overworld coordinates to nether coordinates.
    #[must_use]
    fn overworld_to_nether(self) -> Self;

    /// Converts coordinates from one dimension to another.
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
    pub(crate) fn portal_destinations_naive(
        &self,
        destination_dimension: Dimension,
        destination_region: BlockRegion,
    ) -> PortalDestinations<'_> {
        let candidates = &self[destination_dimension];

        let mut candidates_in_range = vec![false; candidates.len()];

        let mut distances = vec![0; candidates.len()]; // buffer for reuse
        let mut new_portal = false;
        for point in destination_region.iter() {
            for i in 0..candidates.len() {
                distances[i] = if candidates[i].is_in_range_of_point(point, destination_dimension) {
                    candidates[i]
                        .region
                        .min_euclidean_distance_sq_to_point(point)
                } else {
                    i64::MAX
                };
            }
            let min_distance = distances.iter().copied().min().unwrap_or(i64::MAX);
            if min_distance == i64::MAX {
                new_portal = true;
            } else {
                for (i, &distance) in distances.iter().enumerate() {
                    candidates_in_range[i] |= distance == min_distance;
                }
            }
        }

        let existing_portals = std::iter::zip(candidates, candidates_in_range)
            .filter(|(_, in_range)| *in_range)
            .map(|(p, _)| p)
            .collect();

        PortalDestinations {
            existing_portals,
            new_portal,
        }
    }

    /// Returns the set of portals that are reachable from `destination_region`.
    pub fn portal_destinations(
        &self,
        destination_dimension: Dimension,
        destination_region: BlockRegion,
    ) -> PortalDestinations<'_> {
        let candidates = &self[destination_dimension];

        let mut confirmed_reachable = vec![false; candidates.len()];
        let mut may_generate_new_portal = false;

        let mut steps = 0;

        mark_reachable_portals(
            destination_dimension,
            destination_region,
            candidates,
            (0..candidates.len()).collect(),
            &mut confirmed_reachable,
            &mut may_generate_new_portal,
            &mut steps,
        );

        PortalDestinations {
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

    // Filter for portals that are not strictly farther than another
    // always-in-range portal
    let smallest_max_distance = candidates_that_might_be_reachable
        .iter()
        .filter(|&&p| {
            candidates[p].is_always_in_range_of_region(destination_region, destination_dimension)
        })
        .map(|&p| destination_region.max_euclidean_distance_sq_to(candidates[p].region))
        .min()
        .unwrap_or(i64::MAX);
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

    log::trace!(
        "in region [{dx}, {dy}, {dz}]",
        dx = destination_region.max.x - destination_region.min.x,
        dy = destination_region.max.y - destination_region.min.y,
        dz = destination_region.max.z - destination_region.min.z,
    );
    log::trace!("candidates are {candidates_that_might_be_reachable:?}");
    log::trace!("closest at each corner is {closest_at_each_corner:?}");

    // Split along an axis that has a difference.
    let axes_to_split_along = Axis::ALL.map(|axis| {
        let should_split_along_axis = (0..8).any(|corner1| {
            let corner2 = corner1 ^ (1 << axis as usize);
            corner1 < corner2 && closest_at_each_corner[corner1] != closest_at_each_corner[corner2]
        });
        if should_split_along_axis {
            log::trace!("splitting region along {axis}");
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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PortalDestinations<'a> {
    pub existing_portals: Vec<&'a Portal>,
    pub new_portal: bool,
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use crate::{Entity, PortalAxis};

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

    #[test]
    fn test_portal_splitting() {
        let big = Portal::new_test(([8, 64, 5], [8, 66, 18])); // nether
        let a = Portal::new_test(([88, 60, -15], [90, 62, -15])); // overworld
        let b = Portal::new_test(([0, 64, 0], [0, 66, 1])); // overworld
        let world = World {
            portals: WorldPortals {
                overworld: vec![a, b],
                nether: vec![big.clone()],
            },
        };
        let destination_region = big
            .destination_region(Entity::PLAYER, Dimension::Overworld)
            .unwrap();
        let expected = world
            .portals
            .portal_destinations_naive(Dimension::Overworld, destination_region);
        let actual = world
            .portals
            .portal_destinations(Dimension::Overworld, destination_region);
        assert_eq!(expected, actual);
    }

    proptest! {
        #[test]
        fn proptest_portal_linking(portals in random_portals()) {
            test_portal_linking(portals);
        }
    }

    fn test_portal_linking(portals: WorldPortals) {
        for source_dimension in [Dimension::Overworld, Dimension::Nether] {
            let destination_dimension = source_dimension.other();
            for portal in &portals[source_dimension] {
                let destination_region = portal
                    .destination_region(Entity::PLAYER, destination_dimension)
                    .unwrap(); // valid portals always fit players
                let expected =
                    portals.portal_destinations_naive(destination_dimension, destination_region);
                let actual = portals.portal_destinations(destination_dimension, destination_region);
                assert_eq!(expected.new_portal, actual.new_portal);
                assert_eq!(
                    expected
                        .existing_portals
                        .iter()
                        .map(|p| p.id)
                        .sorted()
                        .collect_vec(),
                    actual
                        .existing_portals
                        .iter()
                        .map(|p| p.id)
                        .sorted()
                        .collect_vec(),
                );
            }
        }
    }

    fn random_portals() -> impl Strategy<Value = WorldPortals> {
        (
            prop::collection::vec(random_portal(Dimension::Overworld), 0..=10),
            prop::collection::vec(random_portal(Dimension::Nether), 0..=10),
        )
            .prop_map(|(overworld, nether)| WorldPortals { overworld, nether })
    }

    fn random_portal(dimension: Dimension) -> impl Strategy<Value = Portal> {
        // The naive algorithm is slow for nether->overworld travel, so we limit
        // the size of portals in the nether for performance.
        let max_width: i64 = match dimension {
            Dimension::Overworld => 21, // max allowed in-game
            Dimension::Nether => 4,
        };
        let max_height: i64 = match dimension {
            Dimension::Overworld => 21, // max allowed in-game
            Dimension::Nether => 5,
        };
        let max_coordinate = (100 as f64 / dimension.scale()) as i64;
        let x = -max_coordinate..=max_coordinate;
        let y = dimension.y_min()..=(dimension.y_max() - 10);
        let z = -max_coordinate..=max_coordinate;
        let w = 2..=max_width;
        let h = 3..=max_height;
        let axis = prop_oneof![Just(PortalAxis::X), Just(PortalAxis::Z)];
        (x, y, z, w, h, axis).prop_map(move |(x, y, z, width, height, axis)| {
            let mut p = Portal::new_minimal([x, y, z].into(), axis, dimension);
            p.adjust_width(|w| *w = width);
            p.adjust_height(|h| *h = height, dimension);
            p
        })
    }
}
