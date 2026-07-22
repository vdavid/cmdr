/**
 * The inventory behind Debug > Soft dialogs.
 *
 * `DIALOG_GALLERY_ENTRIES` covers every id in `SOFT_DIALOG_REGISTRY`, one row each,
 * enforced by the `dialog-gallery-coverage` check. `UNREGISTERED_OVERLAY_ENTRIES`
 * lists the modal-looking overlays that are NOT registered soft dialogs, so the
 * gallery's "complete inventory" claim stays true without over-claiming.
 *
 * Copy here is raw, not i18n: this module is dev-only and lives outside the
 * i18n-enforced areas on purpose. Never add fixture or gallery copy to the message
 * catalog.
 *
 * Adding or wiring an entry: [DETAILS.md](DETAILS.md) § Adding an entry.
 */

import type { SoftDialogId } from '$lib/ui/dialog-registry'

/** The window a dialog actually lives in when the app opens it for real. */
export type GalleryHostWindow = 'main' | 'settings' | 'viewer'

/** One reviewable variant of a dialog. */
export interface DialogGalleryState {
  /** Stable id, unique within the entry. Rides the trigger event to the main window. */
  id: string
  /** Button label in the Debug list. */
  label: string
  /** Optional caveat shown under the button (a side effect, or what the state can't show). */
  note?: string
}

interface DialogGalleryEntryBase {
  /** The registered soft dialog this row previews. */
  dialogId: SoftDialogId
  /** Row label in the Debug list. */
  label: string
  /**
   * Where the dialog lives in the shipping app. The gallery renders every row over
   * the MAIN window, so a `settings` / `viewer` row is showing you the dialog on a
   * backdrop it never has in production. The row says so; that's the deal.
   */
  hostWindow: GalleryHostWindow
  /** Reviewable variants. Empty for a `not-triggerable` row. */
  states: DialogGalleryState[]
  /** Optional caveat that applies to every state of this dialog. */
  note?: string
  /**
   * The dialog does real work on mount (scans, conflict lookups, path
   * resolution), so it runs against a real throwaway directory. The Debug panel
   * creates that directory through a dev-only IPC, ferries its landmarks in the
   * trigger, and the main window navigates the focused pane there. The panel
   * discloses all of that once per row, so the notes don't have to repeat it.
   */
  usesFixtureDir?: boolean
}

export type DialogGalleryEntry = DialogGalleryEntryBase &
  (
    | { status: 'ready'; reason?: never }
    /** `reason` is required, and it's what the Debug row shows instead of buttons. */
    | { status: 'not-triggerable'; reason: string }
  )

/** A modal-looking overlay that is deliberately NOT in `SOFT_DIALOG_REGISTRY`. */
export interface UnregisteredOverlayEntry {
  /** Free-form id: this is not a `SoftDialogId`, and the coverage check ignores it. */
  overlayId: string
  label: string
  hostWindow: GalleryHostWindow
  /** Why it isn't registered, and how to evoke it by hand. */
  reason: string
}

/** Shared wording for a dialog whose fixtures aren't written yet. */
const NOT_WIRED_YET = 'No fixtures written yet, so the gallery can’t open it honestly.'

export const DIALOG_GALLERY_ENTRIES: DialogGalleryEntry[] = [
  // ── Alerts ────────────────────────────────────────────────────────────────
  {
    dialogId: 'alert',
    label: 'Alert',
    hostWindow: 'main',
    status: 'ready',
    states: [
      { id: 'short', label: 'Short message' },
      { id: 'long', label: 'Long message' },
      { id: 'custom-button', label: 'Custom button label' },
      { id: 'long-unbroken-path', label: 'Unbreakable long path' },
    ],
  },

  // ── File operations ───────────────────────────────────────────────────────
  {
    dialogId: 'delete-confirmation',
    label: 'Delete confirmation',
    hostWindow: 'main',
    status: 'ready',
    usesFixtureDir: true,
    note: 'The scan is real: the climbing file and folder tally, the total size, and the throughput line all come from scanning the fixture files. Confirming deletes NOTHING — the delete lives in the onConfirm prop, which the gallery leaves empty.',
    states: [
      { id: 'trash-single', label: 'One item, to Trash' },
      { id: 'trash-many', label: 'Five items, to Trash' },
      { id: 'permanent-single', label: 'One item, permanently' },
      { id: 'permanent-many', label: 'Five items, permanently' },
      {
        id: 'no-trash-support',
        label: 'Volume without Trash',
        note: 'What MTP and most network shares look like: no toggle, permanent only.',
      },
    ],
  },
  {
    dialogId: 'transfer-confirmation',
    label: 'Copy / move confirmation',
    hostWindow: 'main',
    status: 'ready',
    usesFixtureDir: true,
    note: 'Everything the dialog computes runs for real: the source scan, the destination free-space query, and the conflict pre-check (the destination folder already holds entries named like some of the sources, so it finds actual conflicts). Confirming copies or moves NOTHING — the operation lives in the onConfirm prop, which the gallery leaves empty. Switching the toggle to Compress is a fourth state you can reach from any of these.',
    states: [
      { id: 'copy', label: 'Copy five items' },
      { id: 'move', label: 'Move five items' },
      { id: 'copy-single', label: 'Copy one item' },
    ],
  },
  {
    dialogId: 'transfer-progress',
    label: 'Transfer progress',
    hostWindow: 'main',
    status: 'not-triggerable',
    reason:
      'Driven end to end by the backend’s write-progress / write-conflict / write-error / write-cancelled / write-settled stream, so reaching its phases needs a scripted emitter. Its scan phase and the embedded conflict section are part of the same gap.',
    states: [],
  },
  {
    dialogId: 'transfer-error',
    label: 'Transfer error',
    hostWindow: 'main',
    status: 'ready',
    note: 'One state per WriteOperationError variant: the dialog derives its title, explanation, suggestion, icon, container tint, and Retry button entirely from the typed error. Retry just closes the preview, since there is nothing to retry.',
    states: [
      { id: 'source_not_found', label: 'Source not found' },
      { id: 'destination_exists', label: 'Destination exists' },
      { id: 'permission_denied', label: 'Permission denied' },
      { id: 'insufficient_space', label: 'Not enough space' },
      { id: 'same_location', label: 'Same location' },
      { id: 'destination_inside_source', label: 'Destination inside source' },
      { id: 'symlink_loop', label: 'Symlink loop' },
      { id: 'cancelled', label: 'Cancelled' },
      { id: 'device_disconnected', label: 'Device disconnected' },
      { id: 'read_only_device', label: 'Read-only device' },
      { id: 'file_locked', label: 'File locked' },
      { id: 'trash_not_supported', label: 'Trash not supported' },
      { id: 'connection_interrupted', label: 'Connection interrupted' },
      { id: 'read_error', label: 'Read error' },
      { id: 'write_error', label: 'Write error' },
      { id: 'name_too_long', label: 'Name too long' },
      { id: 'invalid_name', label: 'Invalid name' },
      { id: 'delete_pending', label: 'Delete pending' },
      { id: 'files_too_large_for_filesystem', label: 'Too large for the filesystem (three files)' },
      { id: 'files_too_large_for_filesystem-single', label: 'Too large for the filesystem (one file)' },
      { id: 'io_error', label: 'I/O error' },
      { id: 'archive_needs_password', label: 'Archive needs a password' },
    ],
  },
  {
    dialogId: 'mkdir-confirmation',
    label: 'New folder',
    hostWindow: 'main',
    status: 'ready',
    usesFixtureDir: true,
    note: 'This one WRITES. The dialog calls createDirectory() itself, so Create really makes a folder — inside the fixture directory, which is why these rows point there. The conflict check runs against the pane’s live listing, and the AI name suggestions are the real ones (they need a local model to appear).',
    states: [
      { id: 'empty', label: 'Empty name' },
      { id: 'prefilled', label: 'Pre-filled name' },
      { id: 'conflict', label: 'Name that already exists', note: 'A folder really in there, so the warning is live.' },
      { id: 'too-long', label: 'Name past the length limit' },
    ],
  },
  {
    dialogId: 'new-file-confirmation',
    label: 'New file',
    hostWindow: 'main',
    status: 'ready',
    usesFixtureDir: true,
    note: 'This one WRITES too: the dialog calls createFile() itself, inside the fixture directory. No AI suggestions here (that strip is the folder dialog’s), and the conflict check runs against the pane’s live listing.',
    states: [
      { id: 'empty', label: 'Empty name' },
      { id: 'prefilled', label: 'Pre-filled name' },
      { id: 'conflict', label: 'Name that already exists', note: 'A file really in there, so the warning is live.' },
      { id: 'too-long', label: 'Name past the length limit' },
    ],
  },
  {
    dialogId: 'rename-conflict',
    label: 'Rename conflict',
    hostWindow: 'main',
    status: 'ready',
    note: 'The dialog is a comparison: it highlights whichever side is newer and whichever is larger, so both directions are here.',
    states: [
      { id: 'newer-and-larger', label: 'Yours is newer and larger' },
      { id: 'older-and-smaller', label: 'Yours is older and smaller' },
    ],
  },
  {
    dialogId: 'extension-change',
    label: 'Extension change',
    hostWindow: 'main',
    status: 'ready',
    note: 'Ticking “always allow” and confirming writes the real fileOperations.allowFileExtensionChanges setting.',
    states: [
      { id: 'typical', label: '.txt → .zip' },
      { id: 'long-extension', label: 'Long extensions' },
    ],
  },
  {
    dialogId: 'archive-password',
    label: 'Archive password',
    hostWindow: 'main',
    status: 'ready',
    states: [
      { id: 'first-attempt', label: 'First attempt' },
      { id: 'wrong-attempt', label: 'Wrong password re-prompt' },
    ],
  },

  // ── Navigation and selection ──────────────────────────────────────────────
  {
    dialogId: 'go-to-path',
    label: 'Go to path',
    hostWindow: 'main',
    status: 'ready',
    usesFixtureDir: true,
    note: 'Live against the fixture directory: type Photos/exported and the box resolves it for real, type Photos/exported/nope.txt and the nearest-ancestor hint appears. The recent-paths list is your real one, and removing a row removes it for real. “Go to path” closes the preview instead of jumping — the gallery has no navigation behind it — and the clipboard prefill is the real one, so it depends on what you last copied.',
    states: [{ id: 'fixture-dir', label: 'Open' }],
  },
  {
    dialogId: 'search',
    label: 'Search',
    hostWindow: 'main',
    status: 'not-triggerable',
    reason:
      'Nothing blocks it: press ⌘F, or call the MCP open_search_dialog tool, and you have it with a live index. It simply isn’t wired into the gallery.',
    states: [],
  },
  {
    dialogId: 'selection-add',
    label: 'Select files…',
    hostWindow: 'main',
    status: 'ready',
    note: 'Filters a fixture folder snapshot, not the live pane, so committing changes no selection. Recent selections and the AI strip are real (they read the same settings and IPC production does).',
    states: [
      { id: 'mixed-folder', label: 'Mixed folder' },
      { id: 'snapshot-pane', label: 'Search-results snapshot pane' },
      { id: 'empty-folder', label: 'Empty folder' },
    ],
  },
  {
    dialogId: 'selection-remove',
    label: 'Deselect files…',
    hostWindow: 'main',
    status: 'ready',
    note: 'Same component as “Select files…” in remove mode: different title, primary action, and recent-items history.',
    states: [{ id: 'mixed-folder', label: 'Mixed folder' }],
  },

  // ── Ask Cmdr and AI ───────────────────────────────────────────────────────
  {
    dialogId: 'bulk-rename-review',
    label: 'Bulk rename review',
    hostWindow: 'main',
    status: 'not-triggerable',
    reason: NOT_WIRED_YET,
    states: [],
  },
  {
    dialogId: 'delete-ai-model',
    label: 'Delete local AI model',
    hostWindow: 'settings',
    status: 'not-triggerable',
    reason: NOT_WIRED_YET,
    states: [],
  },

  // ── Devices, network, and indexing ────────────────────────────────────────
  {
    dialogId: 'connect-to-server',
    label: 'Connect to server',
    hostWindow: 'main',
    status: 'ready',
    note: 'ONE state, and it has side effects. Opening it starts real mDNS discovery on purpose (the dialog does that in onMount so the macOS Local Network prompt fires alongside the dialog rather than after Connect), so expect that prompt. Its connecting and error states live in internal component state with no prop to reach them, so what you see here is the idle state only. Typing a real address and pressing Connect opens a real socket.',
    states: [{ id: 'idle', label: 'Open' }],
  },
  {
    dialogId: 'mtp-permission',
    label: 'MTP permission (Linux)',
    hostWindow: 'main',
    status: 'ready',
    note: 'Linux-only in the shipping app (it explains a udev rule); the gallery opens it on any platform.',
    states: [{ id: 'default', label: 'Open' }],
  },
  {
    dialogId: 'ptpcamerad',
    label: 'ptpcamerad conflict',
    hostWindow: 'main',
    status: 'ready',
    note: 'The workaround command comes from a real IPC call on mount, so it shows this platform’s command.',
    states: [
      { id: 'known-process', label: 'Named blocking process' },
      { id: 'unknown', label: 'Unknown blocking process' },
    ],
  },
  {
    dialogId: 'drive-index-stale',
    label: 'Stale drive index',
    hostWindow: 'main',
    status: 'not-triggerable',
    reason: NOT_WIRED_YET,
    states: [],
  },

  // ── Licensing and app lifecycle ───────────────────────────────────────────
  {
    dialogId: 'about',
    label: 'About Cmdr',
    hostWindow: 'main',
    status: 'ready',
    note: 'ONE state, and it isn’t a fixture. The license block and version come from the licensing store’s cached status and a version IPC, so you’re reviewing THIS machine’s real license state; a different machine shows different copy. There are no props to override it.',
    states: [{ id: 'default', label: 'Open' }],
  },
  {
    dialogId: 'license',
    label: 'License key',
    hostWindow: 'main',
    status: 'ready',
    note: 'ONE state, and it isn’t a fixture. The dialog takes only callbacks; the existing-license panel, the server-invalid retry, the confirm-reset step, and the loading state all come from the licensing store plus an on-mount IPC, so you get whatever this machine’s license happens to be. Activating or resetting a key here does it for real.',
    states: [{ id: 'default', label: 'Open' }],
  },
  {
    dialogId: 'expiration',
    label: 'License expired',
    hostWindow: 'main',
    status: 'ready',
    note: 'Closing it records the real “expiration modal shown” flag, and Renew opens getcmdr.com in a browser.',
    states: [
      { id: 'organization', label: 'With organization name' },
      { id: 'personal', label: 'Without organization name' },
    ],
  },
  {
    dialogId: 'commercial-reminder',
    label: 'Commercial licensing reminder',
    hostWindow: 'main',
    status: 'ready',
    note: 'Dismissing it records the real dismissal timestamp, so the app won’t remind you again for a while. The other button opens getcmdr.com/pricing in a browser.',
    states: [{ id: 'default', label: 'Open' }],
  },
  {
    dialogId: 'onboarding',
    label: 'Onboarding wizard',
    hostWindow: 'main',
    status: 'not-triggerable',
    reason: NOT_WIRED_YET,
    states: [],
  },
  {
    dialogId: 'whats-new',
    label: 'What’s new',
    hostWindow: 'main',
    status: 'not-triggerable',
    reason: NOT_WIRED_YET,
    states: [],
  },
  {
    dialogId: 'operation-log',
    label: 'Operation log',
    hostWindow: 'main',
    status: 'not-triggerable',
    reason: NOT_WIRED_YET,
    states: [],
  },

  // ── Feedback and diagnostics ──────────────────────────────────────────────
  {
    dialogId: 'feedback',
    label: 'Send feedback',
    hostWindow: 'main',
    status: 'not-triggerable',
    reason: NOT_WIRED_YET,
    states: [],
  },
  {
    dialogId: 'error-report',
    label: 'Error report',
    hostWindow: 'main',
    status: 'not-triggerable',
    reason: NOT_WIRED_YET,
    states: [],
  },
  {
    dialogId: 'crash-report',
    label: 'Crash report',
    hostWindow: 'main',
    status: 'ready',
    note: 'Send uploads nothing in a dev build (the Rust command skips the POST), but it still writes the sticky “always send” / attach-email settings and deletes any real pending crash file. The attach-email row only appears when this machine has a beta contact email on file.',
    states: [
      { id: 'panic', label: 'Panic, with report id' },
      { id: 'signal-no-report-id', label: 'Signal crash, no report id' },
    ],
  },

  // ── File viewer ───────────────────────────────────────────────────────────
  {
    dialogId: 'viewer-copy-confirm',
    label: 'Viewer copy confirmation',
    hostWindow: 'viewer',
    status: 'ready',
    states: [
      { id: 'known-size', label: 'Known size' },
      { id: 'unknown-size', label: 'Unknown size', note: 'A ByteSeek range we never scrolled through.' },
    ],
  },
  {
    dialogId: 'viewer-copy-refuse',
    label: 'Viewer copy too large',
    hostWindow: 'viewer',
    status: 'ready',
    states: [{ id: 'too-large', label: 'Over the limit' }],
  },
]

export const UNREGISTERED_OVERLAY_ENTRIES: UnregisteredOverlayEntry[] = [
  {
    overlayId: 'command-palette',
    label: 'Command palette',
    hostWindow: 'main',
    reason: 'Not in SOFT_DIALOG_REGISTRY: it’s its own overlay, not a ModalDialog. Press ⌘⇧P in the main window.',
  },
  {
    overlayId: 'network-login-form',
    label: 'Network login form',
    hostWindow: 'main',
    reason:
      'Not in SOFT_DIALOG_REGISTRY, and the one sanctioned opt-out from the dialog focus trap. Open a password-protected SMB share from the network browser.',
  },
  {
    overlayId: 'pane-volume-chooser',
    label: 'Pane volume chooser',
    hostWindow: 'main',
    reason:
      'Not in SOFT_DIALOG_REGISTRY: a pane-owned dropdown, not a dialog. Click a pane’s volume breadcrumb, or press ⌥F1 (left) / ⌥F2 (right).',
  },
]
