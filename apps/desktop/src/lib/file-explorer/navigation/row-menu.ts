/**
 * A row's actions: the ONE list every door to them shows. The volume switcher renders it as
 * the row's → submenu (right-click opens the same submenu), the favorites menu does the same
 * for a favorite, and the servers hub renders a one-place server's list at the pointer.
 *
 * ❗ **One list, so no two surfaces can drift** on which items a row has, their order, or
 * which ones a running transfer greys. The busy and ejecting answers are Rust's own
 * (`busy_volume_ids()` and the eject flight set, pushed into `volume-busy-store.svelte.ts`),
 * the same inputs `detachControlFor` reads for the row's inline Eject button, so the button
 * and the menu item three pixels apart can't disagree. The real guard stays in Rust
 * (`eject_volume` refuses a busy volume) and in `runDetach`; greying is the honest preview.
 *
 * `VolumeContextActionKind` stays the action vocabulary, so a pick lands in the handlers the
 * palette and the breadcrumb's native Eject already use.
 *
 * **Three named groups**, in this order (`RowMenu`): the row's ACTIONS on the volume (Open,
 * Eject, Disconnect, the pin, the forgets), then one-shot FIXES for the row's current state
 * ("Connect directly now"), then, below a rule, per-row SETTINGS (checkboxes). A fix shows
 * only while its state needs it, and reads as something to do now, so it runs straight on
 * from the actions; a setting is a standing choice, so it gets the rule. A new item goes
 * into its group's builder (`serverActions` / `detachAction`, `rowFixes`, `rowToggles`), ❌
 * never spliced in by position.
 *
 * Adding a row's checkbox (a per-row setting): give `RowToggleKind` a member, give
 * `VolumeRowFacts` the value it shows, push a `toggle` entry in `rowToggles`, and handle
 * the kind in the two `flipToggle` maps (`VolumeChooserMenu.svelte` and
 * `../network/servers-hub-actions.ts`), each a `Record<RowToggleKind, …>` that stops
 * compiling until you do. The surfaces fill the fact from their stores.
 *
 * Adding a fix: give `RowFixKind` a member, push a `fix` entry in `rowFixes` gated on the
 * state it repairs, and give `runRowFix`'s `Record<RowFixKind, …>` its runner. Every surface
 * already sends a `fix` pick to `runRowFix`.
 */

import type { VolumeContextActionKind } from '$lib/ipc/bindings'
import type { MessageKey } from '$lib/intl/keys.gen'
import { tString } from '$lib/intl/messages.svelte'
import type { IconName } from '$lib/ui/icons/icon-map'
import type { MenuItem, MenuSection } from '$lib/ui/menu-types'
import { connectDirectly } from '../network/direct-connect'
import { showsDisconnect } from './connection-state'
import { detachControlFor } from './detach-control'
import { runDetach } from './detach-volume'
import { isServerPlaceRow, runServerRowAction } from './server-row-actions'
import { volumeScheme } from '$lib/volume-scheme'
import type { VolumeInfo } from '../types'

/** A per-row switch the submenu carries as a checkbox row. */
export type RowToggleKind = 'direct-connection' | 'auto-reconnect'

/** A one-shot repair a row offers only while its current state needs it. */
export type RowFixKind = 'connect-directly'

/** An action on the volume itself. */
export interface RowActionEntry {
  type: 'action'
  action: VolumeContextActionKind
  label: string
  icon: IconName
  disabled?: boolean
  /** Leaves the menu up after the pick (see `MenuItem.keepsMenuOpen`). */
  keepsMenuOpen?: boolean
}

/** A one-shot fix: runs once, now. Closes the menu, since a fix may raise a sheet. */
export interface RowFixEntry {
  type: 'fix'
  fix: RowFixKind
  label: string
  icon: IconName
  /** Why the fix is on offer, since it appears and disappears with the row's state. */
  tooltip: string
}

/** A per-row setting, drawn as a checkbox. */
export interface RowToggleEntry {
  type: 'toggle'
  toggle: RowToggleKind
  label: string
  checked: boolean
  /** What the switch does, where the label alone invites a wrong reading. */
  tooltip?: string
}

/** One row of a row's menu: an action to run, a fix to apply, or a switch to flip. */
export type RowMenuEntry = RowActionEntry | RowFixEntry | RowToggleEntry

/** A row's menu in its three named groups (the header says why this order). */
export interface RowMenu {
  actions: RowActionEntry[]
  fixes: RowFixEntry[]
  settings: RowToggleEntry[]
}

/** A row that offers nothing. Spread it and replace a group, ❌ never push into one: the arrays are shared. */
export const EMPTY_ROW_MENU: RowMenu = { actions: [], fixes: [], settings: [] }

/**
 * The groups as the menu draws them, a rule between each two: the actions and the fixes
 * share a block (both are things to do), and the settings sit below a rule. Empty blocks
 * drop out, so a lone block draws no rule.
 */
function ruledBlocks(menu: RowMenu): RowMenuEntry[][] {
  const blocks: RowMenuEntry[][] = [[...menu.actions, ...menu.fixes], menu.settings]
  return blocks.filter((block) => block.length > 0)
}

/** What the row's own fields can't say, read by the surface from its stores. */
export interface VolumeRowFacts {
  /** A transfer reads from or writes to the volume (Rust's `busy_volume_ids()`). */
  busy: boolean
  /** The volume's eject is still running. */
  ejecting: boolean
  /** A saved server entry backs this row, so Edit and Forget server have a subject. */
  isSaved: boolean
  /** The SMB share's direct-connection switch, or `undefined` until Rust answered for it. */
  directConnection: boolean | undefined
  /**
   * A saved SFTP or WebDAV place's "Reconnect automatically" switch (`SavedServer.autoReconnect`),
   * or `undefined` where nothing saved backs the row: a one-shot connection has nothing to persist.
   */
  autoReconnect: boolean | undefined
}

/** A pick a surface hands back: the entry, and the volume it acts on. */
export interface RowMenuPick {
  kind: 'row-entry'
  volume: VolumeInfo
  entry: RowMenuEntry
}

function action(
  kind: VolumeContextActionKind,
  labelKey: MessageKey,
  icon: IconName,
  options: { disabled?: boolean; keepsMenuOpen?: boolean } = {},
): RowActionEntry {
  return { type: 'action', action: kind, label: tString(labelKey), icon, ...options }
}

/**
 * A server place's actions: Open, Edit… (when saved), Disconnect (when there's a session),
 * Pin to switcher / Unpin, Forget saved password, Forget server (when saved).
 *
 * ❗ A server says Disconnect, ❌ never Eject: "Eject" promises safe-to-unplug, and a server has
 * nothing to unplug. ❗ "Forget saved password" is offered unconditionally, ❌ never gated on "is
 * one stored?": answering that costs a Keychain read, which can raise a system prompt, so the
 * command answers instead (`forgetSavedSecret` words a `false`). Open, Edit, and the pin are
 * never greyed by a transfer: navigating, editing settings, and moving a pin break nothing.
 */
function serverActions(volume: VolumeInfo, facts: VolumeRowFacts): RowActionEntry[] {
  const { busy, isSaved } = facts
  const entries: RowActionEntry[] = [action('open', 'menu.network.open', 'arrow-right')]
  if (isSaved) entries.push(action('edit', 'menu.network.edit', 'pencil'))
  if (showsDisconnect(volume.connectionState)) {
    entries.push(
      action('disconnect', busy ? 'menu.volume.disconnectBusy' : 'menu.network.disconnect', 'unplug', {
        disabled: busy,
        keepsMenuOpen: true,
      }),
    )
  }
  entries.push(pinAction(volume))
  entries.push(
    action('forget-secret', busy ? 'menu.volume.forgetSavedPasswordBusy' : 'menu.network.forgetSavedPassword', 'key', {
      disabled: busy,
    }),
  )
  if (isSaved) {
    entries.push(
      action('forget-server', busy ? 'menu.volume.forgetServerBusy' : 'menu.network.forgetServer', 'trash-2', {
        disabled: busy,
      }),
    )
  }
  return entries
}

/** Pin to switcher, or Unpin. Never greyed: moving a pin breaks nothing. */
function pinAction(volume: VolumeInfo): RowActionEntry {
  return volume.pinned === true
    ? action('unpin', 'menu.network.unpin', 'pin-off', { keepsMenuOpen: true })
    : action('pin', 'menu.network.pinToSwitcher', 'pin', { keepsMenuOpen: true })
}

/**
 * A drive's or a phone's detach item, worded and greyed from the same `detachControlFor`
 * answer the row's inline button renders. `null` when the row has no detach at all.
 */
function detachAction(volume: VolumeInfo, facts: VolumeRowFacts): RowActionEntry | null {
  const detach = detachControlFor(volume, { busy: facts.busy, ejecting: facts.ejecting })
  if (detach?.action !== 'eject') return null
  const { disabled, icon } = detach.button
  // The phone's glyph is `unplug`, and so is its word: `adb` detaches nothing to unplug.
  const label =
    icon === 'unplug'
      ? tString(disabled ? 'menu.volume.disconnectBusy' : 'menu.network.disconnect')
      : tString(disabled ? 'menu.volume.ejectBusy' : 'menu.volume.eject', { name: volume.name })
  return { type: 'action', action: 'eject', label, icon, disabled, keepsMenuOpen: true }
}

/**
 * The fixes group: one-shot repairs, each gated on the state it repairs.
 *
 * "Connect directly now" shows ONLY while the share's switch is ON and the share is still
 * on the macOS mount (the auto upgrade couldn't dial: no saved credentials, the server
 * asleep, the pane-open cooldown). With the switch OFF, checking it already connects, and
 * a direct share has nothing to fix, so in both the checkbox stands alone.
 */
function rowFixes(volume: VolumeInfo, facts: VolumeRowFacts): RowFixEntry[] {
  const fixes: RowFixEntry[] = []
  if (facts.directConnection === true && volume.connectionState === 'os_mount') {
    fixes.push({
      type: 'fix',
      fix: 'connect-directly',
      label: tString('fileExplorer.navigation.connectDirectlyNow'),
      icon: 'zap',
      tooltip: tString('fileExplorer.navigation.connectDirectlyNowTooltip'),
    })
  }
  return fixes
}

/** The switches group: per-row settings, below a rule. */
function rowToggles(facts: VolumeRowFacts): RowToggleEntry[] {
  const toggles: RowToggleEntry[] = []
  if (facts.directConnection !== undefined) {
    toggles.push({
      type: 'toggle',
      toggle: 'direct-connection',
      label: tString('fileExplorer.navigation.useDirectConnection'),
      checked: facts.directConnection,
    })
  }
  if (facts.autoReconnect !== undefined) {
    // The sign-in sheet's words, label and explanation both, so the two doors to the one
    // setting can't drift apart in meaning (`$lib/servers/DETAILS.md`).
    toggles.push({
      type: 'toggle',
      toggle: 'auto-reconnect',
      label: tString('servers.sheet.autoReconnect'),
      checked: facts.autoReconnect,
      tooltip: tString('servers.sheet.autoReconnectHelp'),
    })
  }
  return toggles
}

/** A volume-switcher (or servers-hub) row's menu. Empty when the row offers nothing. */
export function volumeRowMenu(volume: VolumeInfo, facts: VolumeRowFacts): RowMenu {
  const actions: RowActionEntry[] = []
  if (isServerPlaceRow(volume)) {
    actions.push(...serverActions(volume, facts))
  } else {
    const detach = detachAction(volume, facts)
    if (detach) actions.push(detach)
    // A SAVED SMB share is pinnable like a server place (`docs/specs/saved-smb-shares.md`);
    // a mount nothing saved (Finder's) has no pin to move.
    if (volume.category === 'network' && volumeScheme(volume.id) === 'smb' && facts.isSaved)
      actions.push(pinAction(volume))
  }
  return { actions, fixes: rowFixes(volume, facts), settings: rowToggles(facts) }
}

/**
 * A favorite's menu: Rename, Set shortcut, and Remove from favorites. All keep the menu up:
 * the editors happen in the row itself, and a list being tidied is several removals in a row. Never
 * greyed: a favorite is a stored `{ path, name }` pair nothing can be using.
 */
export function favoriteRowMenu(): RowMenu {
  return {
    ...EMPTY_ROW_MENU,
    actions: [
      action('rename-favorite', 'menu.volume.renameFavorite', 'pencil', { keepsMenuOpen: true }),
      action('edit-favorite-shortcut', 'menu.volume.editFavoriteShortcut', 'key', { keepsMenuOpen: true }),
      action('remove-favorite', 'menu.volume.removeFavorite', 'star-off', { keepsMenuOpen: true }),
    ],
  }
}

/** The entry's name within its row: the action, or the fix or toggle behind its prefix. */
function entryKey(entry: RowMenuEntry): string {
  if (entry.type === 'action') return entry.action
  if (entry.type === 'fix') return `fix:${entry.fix}`
  return `toggle:${entry.toggle}`
}

/**
 * One entry as a house `Menu` row. Its value is unique across the whole menu
 * (`row:<volumeId>:<entry>`, since every row's submenu shares one value space), and
 * `wrap(entry)` is the payload a pick hands back.
 */
function entryItem<T>(volumeId: string, entry: RowMenuEntry, wrap: (entry: RowMenuEntry) => T): MenuItem<T> {
  const common = { value: `row:${volumeId}:${entryKey(entry)}`, label: entry.label, data: wrap(entry) }
  if (entry.type === 'action') {
    return { ...common, icon: { lucide: entry.icon }, disabled: entry.disabled, keepsMenuOpen: entry.keepsMenuOpen }
  }
  if (entry.type === 'fix') return { ...common, icon: { lucide: entry.icon }, tooltip: entry.tooltip }
  return { ...common, check: { kind: 'toggle', checked: entry.checked }, tooltip: entry.tooltip }
}

/**
 * A row's menu as a SUBMENU (the switcher's and the favorites menu's rows): one flat list,
 * with a rule above each block after the first (`ruledBlocks`). `undefined` for a row with
 * nothing to offer, so it draws no submenu arrow.
 */
export function rowMenuItems<T>(
  volumeId: string,
  menu: RowMenu,
  wrap: (entry: RowMenuEntry) => T,
): MenuItem<T>[] | undefined {
  const items = ruledBlocks(menu).flatMap((group, groupIndex) =>
    group.map((entry, index) => ({
      ...entryItem(volumeId, entry, wrap),
      separatorBefore: groupIndex > 0 && index === 0,
    })),
  )
  return items.length > 0 ? items : undefined
}

/**
 * A row's menu as a TOP-LEVEL menu (the servers hub's right-click): a section per ruled
 * block, which the primitive separates, since a top-level list draws no rules inside a
 * section.
 */
export function rowMenuSections<T>(
  volumeId: string,
  menu: RowMenu,
  wrap: (entry: RowMenuEntry) => T,
): MenuSection<T>[] {
  return ruledBlocks(menu).map((group, index) => ({
    id: `group-${String(index)}`,
    items: group.map((entry) => entryItem(volumeId, entry, wrap)),
  }))
}

/** What each fix runs. A `Record`, so a new `RowFixKind` won't compile until it's handled. */
const fixRunners: Record<RowFixKind, (volume: VolumeInfo) => Promise<unknown>> = {
  // The ONE "Connect directly" flow (`../network/DETAILS.md` § "Connect directly"), the one
  // the chip's yellow dot and the fallback notice run, with its sign-in sheet and toasts.
  'connect-directly': (volume) => connectDirectly({ volumeId: volume.id, shareName: volume.name }),
}

/** Runs a row's fix, whichever surface it was picked on. */
export async function runRowFix(payload: { volume: VolumeInfo; fix: RowFixKind }): Promise<void> {
  await fixRunners[payload.fix](payload.volume)
}

/**
 * Runs a volume or server row's action. Eject and Disconnect go through `runDetach`, the
 * path the row's inline button takes, which refuses a busy volume whatever the menu showed;
 * the rest go to the servers family (`runServerRowAction`), shared with the palette.
 *
 * `onOpen` is the surface's own navigation: the switcher moves ITS pane, the hub moves its.
 */
export async function runVolumeRowAction(payload: {
  volume: VolumeInfo
  action: VolumeContextActionKind
  onOpen?: (volumeId: string) => void
}): Promise<void> {
  const { volume, action: kind, onOpen } = payload
  if (kind === 'eject') {
    await runDetach(volume, 'eject')
    return
  }
  if (kind === 'disconnect') {
    await runDetach(volume, 'disconnect-place')
    return
  }
  await runServerRowAction({ action: kind, volumeId: volume.id, volumeName: volume.name, onOpen })
}
