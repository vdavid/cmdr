/**
 * Opens the "Tell me about this" card for the entry under the cursor, with what its rows and footer actions do. Every
 * hook here runs AFTER the card closed (`goToFromCard` / `actFromCard`), so focus lands where the action went: the
 * pane, the delete dialog, Finder, the browser.
 *
 * Each action goes through the path the pane already has for it, so the card asks and refuses exactly what the pane
 * would: Open is Enter's own activation chain, Move to trash is the delete dialog (with its confirmation, and the
 * operation log recording it), and a revealed place is a `navigate()`.
 */
import type { AboutAction, Location } from '#lib/ipc/bindings.js'
import { openAboutThis } from '#lib/about-this/about-this-state.svelte.js'
import type { AboutGoTarget, AboutSubject } from '#lib/about-this/about-card.js'
import type { AboutTrigger } from '#lib/about-this/about-analytics.js'
import { resolveLocationOrToast } from '#lib/file-explorer/navigation/navigate-and-select.js'
import { capabilitiesFor } from '#lib/file-explorer/pane/volume-capabilities.js'
import { getFocusedPaneVolumeId } from '#lib/file-explorer/pane/focused-pane-reads.js'
import { lastName, parentOrRoot } from '#lib/path/grammar.js'
import { getAppLogger } from '#lib/logging/logger.js'
import { openExternalUrl, showInFinder } from '#lib/tauri-commands/index.js'
import { tString } from '#lib/intl/messages.svelte.js'
import type { ExplorerAPI } from '../explorer-api'
import { detached } from './detached'

const log = getAppLogger('aboutThis')

/** The pane's row the card is about. */
export interface AboutEntry {
  path: string
  filename: string
  iconId: string
  isDirectory: boolean
}

/** Opens the card about `entry`, the focused pane's cursor row, from the road `trigger` (what its usage event reports). */
export function openAboutThisFor(explorer: ExplorerAPI | undefined, entry: AboutEntry, trigger: AboutTrigger): void {
  const volumeId = getFocusedPaneVolumeId()
  const subject: AboutSubject = {
    path: entry.path,
    volumeId,
    name: entry.filename,
    iconId: entry.iconId,
    isDirectory: entry.isDirectory,
  }
  openAboutThis(
    subject,
    {
      goTo: (target) => {
        goTo(explorer, volumeId, target)
      },
      act: (action, about) => {
        detached(act(explorer, action, about))
      },
    },
    { trigger, volumeKind: capabilitiesFor(volumeId).kind },
  )
}

/**
 * Takes the focused pane to a place the card names, on the item's own volume: a folder opens, a file shows in its
 * folder with the cursor on it. A search result's row is a real file on whatever drive holds it, and an app the card
 * reveals is on the boot disk, so those places resolve first.
 */
function goTo(
  explorer: ExplorerAPI | undefined,
  volumeId: string,
  target: AboutGoTarget,
  where: 'sameVolume' | 'resolve' = 'sameVolume',
): void {
  if (!explorer) return
  const path = target.isFolder ? target.path : parentOrRoot(target.path)
  const go = (to: Location): void => {
    explorer.navigate({
      pane: explorer.getFocusedPane(),
      to: { goTo: to },
      source: 'user',
      ...(target.isFolder ? {} : { selectName: lastName(target.path) }),
    })
  }
  if (capabilitiesFor(volumeId).kind !== 'search-results' && where === 'sameVolume') {
    go({ volumeId, path })
    return
  }
  detached(
    resolveLocationOrToast(path).then((location) => {
      if (location) go(location)
    }),
  )
}

/** Whether the focused pane's cursor is still on the card's item, which the card's open and trash act through. */
async function cursorStillOn(explorer: ExplorerAPI, subject: AboutSubject): Promise<boolean> {
  const under = await explorer.getFileAndPathUnderCursor()
  if (under?.path === subject.path) return true
  log.warn('The cursor left the item the card was about; its action does nothing')
  return false
}

async function act(explorer: ExplorerAPI | undefined, action: AboutAction, subject: AboutSubject): Promise<void> {
  switch (action.type) {
    case 'openWithDefaultApp':
      // Enter's own chain, so a just-downloaded app gets whatever the pane would ask or refuse.
      if (explorer && (await cursorStillOn(explorer, subject))) await explorer.openItemUnderCursor()
      return
    case 'showInFileManager':
      await showInFinder(subject.path)
      return
    case 'openDownloadPage':
      // Rust parsed it, kept it to `http(s)`, and wrote it back out; the button showed its host.
      await openExternalUrl(action.url)
      return
    case 'revealOwner':
      goTo(explorer, subject.volumeId ?? '', { path: action.path, isFolder: false }, 'resolve')
      return
    case 'goToAppFolder':
      goTo(explorer, subject.volumeId ?? '', { path: action.path, isFolder: true }, 'resolve')
      return
    case 'continueInAskCmdr': {
      // A new chat with the item attached by path and kind, and a FIXED draft, unsent. ❌ The draft never quotes the
      // card's facts or its AI take: the agent reads the facts itself, as a tool result (`about_item`). Loaded on the
      // press: the rail's trigger slice pulls in the whole explorer state, which the opener doesn't otherwise need.
      const { openNewChatWith } = await import('#lib/ask-cmdr/ask-cmdr-trigger.svelte.js')
      openNewChatWith(
        [{ path: subject.path, kind: subject.isDirectory ? 'folder' : 'file' }],
        tString('aboutThis.continueInAskCmdr.draft'),
      )
      return
    }
    case 'moveToTrash':
    case 'delete':
      // The pane's own delete flow, for the cursor row alone and only while it's this item: it asks first, and the
      // operation log records what it does. ❌ The card never deletes by itself.
      await explorer?.openDeleteDialog({
        permanent: action.type === 'delete',
        initiator: 'user',
        cursorPath: subject.path,
      })
      return
  }
}
