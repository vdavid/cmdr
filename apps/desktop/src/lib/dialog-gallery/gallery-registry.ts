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
    status: 'not-triggerable',
    reason: NOT_WIRED_YET,
    states: [],
  },
  {
    dialogId: 'transfer-confirmation',
    label: 'Copy / move confirmation',
    hostWindow: 'main',
    status: 'not-triggerable',
    reason: NOT_WIRED_YET,
    states: [],
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
    status: 'not-triggerable',
    reason: NOT_WIRED_YET,
    states: [],
  },
  {
    dialogId: 'mkdir-confirmation',
    label: 'New folder',
    hostWindow: 'main',
    status: 'not-triggerable',
    reason: NOT_WIRED_YET,
    states: [],
  },
  {
    dialogId: 'new-file-confirmation',
    label: 'New file',
    hostWindow: 'main',
    status: 'not-triggerable',
    reason: NOT_WIRED_YET,
    states: [],
  },
  {
    dialogId: 'rename-conflict',
    label: 'Rename conflict',
    hostWindow: 'main',
    status: 'not-triggerable',
    reason: NOT_WIRED_YET,
    states: [],
  },
  {
    dialogId: 'extension-change',
    label: 'Extension change',
    hostWindow: 'main',
    status: 'not-triggerable',
    reason: NOT_WIRED_YET,
    states: [],
  },
  {
    dialogId: 'archive-password',
    label: 'Archive password',
    hostWindow: 'main',
    status: 'not-triggerable',
    reason: NOT_WIRED_YET,
    states: [],
  },

  // ── Navigation and selection ──────────────────────────────────────────────
  {
    dialogId: 'go-to-path',
    label: 'Go to path',
    hostWindow: 'main',
    status: 'not-triggerable',
    reason: NOT_WIRED_YET,
    states: [],
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
    status: 'not-triggerable',
    reason: NOT_WIRED_YET,
    states: [],
  },
  {
    dialogId: 'selection-remove',
    label: 'Deselect files…',
    hostWindow: 'main',
    status: 'not-triggerable',
    reason: NOT_WIRED_YET,
    states: [],
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
    status: 'not-triggerable',
    reason: NOT_WIRED_YET,
    states: [],
  },
  {
    dialogId: 'mtp-permission',
    label: 'MTP permission (Linux)',
    hostWindow: 'main',
    status: 'not-triggerable',
    reason: NOT_WIRED_YET,
    states: [],
  },
  {
    dialogId: 'ptpcamerad',
    label: 'ptpcamerad conflict',
    hostWindow: 'main',
    status: 'not-triggerable',
    reason: NOT_WIRED_YET,
    states: [],
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
    status: 'not-triggerable',
    reason: NOT_WIRED_YET,
    states: [],
  },
  {
    dialogId: 'license',
    label: 'License key',
    hostWindow: 'main',
    status: 'not-triggerable',
    reason: NOT_WIRED_YET,
    states: [],
  },
  {
    dialogId: 'expiration',
    label: 'License expired',
    hostWindow: 'main',
    status: 'not-triggerable',
    reason: NOT_WIRED_YET,
    states: [],
  },
  {
    dialogId: 'commercial-reminder',
    label: 'Commercial licensing reminder',
    hostWindow: 'main',
    status: 'not-triggerable',
    reason: NOT_WIRED_YET,
    states: [],
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
    status: 'not-triggerable',
    reason: NOT_WIRED_YET,
    states: [],
  },

  // ── File viewer ───────────────────────────────────────────────────────────
  {
    dialogId: 'viewer-copy-confirm',
    label: 'Viewer copy confirmation',
    hostWindow: 'viewer',
    status: 'not-triggerable',
    reason: NOT_WIRED_YET,
    states: [],
  },
  {
    dialogId: 'viewer-copy-refuse',
    label: 'Viewer copy too large',
    hostWindow: 'viewer',
    status: 'not-triggerable',
    reason: NOT_WIRED_YET,
    states: [],
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
