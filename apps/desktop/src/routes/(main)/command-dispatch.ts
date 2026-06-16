/**
 * Command dispatch: maps command IDs from the command palette, keyboard shortcuts,
 * and menu actions to concrete app actions.
 *
 * This is the dispatch CORE: it runs the preamble (text-region intercept →
 * `log.info` → breadcrumb → close palette → capability guard) in order, builds the
 * per-dispatch context once, then looks up the id in the flat
 * `commandHandlers` record and awaits the handler. The handlers themselves live
 * in `command-handlers/`, grouped by family. Ids with no handler are the
 * `DispatchExemptId` families (native-menu-owned, per-keystroke P2, and
 * component-scoped); they silently no-op after the preamble.
 */
import { recordBreadcrumb } from '$lib/error-reporter/breadcrumbs'
import { addToast } from '$lib/ui/toast'
import { SEARCH_RESULTS_NOT_A_FOLDER_TOAST } from '$lib/search/capabilities'
import { getAppLogger } from '$lib/logging/logger'
import { getFocusedPaneVolumeId } from '$lib/file-explorer/pane/focused-pane-reads'
import { capabilitiesFor } from '$lib/file-explorer/pane/volume-capabilities'
import type { CommandId, CommandArgs, CommandDispatchArgs } from '$lib/commands'
import type { ExplorerAPI } from './explorer-api'
import { commandHandlers } from './command-handlers'
import type { CommandHandler } from './command-handlers'
import type { CommandDispatchContext } from './command-dispatch-context'
import { shouldDropCrossSourceDuplicate } from './dispatch-dedup'

// Re-exported so existing importers (`+page.svelte`, the dispatch tests) keep
// resolving these from `./command-dispatch` after the move to the context leaf.
export type { CommandDispatchContext, CommandDispatchDialogs } from './command-dispatch-context'

const log = getAppLogger('user-action')

/**
 * Returns the closest selectable-text container (e.g. `ErrorPane`) the current text
 * selection sits in, or `null` if the selection isn't inside one. Even a collapsed
 * selection counts (the user clicked into the region), so ⌘A works without prior
 * highlighting. Add `data-text-region` to opt new components into this routing.
 */
function activeTextRegion(): Element | null {
  const sel = window.getSelection()
  const anchor = sel?.anchorNode
  if (!anchor) return null
  const el = anchor.nodeType === Node.ELEMENT_NODE ? (anchor as Element) : anchor.parentElement
  return el?.closest('.error-pane, [data-text-region]') ?? null
}

/**
 * Returns `true` (and surfaces a toast) when `commandId` is a destination-side
 * action the focused pane's volume capabilities can't satisfy. Reads the focused
 * pane's `VolumeCapabilities` (one source, shared with the F-bar's `disabled`
 * flags and the menu items) instead of a `volumeId === 'search-results'` string
 * compare (invariant A6): paste/pasteAsMove gate on `!canPasteInto`,
 * newFolder/newFile on `!canCreateChild`, rename on `!canRenameInPlace`.
 * Source-side actions (copy/move/delete) stay enabled (`canBeSource: true`).
 *
 * Menu paths are disabled at the source (F-bar, context-menu items), so this
 * guard exists for the shortcut path (⌘V, F7, etc.) that bypasses the UI. The
 * toast is the LAST RESORT — so the user isn't left wondering whether the
 * keystroke registered.
 *
 * The toast fires ONLY for the `search-results` kind (PR3 byte-identical
 * behavior). A `network`-kind pane shares the same `false` destination caps, but
 * those ops are unreachable through the UI and the shortcut path historically
 * fell through SILENTLY to the explorer call, which no-ops deep down (no listing
 * id ⇒ mkdir/mkfile bail, no cursor row ⇒ rename returns, paste hits the
 * "No files on the clipboard" path). The search-results-worded toast there would
 * be a NEW, mis-worded toast, so network keeps its prior silence: the capability
 * decides the BLOCK; the kind decides the TOAST.
 */
function blockedByCapabilities(commandId: CommandId, explorer: ExplorerAPI | undefined): boolean {
  if (!explorer) return false

  const caps = capabilitiesFor(getFocusedPaneVolumeId())
  // The snapshot pane is the only kind whose destination-op block produces the
  // user-facing toast; other kinds with false caps fall through as before.
  if (caps.kind !== 'search-results') return false

  const isBlocked =
    ((commandId === 'edit.paste' || commandId === 'edit.pasteAsMove') && !caps.canPasteInto) ||
    ((commandId === 'file.newFolder' || commandId === 'file.newFile') && !caps.canCreateChild) ||
    (commandId === 'file.rename' && !caps.canRenameInPlace)
  if (!isBlocked) return false

  addToast(SEARCH_RESULTS_NOT_A_FOLDER_TOAST, { level: 'info' })
  return true
}

/**
 * Intercepts text-region shortcuts (⌘C, ⌘A) BEFORE the dispatcher logs or records
 * the action, so selecting text in the ErrorPane and copying it doesn't pollute the
 * user-action log used for rollback context, and doesn't fire file-scope side
 * effects (copy files, select all files). Returns `true` if the shortcut was handled.
 *
 * For `edit.copy` we only intercept when the selection is non-collapsed (something is
 * actually selected); otherwise we fall through so the file copy path can run.
 */
function handleTextRegionShortcut(commandId: CommandId): boolean {
  if (commandId !== 'edit.copy' && commandId !== 'selection.selectAll') return false
  const region = activeTextRegion()
  if (!region) return false

  if (commandId === 'edit.copy') {
    const text = window.getSelection()?.toString() ?? ''
    if (!text) return false
    void navigator.clipboard.writeText(text)
    return true
  }

  // selection.selectAll: replace the current selection with the whole region.
  // Includes hidden content inside collapsed <details>, which is what the user
  // actually wants when copying error context (technical details included).
  const range = document.createRange()
  range.selectNodeContents(region)
  const sel = window.getSelection()
  sel?.removeAllRanges()
  sel?.addRange(range)
  return true
}

/**
 * Typed dispatch entry point. The generic `K` keeps the public signature
 * arg-checked per command (arg-less ids take no second argument; arg-carrying
 * ones like `view.setMode` require their typed payload). Inside, `commandId`
 * widens back to the `CommandId` union for the record lookup, and the single arg
 * payload is read from `dispatchArgs` by the matched handler.
 */
export async function handleCommandExecute<K extends CommandId>(
  commandId: K,
  ctx: CommandDispatchContext,
  ...args: CommandDispatchArgs<K>
): Promise<void> {
  // Widen the generic so the id is the full union for the record lookup (a
  // generic `K` doesn't index the record). The lone arg payload, if any, is read
  // from `dispatchArgs` by the matched handler.
  const id: CommandId = commandId
  const dispatchArgs: CommandArgs[CommandId] | undefined = args[0]

  // Swallow the spurious second half of a macOS keyboard+menu double-fire
  // before anything else runs (no double log, no double breadcrumb, no toggle
  // flip-back). Same-source repeats and untagged dispatches always pass; see
  // dispatch-dedup.ts for the source-pair rationale.
  if (shouldDropCrossSourceDuplicate(id)) {
    log.debug('Dropped cross-source duplicate dispatch of {id}', { id })
    return
  }

  const explorerRef = ctx.getExplorer()

  // Bail before logging if the user's intent is text manipulation in a selectable
  // region. Native menu accelerators (⌘C, ⌘A) flow through here even when focus is
  // outside the file pane, so without this guard every text copy would log
  // `edit.copy` / `selection.selectAll` and trigger file-scope behavior.
  if (handleTextRegionShortcut(id)) return

  // Every keyboard / palette / menu command flows through here. Two channels:
  // - Info-level structured log → LogTape → Rust bridge → fern file chain, so the
  //   line appears alongside backend logs in error-report bundles.
  // - A `kind: "command"` breadcrumb → the manifest's rolling buffer, so triagers
  //   see what the user did right before an error fired.
  log.info(id)
  recordBreadcrumb('command', id)

  ctx.dialogs.showCommandPalette(false)

  // Block destination-side actions the focused pane's capabilities can't satisfy
  // with a friendly toast. Menu paths are visibly disabled at the source; this
  // catches the shortcut-driven path that bypasses the UI.
  if (blockedByCapabilities(id, explorerRef)) return

  // Look up the family handler. An id with no handler is one of the 20
  // `DispatchExemptId`s (see `command-handlers/types.ts`): native-menu-owned,
  // per-keystroke P2, or component-scoped. Those silently no-op after the
  // preamble, byte-identical to the old switch falling off its end. The record's
  // `Exclude<CommandId, DispatchExemptId>` key type guarantees only those 20 ids
  // reach this guard.
  const handler = (commandHandlers as Partial<Record<CommandId, CommandHandler>>)[id]
  if (!handler) return

  await handler({ explorerRef, ctx, dispatchArgs })
}
