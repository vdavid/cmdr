//! What an F8 over a selection should run: the trash, or a permanent delete for
//! online-only cloud content. The wire answer every host gives; the routing
//! against the local disk is `cloud_trash.rs`, and a host with no local disk
//! answers [`TrashRoutingAnswer::TRASH`].

/// What an F8 over a given selection should actually run.
///
/// Typed rather than a bare `bool` so the frontend's routing reads as a decision
/// the backend made, and so a future "this one is trashless for another reason"
/// arrives as a variant instead of a second boolean.
///
/// The two delete variants differ only in what the confirmation can honestly
/// SAY. Both run the same permanent delete, and the copy for a wholly online-only
/// selection can't offer "deselect the online-only files" as a remedy, because
/// that would leave nothing selected. Carried in the variant rather than beside
/// it, so there's no "which half of the selection" answer to forget when the
/// routing is `Trash`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum TrashRouting {
    /// Ask the OS trash, and let a refusal answer for itself. The default for
    /// everything, including an unknown or unreachable path.
    Trash,
    /// Every selected item is online-only in a cloud-storage drive. Run the
    /// permanent-delete flow, whose dialog says why.
    PermanentDeleteAllOnlineOnly,
    /// Part of the selection is online-only in a cloud-storage drive, and the
    /// rest isn't. Same permanent delete; the dialog can also suggest dropping
    /// the online-only items from the selection.
    PermanentDeleteMixedOnlineOnly,
}

/// The backend's full answer about a selection: what to run, and whether that
/// answer is final.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
#[allow(
    missing_docs,
    reason = "a wire type: every `///` here reaches `bindings.ts` as JSDoc, so its fields and variants stay as they ship"
)]
pub struct TrashRoutingAnswer {
    pub routing: TrashRouting,
    /// A selected FOLDER sits in a known cloud drive, so `routing` is only what
    /// the top-level items say: an online-only file deeper inside would flip it.
    /// The confirmation dialog already walks that folder for its scan preview,
    /// and that walk is what answers (`scan_walker.rs`'s `online_only_seen`).
    ///
    /// ❗ False for a selection of plain files, where the per-item `lstat` above
    /// IS the whole answer, so the dialog never waits for a walk it doesn't need.
    pub folder_may_hold_online_only: bool,
}

impl TrashRoutingAnswer {
    /// The answer for anything unresolvable, and the starting point every
    /// selection is judged from: today's behavior, with nothing left to wait for.
    pub const TRASH: Self = Self {
        routing: TrashRouting::Trash,
        folder_may_hold_online_only: false,
    };
}
