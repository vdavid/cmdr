//! A swap's landing on a destination without a one-step replace
//! ([`swap_through_an_aside`]): the original renamed aside under a record, the
//! temp renamed onto its name, the aside deleted last, and how a swap the
//! transport cut off settles. Split out of `staged_write_landing.rs`, whose
//! [`Landing`] and rescue rules it shares, and whose cut-off settle
//! (`staged_write_landing_cut_off.rs`) calls its resume and give-up steps.

use std::path::Path;
use std::sync::Arc;

use super::super::super::super::in_flight_temps;
use super::super::super::recovered_name::{AfterARefusedDelete, FinalizeFailure, what_a_refused_delete_left};
use super::super::super::volume::DisplacedDestination;
use super::{
    CutOffAt, CutOffLanding, Landing, LandingError, Patience, Resumed, cut_off, holds_our_size, landing_not_recorded,
    rescue, temp_is_gone,
};
use cmdr_fs::log_error::LogError;
use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::volume::{Volume, VolumeError};

impl Landing {
    /// The swap's aside, recorded and not yet settled.
    fn aside_path(&self) -> Option<&Path> {
        self.aside.as_ref().map(DisplacedDestination::aside_path)
    }

    /// The new bytes are at the name, so the original set aside for them goes.
    /// One the destination won't delete keeps its record, which the volume's
    /// next sweep settles: it removes an aside once the name holds a file of
    /// exactly the replacement's size.
    pub(super) async fn discard_aside(&mut self, dest_volume: &Arc<dyn Volume>) {
        if let Some(aside) = self.aside.take() {
            aside.discard(dest_volume).await;
        }
    }
}

/// A swap on a destination without a one-step replace: record where the
/// original goes, rename it aside, rename the temp onto the name it vacated,
/// and only then delete the aside. The original is whole at one of its two
/// names at every moment, so a crash between any two steps leaves it for the
/// volume's next sweep (`in_flight_sweep.rs`, `ItemKind::FileAside`): put back
/// to an empty name, or removed once the name holds the whole replacement.
///
/// ❗ Every step classifies before acting, as every landing does: a transport
/// fault cuts the swap off for the caller's wait ([`CutOffAt::SettingAside`],
/// [`CutOffAt::RenameOntoAside`]), and a refusal settles at once: a refused
/// move aside leaves the original at its name and takes the spare temp away,
/// and a refused landing puts the original back.
pub(super) async fn swap_through_an_aside(
    dest_volume: &Arc<dyn Volume>,
    mut landing: Landing,
) -> Result<(), LandingError> {
    match DisplacedDestination::hold_for_swap(&landing.state, landing.home(), &landing.final_path, landing.size).await {
        Ok(aside) => landing.aside = Some(aside),
        Err(in_flight_temps::NotOnDisk) => {
            log::warn!(
                target: "copy",
                "landing: couldn't confirm the aside record for {} on disk, so it stays where it is",
                LogPrivate::file_path(&landing.final_path),
            );
            landing.discard(dest_volume).await;
            return Err(LandingError::Failed(landing_not_recorded()));
        }
    }
    set_aside(dest_volume, landing, Patience::Waits).await
}

/// Renames the original onto its recorded aside, then lands the temp.
async fn set_aside(
    dest_volume: &Arc<dyn Volume>,
    mut landing: Landing,
    patience: Patience,
) -> Result<(), LandingError> {
    let Some(aside) = landing.aside_path().map(Path::to_path_buf) else {
        return rename_onto_aside(dest_volume, landing, patience).await;
    };
    match dest_volume.rename(&landing.final_path, &aside, false).await {
        Ok(()) => rename_onto_aside(dest_volume, landing, patience).await,
        // Nothing at the name after all: no original to keep.
        Err(VolumeError::NotFound(_)) => {
            if let Some(aside) = landing.aside.take() {
                aside.forget();
            }
            rename_onto_aside(dest_volume, landing, patience).await
        }
        Err(error) if patience == Patience::Waits && error.transport_fault().is_some() => {
            Err(cut_off(landing, CutOffAt::SettingAside, error))
        }
        Err(refused) => after_a_refused_aside(dest_volume, landing, refused, patience).await,
    }
}

/// The move aside answered `refused`: one stat says whether the original left
/// its name anyway (the server can act and the answer still be lost).
pub(super) async fn after_a_refused_aside(
    dest_volume: &Arc<dyn Volume>,
    mut landing: Landing,
    refused: VolumeError,
    patience: Patience,
) -> Result<(), LandingError> {
    match what_a_refused_delete_left(dest_volume.as_ref(), &landing.final_path).await {
        // It moved: the original is whole at its aside, so land.
        AfterARefusedDelete::Gone => rename_onto_aside(dest_volume, landing, patience).await,
        // Nothing moved: the original keeps its name, and the temp is a spare
        // copy.
        AfterARefusedDelete::StillThere => {
            log::warn!(
                target: "copy",
                "landing: {} stayed at its name after a refused move aside ({refused}); taking the temp {} away",
                LogPrivate::file_path(&landing.final_path),
                LogPrivate::file_path(landing.temp()), refused = LogError(&refused),
            );
            if let Some(aside) = landing.aside.take() {
                aside.forget();
            }
            landing.discard(dest_volume).await;
            Err(LandingError::Failed(refused.into()))
        }
        // Can't tell where the original is: its aside record waits for the
        // volume's next sweep, which puts it back to an empty name (dropping
        // the landing hands it over). The temp stays a removable copy: the
        // original is whole wherever it is.
        AfterARefusedDelete::Unknown => Err(LandingError::Failed(refused.into())),
    }
}

/// The original is aside (or there was none): rename the temp onto its name,
/// then delete the aside.
pub(super) async fn rename_onto_aside(
    dest_volume: &Arc<dyn Volume>,
    mut landing: Landing,
    patience: Patience,
) -> Result<(), LandingError> {
    match dest_volume.rename(landing.temp(), &landing.final_path, false).await {
        Ok(()) => {
            landing.original_replaced();
            landing.settled();
            landing.discard_aside(dest_volume).await;
            Ok(())
        }
        Err(error) if patience == Patience::Waits && error.transport_fault().is_some() => {
            landing.original_replaced();
            Err(cut_off(landing, CutOffAt::RenameOntoAside, error))
        }
        Err(refused) => {
            if temp_is_gone(dest_volume, &landing).await {
                log::info!(
                    target: "copy",
                    "landing: the rename onto {} answered {refused}, but its temp is gone, so it landed",
                    LogPrivate::file_path(&landing.final_path), refused = LogError(&refused),
                );
                landing.original_replaced();
                landing.settled();
                landing.discard_aside(dest_volume).await;
                return Ok(());
            }
            Err(LandingError::Failed(
                put_the_original_back(dest_volume, landing, refused).await,
            ))
        }
    }
}

/// The new bytes couldn't take the name: the original goes back to it, and
/// the temp, a spare copy then, goes. When even that rename refuses, the
/// original stays at its aside, recorded for the volume's next sweep (which
/// puts it back once the name is free), and the new bytes leave temp space to
/// a ` (recovered)` name the failure reports.
async fn put_the_original_back(
    dest_volume: &Arc<dyn Volume>,
    mut landing: Landing,
    refused: VolumeError,
) -> FinalizeFailure {
    let still_aside = match landing.aside.take() {
        Some(aside) => aside.put_back(dest_volume).await,
        None => None,
    };
    match still_aside {
        None => {
            landing.discard(dest_volume).await;
            refused.into()
        }
        Some(aside) => {
            log::warn!(
                target: "copy",
                "landing: {} couldn't take its name or give it back, so the original stays at {} for the volume's next sweep",
                LogPrivate::file_path(&landing.final_path),
                LogPrivate::file_path(aside.aside_path()),
            );
            aside.keep_for_arrival();
            rescue(dest_volume, landing, refused).await
        }
    }
}

/// [`CutOffLanding::resume`] of a move aside, with the temp still whole: the
/// aside says whether the move went through.
pub(super) async fn resume_setting_aside(dest_volume: &Arc<dyn Volume>, landing: Landing) -> Resumed {
    let Some(aside) = landing.aside_path().map(Path::to_path_buf) else {
        return rename_onto_aside(dest_volume, landing, Patience::Waits).await.into();
    };
    match dest_volume.get_metadata(&aside).await {
        // The move aside went through before the drop.
        Ok(_) => rename_onto_aside(dest_volume, landing, Patience::Waits).await.into(),
        // It never reached the server: move it now.
        Err(VolumeError::NotFound(_)) => set_aside(dest_volume, landing, Patience::Waits).await.into(),
        Err(stat) if stat.transport_fault().is_some() => Resumed::CutOff(Box::new(CutOffLanding {
            landing,
            at: CutOffAt::SettingAside,
            error: stat,
        })),
        Err(stat) => CutOffLanding {
            landing,
            at: CutOffAt::SettingAside,
            error: stat,
        }
        .give_up(dest_volume)
        .await
        .into(),
    }
}

/// [`CutOffLanding::give_up`] of the rename onto the name the original
/// vacated: the temp says whether it happened.
pub(super) async fn give_up_onto_aside(
    dest_volume: &Arc<dyn Volume>,
    mut landing: Landing,
    error: VolumeError,
) -> Result<(), FinalizeFailure> {
    match dest_volume.get_metadata(landing.temp()).await {
        // The rename never happened: the original goes back.
        Ok(_) => Err(put_the_original_back(dest_volume, landing, error).await),
        // Our temp is gone from a name that was empty: the rename happened,
        // when the name holds exactly what we wrote.
        Err(VolumeError::NotFound(_))
            if !landing.discard_asked && holds_our_size(dest_volume, &landing.final_path, landing.size).await =>
        {
            landing.settled();
            landing.discard_aside(dest_volume).await;
            Ok(())
        }
        // Can't tell: the aside's record waits for the next sweep, which
        // settles the original against what the name holds.
        _ => Err(error.into()),
    }
}

/// Nothing proves a cut-off landing landed: a swap's original goes back to
/// its name first, so the file runs again over it as the first time.
pub(super) async fn run_again_with_the_original_home(
    dest_volume: &Arc<dyn Volume>,
    mut landing: Landing,
    error: VolumeError,
) -> Resumed {
    if let Some(aside) = landing.aside.take() {
        match dest_volume.get_metadata(aside.aside_path()).await {
            // The move aside never happened.
            Err(VolumeError::NotFound(_)) => aside.forget(),
            _ => {
                if let Some(aside) = aside.put_back(dest_volume).await {
                    log::warn!(
                        target: "copy",
                        "landing: the original of {} couldn't go back to its name, so it stays at {} for the volume's next sweep",
                        LogPrivate::file_path(&landing.final_path),
                        LogPrivate::file_path(aside.aside_path()),
                    );
                    aside.keep_for_arrival();
                    return Resumed::Failed(error.into());
                }
            }
        }
    }
    Resumed::RunAgain(error)
}
