//! How a landing the transport cut off ([`CutOffLanding`]) settles once the
//! wait for its destination ends: [`CutOffLanding::resume`] when the
//! destination is back, [`CutOffLanding::give_up`] when it isn't. Split out of
//! `staged_write_landing.rs`, which owns the landing itself, the cut-off's
//! vocabulary, and the refusal rules a give-up meets; the proofs it settles by
//! live in `interrupted_landing.rs`.

use std::sync::Arc;

use super::super::super::interrupted_landing::{NameHeld, holds_the_source};
use super::super::super::recovered_name::FinalizeFailure;
use super::aside::{
    after_a_refused_aside, give_up_onto_aside, rename_onto_aside, resume_setting_aside,
    run_again_with_the_original_home,
};
use super::{
    CutOffAt, CutOffLanding, Landing, LandingError, LandingSource, Patience, Resumed, after_a_refused_delete,
    clear_the_way, holds_our_size, land, rename_onto_cleared, replace_in_one_step, rescue,
};
use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::volume::{Volume, VolumeError};

impl CutOffLanding {
    /// What cut it off.
    pub(crate) fn error(&self) -> &VolumeError {
        &self.error
    }

    /// The destination is back: settles what the cut-off step did and lands
    /// what's still to land, without streaming the bytes again when the temp is
    /// still whole (`interrupted_landing.rs` has the proofs).
    ///
    /// - **The temp is there and whole**: land it from where the cut-off left
    ///   off. A first rename lands again from the top, and a one-step replace
    ///   runs again (the original is still whole). A clearing delete that
    ///   may have gone through asks the name: empty means it did, and the
    ///   rename alone finishes the landing; taken by the very entry the
    ///   landing noted before its delete ([`NameHeld`]) means it never reached
    ///   the server, and the way is cleared again; taken by anything else means
    ///   it did, and someone saved there since. After a delete that went
    ///   through, ❗ the rename alone, ❌ never another delete: a file
    ///   someone put at the name during the outage makes it refuse, and the
    ///   new bytes are rescued and reported, never their file deleted. A move
    ///   aside asks the aside: there means it moved, and the rename lands; gone
    ///   means it never did, and the move runs again.
    /// - **The temp is gone** and nothing of ours discarded it, so the server
    ///   performed our rename: landed when the name holds what we wrote. A
    ///   first rename's proof is the size (nothing cleared, so nothing else
    ///   could have moved our temp onto a name that was free); after the way
    ///   was cleared, a one-step replace, or a move aside (whose name held the
    ///   original, possibly at the same size), ❗ the name must hold exactly the
    ///   SOURCE's bytes, compared in full, and with no `source` to compare
    ///   against (a one-pass extract) it never counts as landed. A landed swap's
    ///   aside goes; an unproven one's original goes back to its name first. A
    ///   compare the work's stop cut short is [`Resumed::Stopped`]: its records
    ///   stay for the volume's next arrival, ❌ nothing put back or discarded.
    /// - **Anything else** (a temp that isn't whole, a name that doesn't prove
    ///   it): run the file again, a swap's original back at its name first.
    ///
    /// A step of this that meets the transport again cuts the landing off
    /// again; one that meets a refusal settles by the refusal rules.
    pub(crate) async fn resume(self, dest_volume: &Arc<dyn Volume>, source: Option<LandingSource<'_>>) -> Resumed {
        let Self { mut landing, at, error } = self;
        match dest_volume.get_metadata(landing.temp()).await {
            Ok(entry) if !entry.is_directory && !entry.is_symlink && entry.size == Some(landing.size) => {
                resume_with_a_whole_temp(dest_volume, landing, at).await
            }
            // Not the bytes we wrote, so nothing to land. A temp that never
            // left the in-flight set goes; one recorded as a landing stays
            // recorded (the sweep leaves a short one alone).
            Ok(_) => {
                if matches!(
                    at,
                    CutOffAt::FirstRename
                        | CutOffAt::ReplacingRename
                        | CutOffAt::SettingAside
                        | CutOffAt::RenameOntoAside
                ) {
                    landing.discard(dest_volume).await;
                } else {
                    landing.keep_landing();
                }
                run_again_with_the_original_home(dest_volume, landing, error).await
            }
            Err(VolumeError::NotFound(_)) => {
                let proof = match (&at, source) {
                    _ if landing.discard_asked => Ok(false),
                    (CutOffAt::FirstRename, _) => {
                        Ok(holds_our_size(dest_volume, &landing.final_path, landing.size).await)
                    }
                    (_, Some(source)) => {
                        holds_the_source(
                            source.volume,
                            source.path,
                            landing.size,
                            dest_volume,
                            &landing.final_path,
                            &landing.state.stop_work,
                        )
                        .await
                    }
                    (_, None) => Ok(false),
                };
                let Ok(landed) = proof else {
                    log::info!(
                        target: "copy",
                        "landing: the work stopped while {} was compared against its source; leaving it for the volume's next arrival",
                        LogPrivate::file_path(&landing.final_path),
                    );
                    landing.stopped_unproven();
                    return Resumed::Stopped;
                };
                // The temp is gone either way: nothing left to protect.
                landing.settled();
                if landed {
                    log::info!(
                        target: "copy",
                        "landing: {} landed before the drop swallowed the reply; counting it done",
                        LogPrivate::file_path(&landing.final_path),
                    );
                    landing.discard_aside(dest_volume).await;
                    Resumed::Landed
                } else {
                    run_again_with_the_original_home(dest_volume, landing, error).await
                }
            }
            Err(stat) if stat.transport_fault().is_some() => Resumed::CutOff(Box::new(Self {
                landing,
                at,
                error: stat,
            })),
            Err(_) => Self { landing, at, error }.give_up(dest_volume).await.into(),
        }
    }

    /// The wait ended without the destination: it gave up, the person
    /// cancelled, or there was nothing to wait for. The step's error now meets
    /// the refusal rules, as any refusal would: nothing cleared leaves the temp
    /// to the abandoned-write cleanup; a clearing delete takes one stat (taken:
    /// the temp is a spare copy and goes; can't tell: it stays recorded as a
    /// landing for the next sweep that reaches the share); a rename after the
    /// delete asks the temp first (`give_up_onto_cleared`): gone with our bytes
    /// at the name is landed, still there rescues the bytes out of temp space,
    /// and a share that can't say leaves them recorded. A swap that moved the original
    /// aside puts it back, or leaves its aside record for the next sweep when
    /// the share can't say. `Ok` when the share answered after all and the
    /// landing finished.
    pub(crate) async fn give_up(self, dest_volume: &Arc<dyn Volume>) -> Result<(), FinalizeFailure> {
        let Self { landing, at, error } = self;
        let ended = match at {
            CutOffAt::FirstRename | CutOffAt::ReplacingRename => return Err(error.into()),
            CutOffAt::ClearingDelete(first) => {
                after_a_refused_delete(dest_volume, landing, first, error, Patience::GaveUp).await
            }
            CutOffAt::RenameOntoCleared => return give_up_onto_cleared(dest_volume, landing, error).await,
            CutOffAt::SettingAside => after_a_refused_aside(dest_volume, landing, error, Patience::GaveUp).await,
            CutOffAt::RenameOntoAside => return give_up_onto_aside(dest_volume, landing, error).await,
        };
        ended.map_err(|ended| match ended {
            LandingError::Failed(failure) => failure,
            // Unreachable: a landing that gave up cuts nothing off.
            LandingError::CutOff(cut_off) => cut_off.error.into(),
        })
    }
}

/// [`CutOffLanding::give_up`] of the rename onto a cleared name: the temp says
/// whether it happened, so a rename the server performed before its reply was
/// lost is never "rescued" from a temp that's no longer there.
///
/// - **The temp is there**: the rename never happened, and the bytes leave
///   temp space ([`rescue`]).
/// - **The temp is gone** and nothing of ours discarded it: the rename
///   happened, when the name holds exactly what we wrote. Anything else at the
///   name is no proof, and with the temp gone there's nothing left to protect.
/// - **The share can't say**: the landing stays recorded for the next sweep
///   that reaches it, which finishes the landing.
async fn give_up_onto_cleared(
    dest_volume: &Arc<dyn Volume>,
    mut landing: Landing,
    error: VolumeError,
) -> Result<(), FinalizeFailure> {
    match dest_volume.get_metadata(landing.temp()).await {
        Ok(_) => Err(rescue(dest_volume, landing, error).await),
        Err(VolumeError::NotFound(_)) if !landing.discard_asked => {
            let landed = holds_our_size(dest_volume, &landing.final_path, landing.size).await;
            landing.settled();
            if landed { Ok(()) } else { Err(error.into()) }
        }
        _ => Err(error.into()),
    }
}

/// [`CutOffLanding::resume`] with the temp still whole.
async fn resume_with_a_whole_temp(dest_volume: &Arc<dyn Volume>, landing: Landing, at: CutOffAt) -> Resumed {
    match at {
        CutOffAt::FirstRename => land(dest_volume, landing).await.into(),
        CutOffAt::ReplacingRename => replace_in_one_step(dest_volume, landing).await.into(),
        CutOffAt::RenameOntoCleared => rename_onto_cleared(dest_volume, landing, Patience::Waits).await.into(),
        CutOffAt::RenameOntoAside => rename_onto_aside(dest_volume, landing, Patience::Waits).await.into(),
        CutOffAt::SettingAside => resume_setting_aside(dest_volume, landing).await,
        CutOffAt::ClearingDelete(first) => match dest_volume.get_metadata(&landing.final_path).await {
            // The delete went through before the drop.
            Err(VolumeError::NotFound(_)) => rename_onto_cleared(dest_volume, landing, Patience::Waits).await.into(),
            // It never reached the server: what was in the way still is.
            Ok(entry) if landing.in_the_way == Some(NameHeld::of(&entry)) => {
                clear_the_way(dest_volume, landing, first, Patience::Waits).await.into()
            }
            // ❗ Something else is at the name: the delete went through, and
            // someone saved there during the outage. The rename alone, which
            // refuses, so the new bytes are rescued beside their file, ❌ never
            // their file cleared.
            Ok(_) => rename_onto_cleared(dest_volume, landing, Patience::Waits).await.into(),
            Err(stat) if stat.transport_fault().is_some() => Resumed::CutOff(Box::new(CutOffLanding {
                landing,
                at: CutOffAt::ClearingDelete(first),
                error: stat,
            })),
            Err(stat) => CutOffLanding {
                landing,
                at: CutOffAt::ClearingDelete(first),
                error: stat,
            }
            .give_up(dest_volume)
            .await
            .into(),
        },
    }
}
