//! Where a proposal points: every place a group names goes through the location resolver
//! before anything else looks at the group.
//!
//! `destination`, `selector.root`, a rename's `parent`, and each hand-listed source (named
//! WITH `sourceVolumeId`, so a source on a saved share keeps its `sameShareAs`) resolve over
//! ONE `VolumeFacts` snapshot per call. A miss, or a place on a volume that isn't live with no
//! proven sibling to take it, refuses the whole call in the read tools' miss shape. A place on
//! a not-live volume whose redirectable sibling is live is re-addressed onto that sibling,
//! because the stored group is what runs: storing the dead id would only refuse at approval.
//!
//! A group re-addresses at most one asked volume onto one member, reported as one `via`. A
//! group that would re-address onto two members, or re-address one place and name another
//! member of the same share directly, refuses as `mixedReaddress`: one `via` can't describe it.
//! The rules and the why: `DETAILS.md` § Where a proposal may point.

use super::problem::{GroupProblem, PlaceField};
use crate::location::Location;
use crate::location::miss::{LocationMissResult, volume_not_connected};
use crate::location::resolve::{LocationInput, VolumeFacts, resolve};
use crate::location::same_share::Via;

/// Places one group's locations, remembering what it re-addressed so [`GroupPlacer::finish`]
/// can hold the group to one `via`.
pub(super) struct GroupPlacer<'a> {
    facts: &'a VolumeFacts,
    /// Volumes a place landed on as named.
    direct: Vec<String>,
    /// Every re-address, as (volume asked, member that took it).
    readdressed: Vec<(String, String)>,
}

impl<'a> GroupPlacer<'a> {
    pub(super) fn new(facts: &'a VolumeFacts) -> Self {
        Self {
            facts,
            direct: Vec::new(),
            readdressed: Vec::new(),
        }
    }

    /// Place one location: on its own volume when that's live, else on the first live
    /// redirectable sibling, else the typed miss.
    pub(super) fn place(&mut self, field: PlaceField, input: LocationInput) -> Result<Location, GroupProblem> {
        let input = LocationInput {
            volume_id: input.volume_id.filter(|id| !id.is_empty()),
            path: input.path,
        };
        let resolved = resolve(self.facts, &input).map_err(|miss| missed(field, miss.into()))?;
        if resolved.live {
            self.direct.push(resolved.location.volume_id.clone());
            return Ok(resolved.location);
        }
        let member = self
            .facts
            .readers(&resolved)
            .into_iter()
            .skip(1)
            .find(|reader| reader.live);
        match member {
            Some(member) => {
                self.readdressed
                    .push((resolved.location.volume_id, member.location.volume_id.clone()));
                Ok(member.location)
            }
            None => Err(missed(field, volume_not_connected(self.facts, &resolved))),
        }
    }

    /// Place every source of a hand-listed group on `volume_id`, rewriting each path to where
    /// it was placed, and answer the volume they now share.
    pub(super) fn sources<'p>(
        &mut self,
        field: PlaceField,
        volume_id: &str,
        paths: impl IntoIterator<Item = &'p mut String>,
    ) -> Result<String, GroupProblem> {
        let mut on: Option<String> = None;
        for path in paths {
            let placed = self.place(
                field,
                LocationInput {
                    volume_id: Some(volume_id.to_string()),
                    path: std::mem::take(path),
                },
            )?;
            *path = placed.path;
            if *on.get_or_insert_with(|| placed.volume_id.clone()) != placed.volume_id {
                return Err(GroupProblem::MixedReaddress);
            }
        }
        Ok(on.unwrap_or_else(|| volume_id.to_string()))
    }

    /// The group's one `via`, or `mixedReaddress` when one can't describe what was placed.
    pub(super) fn finish(self) -> Result<Option<Via>, GroupProblem> {
        let Some((asked, member)) = self.readdressed.first() else {
            return Ok(None);
        };
        let one_move = self.readdressed.iter().all(|(a, m)| a == asked && m == member);
        let share = self.facts.same_share_as(asked);
        let named_a_member = self.direct.iter().any(|id| id == asked || share.contains(id));
        if !one_move || named_a_member {
            return Err(GroupProblem::MixedReaddress);
        }
        Ok(Some(Via {
            volume_id: asked.clone(),
        }))
    }
}

fn missed(field: PlaceField, miss: LocationMissResult) -> GroupProblem {
    GroupProblem::Location {
        field,
        miss: Box::new(miss),
    }
}
