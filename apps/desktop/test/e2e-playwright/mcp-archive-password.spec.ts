/**
 * E2E for answering the encrypted-archive password prompt over MCP.
 *
 * This is the state that used to be unreachable from automation. The prompt was
 * IPC-only: an agent that hit an encrypted archive saw a bare
 * `- type: archive-password` in `cmdr://state` — no archive named, no mode, no
 * way to answer — and its only exit was cancel. So neither half of the flow
 * (browse, transfer) could be exercised by anything but a hand, which is exactly
 * the blind spot a conflict-resolution wedge lived in for months.
 *
 * The whole loop is MCP: hit the prompt → read WHICH archive is asking and in
 * which mode → `unlock_archive` → watch a wrong password come back as
 * `wrongAttempt: true` → unlock for real.
 *
 * And the boundary, which is the other half of this spec: **an agent may supply
 * the password, but it may never be the thing that starts the write.** A person
 * typing the password gets the copy re-dispatched for them; `unlock_archive`
 * deliberately does not, so extraction is gated exactly like any other copy. The
 * two `expect`s that prove it are marked as such below.
 *
 * Requires `--features playwright-e2e`.
 */

import fs from 'fs'
import path from 'path'
import { fileURLToPath } from 'url'
import { test, expect } from './fixtures.js'
import { restoreFixtureTree } from '../e2e-shared/fixture-manifest.js'
import { recreateFixtures } from '../e2e-shared/fixtures.js'
import { ensureAppReady, expectAndDismissToast, getFixtureRoot } from './helpers.js'
import { ensureMcpClient, mcpCall, mcpCallRaw, mcpReadResource, mcpNavToPath } from '../e2e-shared/mcp-client.js'
import type { TauriPage } from '@srsholmes/tauri-playwright'

/** A ZipCrypto zip: its central directory lists fine, so browsing in works and
 *  only reading an entry needs the password. That's the TRANSFER prompt. */
const ENCRYPTED_ZIP = 'encrypted.zip'
/** A header-encrypted 7z (`-mhe=on`): even the metadata is encrypted, so the
 *  LISTING needs the password. That's the BROWSE prompt. */
const LOCKED_7Z = 'locked.7z'
/** Both fixtures hold `hidden.txt` + `notes.txt` under this password. */
const PASSWORD = 'hunter2'
const WRONG_PASSWORD = 'not-the-password'
const INNER_FILE = 'hidden.txt'
const INNER_CONTENT = 'top secret payload\n'

const archiveFixturesDir = path.join(
  path.dirname(fileURLToPath(import.meta.url)),
  '..',
  'e2e-shared',
  'archive-fixtures',
)

test.setTimeout(90_000)

/** The `archive-password` block `cmdr://state` renders under `dialogs:`. */
interface PasswordPrompt {
  archive: string
  archivePath: string
  mode: string
  wrongAttempt: boolean
}

test.beforeEach(async ({ tauriPage }) => {
  const fixtureRoot = getFixtureRoot()
  recreateFixtures(fixtureRoot)
  // Copied per-spec rather than added to the shared archive fixtures: every
  // other spec's `left/` would grow two entries it never asked for.
  for (const name of [ENCRYPTED_ZIP, LOCKED_7Z]) {
    fs.copyFileSync(path.join(archiveFixturesDir, name), path.join(fixtureRoot, 'left', name))
  }
  await ensureAppReady(tauriPage)
  await ensureMcpClient(tauriPage)
})

test.afterEach(async ({ tauriPage }) => {
  // A prompt left up would block every later spec's file operations, and the
  // stored password would outlive the test. Cancelling does both.
  await mcpCallRaw('dialog', { action: 'close', type: 'archive-password' })
  await tauriPage.evaluate(`(async function() {
    try {
      var ops = await window.__TAURI_INTERNALS__.invoke('list_operations');
      var ids = ops.map(function(o) { return o.operationId; });
      if (ids.length) await window.__TAURI_INTERNALS__.invoke('cancel_operations', { operationIds: ids });
    } catch (e) {}
    try { await window.__TAURI_INTERNALS__.invoke('dismiss_all_failed_operations'); } catch (e) {}
  })()`)
  restoreFixtureTree(getFixtureRoot())
})

test.describe('MCP archive password', () => {
  test('a browse prompt names its archive, rejects a wrong password out loud, and unlocks', async () => {
    const fixtureRoot = getFixtureRoot()

    // 1. Stepping into a header-encrypted archive can't even read its listing,
    //    so the prompt comes up before anything is shown. `nav_to_path` (not
    //    Enter): the archive Enter policy defaults to Ask, and answering that
    //    popup isn't what's under test here.
    await mcpNavToPath('left', path.join(fixtureRoot, 'left'))
    await mcpCallRaw('nav_to_path', { pane: 'left', path: path.join(fixtureRoot, 'left', LOCKED_7Z) })

    // 2. The prompt is READABLE. Without this an agent sees a bare
    //    `- type: archive-password` and can't tell what is being asked.
    const first = await waitForPasswordPrompt()
    expect(first.archive).toBe(LOCKED_7Z)
    expect(first.mode).toBe('browse')
    expect(first.wrongAttempt).toBe(false)

    // 3. A wrong password is not silently swallowed: the prompt comes back
    //    saying so, which is the only thing that makes the loop closeable.
    const rejected = await unlockArchive(first.archivePath, WRONG_PASSWORD)
    expect(rejected.outcome).toBe('retrying_listing')
    const retry = await waitForPasswordPrompt({ wrongAttempt: true })
    expect(retry.archive).toBe(LOCKED_7Z)

    // 4. The real password gets in. Browsing is a READ, so unlocking completes
    //    it: no separate approval, and the pane lists the archive.
    const unlocked = await unlockArchive(retry.archivePath, PASSWORD)
    expect(unlocked.outcome).toBe('retrying_listing')
    await expect
      .poll(async () => (await mcpReadResource('cmdr://state?include=panes')).includes(INNER_FILE), { timeout: 15_000 })
      .toBeTruthy()
  })

  test('a transfer prompt is answerable, and unlocking still cannot start the write', async ({ tauriPage }) => {
    const main = tauriPage as TauriPage
    const fixtureRoot = getFixtureRoot()
    const destination = path.join(fixtureRoot, 'right', INNER_FILE)

    // 1. A ZipCrypto zip lists without a password, so we browse in and copy an
    //    entry out. The READ of that entry is what needs the password.
    await mcpNavToPath('left', path.join(fixtureRoot, 'left', ENCRYPTED_ZIP))
    await mcpNavToPath('right', path.join(fixtureRoot, 'right'))
    await mcpCall('select', { pane: 'left', names: [INNER_FILE] })
    await mcpCall('copy', {})
    await mcpCall('dialog', { action: 'confirm', type: 'transfer-confirmation' })

    const prompt = await waitForPasswordPrompt()
    expect(prompt.archive).toBe(ENCRYPTED_ZIP)
    expect(prompt.mode).toBe('transfer')

    // 2. While the prompt is up, starting a file operation is refused, naming
    //    the blocker in typed data. Same gate every other dialog gets.
    const blocked = await mcpCallRaw('copy', {})
    expect(blocked.error?.data?.blockingDialog).toBe('archive-password')

    // 3. Answering with the WRONG archive is refused, not applied to whatever
    //    happens to be asking. The prompt must be NAMED, like a conflictId is.
    const misaimed = await mcpCallRaw('unlock_archive', {
      archivePath: path.join(fixtureRoot, 'left', 'sample.zip'),
      password: PASSWORD,
    })
    expect(misaimed.error?.data?.outcome).toBe('different_archive')

    // 4. The password lands. The copy that hit the prompt is already settled
    //    (a password failure settles it), so nothing is unparked.
    const stored = await unlockArchive(prompt.archivePath, PASSWORD)
    expect(stored.outcome).toBe('password_stored')

    // 5. ⭐ THE BOUNDARY. Supplying the password did NOT start a write. A
    //    person's submit re-dispatches the copy for them; an agent's must not,
    //    or extraction would be the one write with no gate in front of it.
    //    Held for a few seconds, because a re-dispatch would be asynchronous:
    //    a single read right after the unlock could miss one that is coming.
    for (let i = 0; i < 6; i++) {
      const operations = await mcpReadResource('cmdr://state?include=operations')
      expect(operations).toContain('operations: []')
      expect(fs.existsSync(destination)).toBe(false)
      await new Promise((resolve) => setTimeout(resolve, 500))
    }

    // 6. ⭐ AND the write is still reachable the ordinary way: the agent starts
    //    the copy again, through the same confirmation every copy goes through,
    //    and the stored password lets it read the entry this time.
    await mcpCall('select', { pane: 'left', names: [INNER_FILE] })
    await mcpCall('copy', {})
    await mcpCall('dialog', { action: 'confirm', type: 'transfer-confirmation' })
    await expect.poll(() => fs.existsSync(destination), { timeout: 20_000 }).toBeTruthy()
    expect(fs.readFileSync(destination, 'utf8')).toBe(INNER_CONTENT)

    await expectAndDismissToast(main, 'Copied')
  })
})

/** Polls `cmdr://state` until the archive-password prompt is up, optionally
 *  waiting for the re-prompt a rejected password raises. */
async function waitForPasswordPrompt(options?: { wrongAttempt: boolean }): Promise<PasswordPrompt> {
  const seen: PasswordPrompt[] = []
  await expect
    .poll(
      async () => {
        const prompt = parsePasswordPrompt(await mcpReadResource('cmdr://state?include=dialogs'))
        if (!prompt) return false
        if (options && prompt.wrongAttempt !== options.wrongAttempt) return false
        seen.push(prompt)
        return true
      },
      { timeout: 20_000 },
    )
    .toBeTruthy()
  const captured = seen.at(-1)
  if (!captured) throw new Error('no archive-password prompt was captured')
  return captured
}

/** Reads the `- type: archive-password` entry out of the `dialogs:` YAML. */
function parsePasswordPrompt(stateYaml: string): PasswordPrompt | null {
  const lines = stateYaml.split('\n')
  const blockAt = lines.findIndex((l) => l.includes('- type: archive-password'))
  if (blockAt === -1) return null
  const archive = findWithin(lines, blockAt, /^\s+archive: "(.*)"/)
  const archivePath = findWithin(lines, blockAt, /^\s+archivePath: "(.*)"/)
  const mode = findWithin(lines, blockAt, /^\s+mode: (\S+)/)
  const wrongAttempt = findWithin(lines, blockAt, /^\s+wrongAttempt: (\S+)/)
  if (archive === undefined || archivePath === undefined || mode === undefined || wrongAttempt === undefined) {
    return null
  }
  return { archive, archivePath, mode, wrongAttempt: wrongAttempt === 'true' }
}

/** First capture of `pattern` in the handful of lines after `from`. */
function findWithin(lines: string[], from: number, pattern: RegExp): string | undefined {
  for (let i = from + 1; i < Math.min(from + 8, lines.length); i++) {
    const match = pattern.exec(lines[i])
    if (match) return match[1]
  }
  return undefined
}

interface UnlockResult {
  outcome: string
  archive: string
}

/** Answers the prompt and parses the tool's typed result. */
async function unlockArchive(archivePath: string, password: string): Promise<UnlockResult> {
  return JSON.parse(await mcpCall('unlock_archive', { archivePath, password })) as UnlockResult
}
