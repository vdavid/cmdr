/**
 * What a pasted `sftp://`, `smb://`, `adb://`, or `https://` means to Go to path,
 * decided BEFORE the local resolver is asked.
 *
 * ❗ **The Rust resolver is local-only by design** (`commands/go_to_path.rs` walks
 * `std::fs::metadata` over a tilde-expanded, base-dir-joined path). A scheme
 * input there joins onto the pane's directory and answers `invalid`, and teaching
 * it schemes would mean a resolver that consults the saved-server stores it has
 * no business reading. So the classification happens here, and the three sites
 * that resolve (the jump, the debounced preview, and the clipboard prefill) all
 * read the SAME function. A branch inside the jump alone would leave the preview
 * saying "this path doesn't exist" about an address that is about to work.
 *
 * ❗ **Reading and ACTING are separate on purpose.** The preview runs on a
 * debounce while someone types, and a read that opened a modal there would put a
 * sheet on screen mid-keystroke. [`readSchemeInput`] classifies and is safe
 * everywhere; only the jump calls [`actOnSchemeInput`].
 */

import { parseServerAddress } from '$lib/servers/address-parser'
import { isServerPath, parseServerPath } from '$lib/servers/server-path-utils'
import { openAddServerSheet } from '$lib/servers/open-sign-in'
import { listSavedServers } from '$lib/tauri-commands'
import { getAppLogger } from '$lib/logging/logger'
import { tString } from '$lib/intl/messages.svelte'
import type { GoToPathResolution } from '$lib/ipc/bindings'

const log = getAppLogger('go-to-path')

/**
 * What Go to path can answer, once a scheme input is in play.
 *
 * `GoToPathResolution` is Rust-generated, so the hand-off is a member the
 * frontend adds. The dialog closes on anything that isn't `invalid`, and a
 * hand-off has done its job, so it closes too.
 */
export type GoToPathOutcome = GoToPathResolution | { kind: 'handed_off' }

/** What a scheme input turned out to mean. */
export type SchemeIntent =
  /** A place the app can navigate to right now: a saved server, a phone, a camera. */
  | { kind: 'place'; path: string; label: string }
  /** An address for a server nothing has saved. The sheet opens on it. */
  | { kind: 'add'; address: string }

/** Devices already carry a scheme and already resolve; a path on one just navigates. */
const DEVICE_SCHEMES = ['adb://', 'mtp://']

/**
 * What `input` means, or `null` when it is an ordinary path the local resolver
 * owns.
 *
 * ❗ Safe to call from a debounced preview: it reads the saved list (cached
 * state) and opens nothing.
 */
export async function readSchemeInput(input: string): Promise<SchemeIntent | null> {
  const trimmed = input.trim()
  if (trimmed === '') return null

  if (DEVICE_SCHEMES.some((scheme) => trimmed.startsWith(scheme))) {
    // A device path is already resolvable, so it navigates rather than
    // opening anything. `DETAILS.md` § "The scheme intercept".
    return { kind: 'place', path: trimmed, label: deviceLabel(trimmed) }
  }

  if (isServerPath(trimmed)) {
    const place = await savedPlaceFor(trimmed)
    // ❗ A server path nothing saved is an ADDRESS, not a dead end: someone
    // pasted a link to a server they haven't added yet, and the sheet is what
    // adds it.
    return place ? { kind: 'place', path: trimmed, label: place } : { kind: 'add', address: trimmed }
  }

  // Every other shape the address field understands: `ssh://`, `smb://`,
  // `davs://`, a web address. ❗ Only when it carries a SCHEME — a bare hostname
  // is a legal relative path, and Go to path has always resolved those.
  if (!trimmed.includes('://')) return null
  return parseServerAddress(trimmed).kind === 'parsed' ? { kind: 'add', address: trimmed } : null
}

/** The line the dialog shows under the box for a scheme input. */
export function previewSchemeInput(intent: SchemeIntent): string {
  return intent.kind === 'place'
    ? tString('goToPath.dialog.opensServer', { name: intent.label })
    : tString('goToPath.dialog.addsServer')
}

/**
 * Acts on a scheme input the jump is committing to: hands an address to the
 * sheet, or reports the path so the caller navigates to it.
 *
 * ❗ Only the jump calls this. Opening a modal from the debounced preview would
 * put a sheet on screen while someone is still typing the address for it.
 */
export async function actOnSchemeInput(
  intent: SchemeIntent,
  deps: {
    /**
     * Where an SMB address lands. ❗ Go to path is a NAVIGATION command, so it
     * has to put the person somewhere: an SMB connect is a share MOUNT rather
     * than a session, so there is no volume to go to and the host's places list
     * is the destination. That is where ⌘K's own hand-off goes
     * (`command-handlers/servers-handlers.ts`), and two entry points into one
     * sheet ending differently for one input is what this exists to prevent.
     */
    onSmbHandOff: () => void
  },
): Promise<GoToPathOutcome> {
  if (intent.kind === 'place') return { kind: 'directory', path: intent.path }
  await openAddServerSheet({ prefill: intent.address, onSmbHandOff: deps.onSmbHandOff })
  return { kind: 'handed_off' }
}

/** The saved place a server path belongs to, by its own name. */
async function savedPlaceFor(path: string): Promise<string | null> {
  const parsed = parseServerPath(path)
  if (!parsed) return null
  try {
    const servers = await listSavedServers()
    for (const server of servers) {
      for (const place of server.places) {
        // Whole-prefix containment is `server-path-utils`'s own rule; here the
        // app root is a prefix of any path inside the place.
        if (path === place.appRoot || path.startsWith(`${place.appRoot}/`)) return place.name
      }
    }
  } catch (e) {
    // A store that doesn't answer means the address opens the sheet instead of
    // navigating: one extra step, ❌ never a wrong destination.
    log.warn('Reading the saved servers for {path} broke down: {error}', { path, error: String(e) })
  }
  return null
}

/** What to call a device in the preview: the serial or id the path names. */
function deviceLabel(path: string): string {
  const rest = path.slice(path.indexOf('://') + 3)
  const slash = rest.indexOf('/')
  return slash === -1 ? rest : rest.slice(0, slash)
}
