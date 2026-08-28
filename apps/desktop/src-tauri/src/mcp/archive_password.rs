//! What the encrypted-archive password prompt is asking, mirrored for MCP.
//!
//! The prompt lives in the frontend (`archive-password-flow.svelte.ts`), which
//! is the only place that knows WHICH archive raised it, in which of the two
//! flows, and whether a stored password was just rejected. `SoftDialogTracker`
//! carries the id and nothing else, so without this mirror an agent reads a bare
//! `- type: archive-password` in `cmdr://state` and can neither name the archive
//! nor answer it — the same shape of blind spot a conflict wedge lived in for
//! months.
//!
//! ❌ **The password itself never comes here.** This store holds the QUESTION,
//! never the answer: `unlock_archive` hands the secret straight to the archive
//! volume's `Zeroizing` slot (`commands::file_system::archive`), so nothing a
//! resource renders has ever seen it.

use crate::ignore_poison::RwLockIgnorePoison;
use serde::{Deserialize, Serialize};
use std::sync::RwLock;
use std::sync::atomic::{AtomicU64, Ordering};
use tauri::{AppHandle, Manager};

/// Which flow raised the prompt. The two are genuinely different situations for
/// a caller: unlocking a browse completes it (a listing is a read), while
/// unlocking a transfer only stores the password — the copy that hit the prompt
/// is already settled, and starting another one is a write like any other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ArchivePromptMode {
    /// Listing the archive needs the password: its metadata is encrypted too.
    Browse,
    /// A copy or move out of the archive needs it to read the source entry.
    Transfer,
}

impl ArchivePromptMode {
    /// The mode's own wire name, from its serde representation, so the token an
    /// agent branches on can't drift from the one the frontend sends over IPC.
    pub fn token(self) -> &'static str {
        match self {
            ArchivePromptMode::Browse => "browse",
            ArchivePromptMode::Transfer => "transfer",
        }
    }
}

/// The live archive-password prompt, as the frontend raised it.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ArchivePasswordPrompt {
    /// The archive's display name, the one the dialog shows (`photos.zip`).
    pub archive_name: String,
    /// The path the prompt was raised on, and the one an answer must NAME: the
    /// archive file for a browse, the errored source path (which may be INSIDE
    /// the archive) for a transfer. Both resolve to the same archive volume.
    pub archive_path: String,
    /// The drive the archive lives on. Supplied by the frontend and never by a
    /// caller: an answer names the archive, and the backend supplies the rest.
    pub parent_volume_id: String,
    pub mode: ArchivePromptMode,
    /// Whether a stored password was just rejected. The only thing that makes
    /// the loop closeable from outside: an agent can tell a rejected attempt
    /// from a first ask without watching the dialog.
    pub wrong_attempt: bool,
    /// The operation that hit the prompt, for a `transfer`. Already settled (a
    /// password failure settles the operation rather than parking it), so it is
    /// a correlation handle, not something to resume.
    pub operation_id: Option<String>,
}

/// The single live prompt, or `None` when none is up.
///
/// One slot, not a list: the prompt is modal and blocks operation starts, so a
/// second one can't be raised over it.
#[derive(Debug, Default)]
pub struct ArchivePasswordPromptStore {
    prompt: RwLock<Option<ArchivePasswordPrompt>>,
    generation: AtomicU64,
}

impl ArchivePasswordPromptStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self) -> Option<ArchivePasswordPrompt> {
        self.prompt.read_ignore_poison().clone()
    }

    /// Bumped by every raise AND every dismissal, so a caller can tell that the
    /// frontend acted on its unlock without guessing which way it went.
    ///
    /// ⚠️ This, not "the dialog closed", is `unlock_archive`'s ack. A browse
    /// unlock takes the prompt down and re-lists, and a wrong password puts the
    /// prompt straight back up — fast enough that the dialog may never unmount,
    /// so `SoftDialogDisappeared` can wait out its whole budget on a flow that
    /// worked. A generation moves either way.
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }

    pub fn set(&self, prompt: ArchivePasswordPrompt) {
        *self.prompt.write_ignore_poison() = Some(prompt);
        self.generation.fetch_add(1, Ordering::SeqCst);
    }

    pub fn clear(&self) {
        *self.prompt.write_ignore_poison() = None;
        self.generation.fetch_add(1, Ordering::SeqCst);
    }
}

/// Tauri command: the frontend raised (or re-raised) the prompt.
#[tauri::command]
#[specta::specta]
pub fn notify_archive_password_prompt(app: AppHandle, prompt: ArchivePasswordPrompt) {
    if let Some(store) = app.try_state::<ArchivePasswordPromptStore>() {
        store.set(prompt);
    }
}

/// Tauri command: the prompt is gone (answered, cancelled, or swept).
#[tauri::command]
#[specta::specta]
pub fn notify_archive_password_dismissed(app: AppHandle) {
    if let Some(store) = app.try_state::<ArchivePasswordPromptStore>() {
        store.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prompt(mode: ArchivePromptMode, wrong_attempt: bool) -> ArchivePasswordPrompt {
        ArchivePasswordPrompt {
            archive_name: "photos.zip".to_string(),
            archive_path: "/tmp/photos.zip".to_string(),
            parent_volume_id: "drive-1".to_string(),
            mode,
            wrong_attempt,
            operation_id: None,
        }
    }

    #[test]
    fn the_store_holds_one_prompt_and_gives_it_back() {
        let store = ArchivePasswordPromptStore::new();
        assert!(store.get().is_none());

        store.set(prompt(ArchivePromptMode::Browse, false));
        let held = store.get().expect("the prompt is held");
        assert_eq!(held.archive_name, "photos.zip");
        assert_eq!(held.mode, ArchivePromptMode::Browse);

        store.clear();
        assert!(store.get().is_none());
    }

    #[test]
    fn re_raising_replaces_the_previous_ask() {
        // A rejected password re-raises the SAME prompt with `wrongAttempt` set.
        // Keeping the first one would tell an agent its answer was never tried.
        let store = ArchivePasswordPromptStore::new();
        store.set(prompt(ArchivePromptMode::Transfer, false));
        store.set(prompt(ArchivePromptMode::Transfer, true));

        assert!(store.get().expect("the prompt is held").wrong_attempt);
    }

    #[test]
    fn the_generation_moves_on_a_raise_and_on_a_dismissal_alike() {
        // `unlock_archive` acks on this. A browse unlock re-lists at once, and a
        // wrong password puts the prompt straight back up — sometimes without
        // the dialog ever unmounting, which is why "the dialog closed" is not a
        // usable ack and this is.
        let store = ArchivePasswordPromptStore::new();
        let start = store.generation();

        store.set(prompt(ArchivePromptMode::Browse, false));
        let raised = store.generation();
        assert!(raised > start, "a raise moves it");

        store.clear();
        assert!(store.generation() > raised, "and so does a dismissal");

        store.set(prompt(ArchivePromptMode::Browse, true));
        assert!(
            store.generation() > raised + 1,
            "a re-prompt for the same archive moves it too"
        );
    }

    #[test]
    fn every_mode_token_matches_its_serde_name() {
        // The tokens are what an agent branches on, and the frontend sends the
        // serde ones over IPC. One name per mode, or the two disagree.
        for mode in [ArchivePromptMode::Browse, ArchivePromptMode::Transfer] {
            let serde_name = serde_json::to_value(mode).expect("the mode serializes");
            assert_eq!(serde_name.as_str(), Some(mode.token()), "for {mode:?}");
        }
    }
}
