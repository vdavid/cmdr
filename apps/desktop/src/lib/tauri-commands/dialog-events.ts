// Window-management event listeners. Typed `on*` wrappers over
// the `tauri-specta` `events.*` helpers for the `emit_to`-targeted window
// lifecycle events: the MCP `dialog` tool's open/focus/close round-trips, the
// unified `execute-command` menu/cross-window relay, the tab context menu, the
// settings-window self-close, the viewer word-wrap toggle, and the viewer's
// restricted-settings forward.
//
// Payloadless events (unit structs → `type X = null`) wrap a `() => void`
// callback; the rest hand the typed payload through.

import { type UnlistenFn } from '@tauri-apps/api/event'
import {
  events,
  type CloseFileViewer,
  type ExecuteCommand,
  type FocusFileViewer,
  type OpenFileViewer,
  type OpenSettings,
  type PersistRestrictedSetting,
} from '$lib/ipc/bindings'

/**
 * The unified menu / cross-window command relay. Rust's `on_menu_event` and the
 * MCP dialog/app tools emit this to the main window; the settings window's
 * License section emits it too (via `emitExecuteCommand`). `commandId` is a bare
 * string across IPC; the main-window listener narrows it to a registry
 * `CommandId` before dispatching.
 */
export function onExecuteCommand(handler: (payload: ExecuteCommand) => void): Promise<UnlistenFn> {
  return events.executeCommand.listen((event) => {
    handler(event.payload)
  })
}

/**
 * Emits an `execute-command` cross-window relay. Used by the settings window's
 * License section to dispatch a command into the main window. Only the main
 * window listens for `execute-command`, so the broadcast reaches exactly it.
 * The `commandId` must stay a valid registry `CommandId` (the main-window
 * listener narrows it; `rust-command-id-drift.test.ts` pins it).
 */
export function emitExecuteCommand(commandId: string): Promise<void> {
  return events.executeCommand.emit({ commandId })
}

/**
 * MCP `dialog open settings --section …`: open settings deep-linked to a section.
 *
 * The MCP path always carries a `section`, but the same `open-settings` event is
 * also emitted bare (no payload) to open settings at its default landing section
 * (the E2E `openSettingsWindowViaProd` helper does this). A bare emit delivers a
 * `null` payload, which the typed `OpenSettings` (`{ section: string }`) doesn't
 * model, so the handler receives an optional section and the caller defaults it.
 */
/**
 * Ask the main window to open Settings deep-linked to `section`. For windows that
 * lack window-creation capability (the read-only Keyboard shortcuts help window):
 * the main window owns `openSettingsWindow` and reacts via `onOpenSettings`, so the
 * help window stays minimally privileged (it can't spawn windows itself). Reuses
 * the same `open-settings` channel the MCP `dialog open settings` path uses.
 */
export function requestOpenSettings(section: string): Promise<void> {
  return events.openSettings.emit({ section })
}

export function onOpenSettings(handler: (payload: Partial<OpenSettings>) => void): Promise<UnlistenFn> {
  return events.openSettings.listen((event) => {
    // The generated type says `event.payload` is always `OpenSettings`, but a bare
    // `open-settings` emit (no payload) delivers `null` at runtime, so the guard is real.
    // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- bare emit yields a null payload the type doesn't model
    handler(event.payload ?? {})
  })
}

/** MCP `dialog open file-viewer`: `path` present → open that file; absent → cursor file. */
export function onOpenFileViewer(handler: (payload: OpenFileViewer) => void): Promise<UnlistenFn> {
  return events.openFileViewer.listen((event) => {
    handler(event.payload)
  })
}

/** MCP `dialog focus settings`: bring the settings window forward. */
export function onFocusSettings(handler: () => void): Promise<UnlistenFn> {
  return events.focusSettings.listen(() => {
    handler()
  })
}

/** MCP `dialog focus file-viewer`: `path` present → that file's viewer; absent → the most recent. */
export function onFocusFileViewer(handler: (payload: FocusFileViewer) => void): Promise<UnlistenFn> {
  return events.focusFileViewer.listen((event) => {
    handler(event.payload)
  })
}

/** MCP `dialog focus about`: ensure the (soft, overlay) about dialog is visible. */
export function onFocusAbout(handler: () => void): Promise<UnlistenFn> {
  return events.focusAbout.listen(() => {
    handler()
  })
}

/** MCP `dialog focus <confirmation>`: focus the main window so the overlay is visible. */
export function onFocusConfirmation(handler: () => void): Promise<UnlistenFn> {
  return events.focusConfirmation.listen(() => {
    handler()
  })
}

/** MCP `dialog close file-viewer` (with path): close that file's viewer. */
export function onCloseFileViewer(handler: (payload: CloseFileViewer) => void): Promise<UnlistenFn> {
  return events.closeFileViewer.listen((event) => {
    handler(event.payload)
  })
}

/** MCP `dialog close file-viewer` (no path): close every open viewer. */
export function onCloseAllFileViewers(handler: () => void): Promise<UnlistenFn> {
  return events.closeAllFileViewers.listen(() => {
    handler()
  })
}

/** MCP `dialog close about`: dismiss the about overlay. */
export function onCloseAbout(handler: () => void): Promise<UnlistenFn> {
  return events.closeAbout.listen(() => {
    handler()
  })
}

/** MCP `dialog close <confirmation>`: cancel the open confirmation overlay. */
export function onCloseConfirmation(handler: () => void): Promise<UnlistenFn> {
  return events.closeConfirmation.listen(() => {
    handler()
  })
}

/**
 * MCP `dialog close settings`: the settings window listens for this and closes
 * itself. Lives in the settings window's `+page.svelte`.
 */
export function onMcpSettingsClose(handler: () => void): Promise<UnlistenFn> {
  return events.mcpSettingsClose.listen(() => {
    handler()
  })
}

/**
 * The View > Word wrap menu item was clicked while a viewer window had focus.
 * Emitted to that specific viewer's label; the viewer window listens.
 */
export function onViewerWordWrapToggled(handler: () => void): Promise<UnlistenFn> {
  return events.viewerWordWrapToggled.listen(() => {
    handler()
  })
}

/**
 * The viewer (a restricted-capability window with no store access) forwarded an
 * allowlisted setting write; the main window persists it. Consumed by
 * `restricted-settings-bridge.ts`.
 */
export function onPersistRestrictedSetting(handler: (payload: PersistRestrictedSetting) => void): Promise<UnlistenFn> {
  return events.persistRestrictedSetting.listen((event) => {
    handler(event.payload)
  })
}
