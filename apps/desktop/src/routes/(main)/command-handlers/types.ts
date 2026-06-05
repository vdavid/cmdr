/**
 * The dispatch handler-record seam: the per-dispatch context handlers receive,
 * the handler type, the family-keyed record, and the `DispatchExemptId` union
 * (with its backing runtime tuple) that carves the handler-less ids out of
 * `CommandId`.
 *
 * Import topology: this leaf imports only the `CommandId` / `CommandArgs` types
 * from `$lib/commands` and the `ExplorerAPI` interface from `../explorer-api`.
 * The family handler modules and the dispatch core both import it; it imports
 * neither, so the graph stays acyclic for the `import-cycles` check.
 */
import type { CommandId, CommandArgs } from '$lib/commands'
import type { CommandDispatchContext } from '../command-dispatch-context'
import type { ExplorerAPI } from '../explorer-api'

/**
 * Ids that are registered (for the shortcuts-rebinding UI) but deliberately have
 * NO dispatch handler. Three families; see each member's comment for WHY. This is
 * the ONLY maintained exemption list: the `CommandHandlerRecord` is keyed by
 * `Exclude<CommandId, DispatchExemptId>`, and a runtime set-equality test
 * (`command-handler-record.test.ts`) proves `keys(record) ∪ DISPATCH_EXEMPT_IDS`
 * equals `COMMAND_IDS`, disjoint. Adding a command then forces a decision: a
 * record entry (compile error until added) or an explicit exemption here.
 */
export type DispatchExemptId =
  // Family 1 — Native-menu-owned. macOS PredefinedMenuItems (terminate:, hide:,
  // hideOtherApplications:, unhideAllApplications:) run these via native selectors.
  // A JS handler would double-fire alongside the native one (registry § Decision).
  | 'app.quit'
  | 'app.hide'
  | 'app.hideOthers'
  | 'app.showAll'
  // Family 2 — Per-keystroke, P2-protected. Live path is handleKeyDown → FilePane;
  // these NEVER ride the bus (per-keypress lookup + log + breadcrumb IPC = perf
  // regression). Registered only so the rebinding UI can show/edit their shortcuts.
  // ❌ DO NOT add handlers — that is a P2 violation, not a completion.
  | 'nav.up'
  | 'nav.down'
  | 'nav.left'
  | 'nav.right'
  | 'nav.firstInFull'
  | 'nav.lastInFull'
  // Family 3 — Component-scoped. Handled inside the component that owns the modal /
  // sub-view (CommandPalette, VolumeChooser, NetworkBrowser, ShareBrowser, the
  // context menu), via its own keydown handler — not the global dispatch spine.
  // Registered for the rebinding UI.
  | 'palette.up'
  | 'palette.down'
  | 'palette.execute'
  | 'palette.close'
  | 'volume.select'
  | 'volume.close'
  | 'network.selectHost'
  | 'share.back'
  | 'share.selectShare'
  | 'file.contextMenu'

/**
 * Runtime mirror of `DispatchExemptId` (an `as const` tuple). Backs the
 * set-equality completeness test and is the value the dispatch core never looks
 * up. `satisfies readonly DispatchExemptId[]` keeps the two in lockstep: a tuple
 * member that drifts out of the union fails to compile.
 */
export const DISPATCH_EXEMPT_IDS = [
  // Family 1 — Native-menu-owned.
  'app.quit',
  'app.hide',
  'app.hideOthers',
  'app.showAll',
  // Family 2 — Per-keystroke, P2-protected. ❌ DO NOT add handlers.
  'nav.up',
  'nav.down',
  'nav.left',
  'nav.right',
  'nav.firstInFull',
  'nav.lastInFull',
  // Family 3 — Component-scoped.
  'palette.up',
  'palette.down',
  'palette.execute',
  'palette.close',
  'volume.select',
  'volume.close',
  'network.selectHost',
  'share.back',
  'share.selectShare',
  'file.contextMenu',
] as const satisfies readonly DispatchExemptId[]

/** The id set a handler record must cover: every `CommandId` minus the exempt families. */
export type DispatchableId = Exclude<CommandId, DispatchExemptId>

/**
 * Per-dispatch context, resolved ONCE in the dispatch core before the record
 * lookup. `explorerRef` is read once per dispatch (NOT per handler), preserving
 * the switch's evaluation semantics. Handlers read args off `dispatchArgs` with
 * the same single cast the switch used (the public generic already type-checked
 * the payload at the call site).
 */
export interface CommandHandlerContext {
  explorerRef: ExplorerAPI | undefined
  ctx: CommandDispatchContext
  dispatchArgs: CommandArgs[CommandId] | undefined
}

/**
 * A handler may be sync or async. The dispatch core does `await handler(hctx)`
 * uniformly, which is byte-identical to the old switch for both arm shapes: an
 * arm that `return`s synchronously (sync handler) and an arm that `await`s before
 * returning (async handler resolving after its work). The await-vs-`void`
 * decision lives INSIDE each handler body, exactly as the switch case had it: a
 * fire-and-forget `void` stays `void`, a round-trip `await` stays `await`.
 */
export type CommandHandler = (hctx: CommandHandlerContext) => void | Promise<void>

/**
 * The flat handler record. Keyed by `DispatchableId`, so the compiler forces
 * completeness: a missing handler is a compile error, an extra (exempt-id) key is
 * a compile error. The dispatch core assembles one of these from the family
 * modules and looks up by id.
 */
export type CommandHandlerRecord = Record<DispatchableId, CommandHandler>
