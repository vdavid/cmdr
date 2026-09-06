/**
 * The servers family: showing the hub, and the three actions that mirror a server
 * row's context menu for people who reach for the keyboard.
 *
 * ❗ **All three action arms go through `runServerRowAction`**, the same function
 * the native menu's answer lands in. One place names what "disconnect" costs, so
 * a menu item and a palette command can't drift on the confirmation or the toast.
 *
 * Which server they act on is `serverCommandTarget`'s call: the hub's cursor row
 * wherever there is one, the focused pane's own volume otherwise. A command that
 * finds neither says nothing — the palette lists every command whatever the pane
 * is on, and a toast for "you weren't pointing at a server" is noise.
 */
import { runServerRowAction } from '$lib/file-explorer/navigation/server-row-actions'
import { openAddServerSheet } from '$lib/servers/open-sign-in'
import { serverCommandTarget, type ServerCommandTarget } from '$lib/servers/server-command-target'
import { getFocusedPaneVolumeId } from '$lib/file-explorer/pane/focused-pane-reads'
import { getVolumes } from '$lib/stores/volume-store.svelte'
import { listSavedServers } from '$lib/tauri-commands'
import { getAppLogger } from '$lib/logging/logger'
import type { ExplorerAPI } from '../explorer-api'
import type { CommandHandlerRecord } from './types'

const log = getAppLogger('servers')

/** The server the user is pointing at, or `null`. */
function target(explorerRef: ExplorerAPI | undefined): ServerCommandTarget | null {
  const volumeId = getFocusedPaneVolumeId()
  return serverCommandTarget({
    cursorRow: explorerRef?.getFocusedPaneServerRow() ?? null,
    paneVolume: getVolumes().find((volume) => volume.id === volumeId) ?? null,
  })
}

/**
 * Whether the target is pinned right now.
 *
 * A target resolved from the pane's volume can't say (a `VolumeInfo` carries no
 * pin), so the saved list answers. A store that doesn't answer reads as unpinned,
 * which makes the command a pin rather than a silent no-op.
 */
async function isPinned(server: ServerCommandTarget): Promise<boolean> {
  if (server.pinned !== null) return server.pinned
  try {
    const saved = await listSavedServers()
    return saved.some((entry) => entry.places.some((place) => place.volumeId === server.volumeId && place.pinned))
  } catch (e) {
    log.warn('Reading the pin for {volumeId} broke down: {error}', { volumeId: server.volumeId, error: String(e) })
    return false
  }
}

export const serversHandlers = {
  'servers.show': ({ explorerRef }) => {
    explorerRef?.showServersInFocusedPane()
  },

  'servers.togglePin': async ({ explorerRef }) => {
    const server = target(explorerRef)
    if (!server) return
    const pinned = await isPinned(server)
    await runServerRowAction({
      action: pinned ? 'unpin' : 'pin',
      volumeId: server.volumeId,
      volumeName: server.name,
    })
  },

  'servers.disconnect': async ({ explorerRef }) => {
    const server = target(explorerRef)
    if (!server) return
    await runServerRowAction({ action: 'disconnect', volumeId: server.volumeId, volumeName: server.name })
  },

  'servers.forgetSecret': async ({ explorerRef }) => {
    const server = target(explorerRef)
    if (!server) return
    await runServerRowAction({ action: 'forget-secret', volumeId: server.volumeId, volumeName: server.name })
  },

  // ❗ The two sheet commands OPEN and return, ❌ never await the sheet: a
  // dispatch that doesn't settle until the user is done typing holds the command
  // pipeline open for as long as they take, and nothing downstream reads the
  // answer. `servers.show` behaves the same way.
  'servers.edit': ({ explorerRef }) => {
    const server = target(explorerRef)
    if (!server) return
    void runServerRowAction({ action: 'edit', volumeId: server.volumeId, volumeName: server.name })
  },

  'servers.connect': ({ explorerRef }) => {
    // ⌘K, Finder's binding for the same thing. ❗ It opens the sheet directly
    // rather than through `serverCommandTarget`: adding a server is about no
    // server in particular, so what the pane is pointing at is irrelevant.
    void openAddServerSheet({
      // An SMB address lands in the hub rather than on a volume: its connect is
      // a share MOUNT, and the host is now a saved manual server the hub lists,
      // one Enter from its shares.
      onSmbHandOff: () => {
        explorerRef?.showServersInFocusedPane()
      },
    })
  },
} satisfies Partial<CommandHandlerRecord>
