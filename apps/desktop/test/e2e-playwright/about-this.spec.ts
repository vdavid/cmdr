/**
 * E2E for "Tell me about this": the card opens from its shortcut, the command palette, and the
 * File menu / context menu (both native rows dispatch the same command), streams its rows in, and
 * closes on Escape with focus back on the pane.
 *
 * The fixture is written here, under the fixture root but outside `left/` and `right/` (the leak
 * guard watches those): a small PNG carrying the OS's own download record, and a folder. The record
 * is what a browser writes, synthesized: macOS gets `kMDItemWhereFroms` (a binary plist of the file
 * and page addresses) plus `com.apple.quarantine`; Linux gets `user.xdg.origin.url`. Every host is
 * made up (❌ no real download tokens).
 *
 * The native context row itself is AppKit/GTK, which Playwright can't click; it emits the same
 * `execute-command` the File menu row does (`cmdr-menu`'s `menu_id_to_command`, pinned in Rust), so
 * the menu path here covers both.
 */

import fs from 'fs'
import path from 'path'
import { execFileSync } from 'child_process'
import { waitBudget } from './wait-budget.js'
import { test, expect } from './fixtures.js'
import {
  dismissOverlay,
  dispatchMenuCommand,
  ensureAppReady,
  executeViaCommandPalette,
  getFixtureRoot,
  moveCursorToFile,
  pressKey,
} from './helpers.js'
import { defaultKey } from './helpers/registry-keys.js'
import type { TauriPage, BrowserPageAdapter } from '@srsholmes/tauri-playwright'

type PageLike = TauriPage | BrowserPageAdapter

const CARD = '[data-dialog-id="tell-me-about-this"]'
const DIR = 'about-this'
const FILE = 'chart.png'
const FOLDER = 'reports'
const HOST = 'downloads.about-this.example'
const FILE_URL = `https://${HOST}/files/chart.png`
const PAGE_URL = 'https://about-this.example/charts'

/** A 1×1 PNG: enough bytes for the type row to recognize an image. */
const PNG = Buffer.from(
  '89504e470d0a1a0a0000000d4948445200000001000000010806000000' +
    '1f15c4890000000d49444154789c6360000002000001e221bc330000000049454e44ae426082',
  'hex',
)

/** A binary plist (`bplist00`) holding one array of short ASCII strings, as Chrome writes `kMDItemWhereFroms`. */
function bplistOfStrings(strings: string[]): Buffer {
  const objects: Buffer[] = []
  // Object 0: the array, referencing objects 1..n with one-byte refs.
  objects.push(Buffer.from([0xa0 | strings.length, ...strings.map((_, i) => i + 1)]))
  for (const s of strings) {
    const bytes = Buffer.from(s, 'ascii')
    // Lengths past 14 take the extended form: 0x5F, then an int object (0x10 = one byte).
    const head = bytes.length < 15 ? Buffer.from([0x50 | bytes.length]) : Buffer.from([0x5f, 0x10, bytes.length])
    objects.push(Buffer.concat([head, bytes]))
  }
  const header = Buffer.from('bplist00', 'ascii')
  const offsets: number[] = []
  let at = header.length
  for (const o of objects) {
    offsets.push(at)
    at += o.length
  }
  const offsetTableStart = at
  const offsetSize = offsetTableStart > 0xff ? 2 : 1
  const table = Buffer.alloc(offsets.length * offsetSize)
  offsets.forEach((o, i) => {
    if (offsetSize === 1) table.writeUInt8(o, i)
    else table.writeUInt16BE(o, i * 2)
  })
  const trailer = Buffer.alloc(32)
  trailer.writeUInt8(offsetSize, 6)
  trailer.writeUInt8(1, 7) // object ref size
  trailer.writeBigUInt64BE(BigInt(objects.length), 8)
  trailer.writeBigUInt64BE(0n, 16) // top object
  trailer.writeBigUInt64BE(BigInt(offsetTableStart), 24)
  return Buffer.concat([header, ...objects, table, trailer])
}

/**
 * Sets a `user.*` xattr on Linux. The container has no `attr` package, so this goes through Perl's
 * `syscall` (Perl is part of `build-essential`): `setxattr` is 188 on x86_64 and 5 on aarch64.
 */
function setLinuxUserXattr(file: string, name: string, value: string): void {
  const number = process.arch === 'arm64' ? 5 : 188
  execFileSync('perl', [
    '-e',
    `my ($p, $n, $v) = @ARGV; my $r = syscall(${String(number)}, $p, $n, $v, length($v), 0); exit($r == 0 ? 0 : 1)`,
    '--',
    file,
    name,
    value,
  ])
}

/** Writes the OS's download record onto `file`, the way a browser leaves it. */
function writeDownloadRecord(file: string): void {
  if (process.platform === 'darwin') {
    const whereFroms = bplistOfStrings([FILE_URL, PAGE_URL]).toString('hex')
    execFileSync('xattr', ['-wx', 'com.apple.metadata:kMDItemWhereFroms', whereFroms, file])
    const seconds = Math.floor(Date.parse('2026-03-02T12:00:00Z') / 1000).toString(16)
    execFileSync('xattr', [
      '-w',
      'com.apple.quarantine',
      `0081;${seconds};Chrome;0E0B8C55-0000-4000-8000-ABOUTTHIS000`,
      file,
    ])
  } else {
    setLinuxUserXattr(file, 'user.xdg.origin.url', FILE_URL)
  }
}

function fixtureDir(): string {
  return path.join(getFixtureRoot(), DIR)
}

/** A fresh directory (never hard-linked from the fixture cache, so its xattrs touch nothing shared). */
function buildFixture(): void {
  fs.rmSync(fixtureDir(), { recursive: true, force: true })
  fs.mkdirSync(path.join(fixtureDir(), FOLDER), { recursive: true })
  fs.writeFileSync(path.join(fixtureDir(), FOLDER, 'q1.txt'), 'first quarter\n')
  fs.writeFileSync(path.join(fixtureDir(), FILE), PNG)
  writeDownloadRecord(path.join(fixtureDir(), FILE))
}

async function openFixtureDir(tauriPage: PageLike): Promise<void> {
  await ensureAppReady(tauriPage)
  await tauriPage.evaluate(`window.__TAURI_INTERNALS__.invoke('plugin:event|emit', {
      event: 'mcp-nav-to-path',
      payload: { pane: 'left', path: ${JSON.stringify(fixtureDir())} }
  })`)
  expect(await moveCursorToFile(tauriPage, FILE)).toBe(true)
}

/** Waits for the card and for its facts to settle. */
async function waitForSettledCard(tauriPage: PageLike): Promise<void> {
  await tauriPage.waitForSelector(CARD, waitBudget(5000))
  await expect
    .poll(async () => tauriPage.evaluate<boolean>(`!!document.querySelector('${CARD} [data-settled="true"]')`), {
      timeout: waitBudget(5000),
    })
    .toBeTruthy()
}

async function rowText(tauriPage: PageLike, kind: string): Promise<string> {
  return tauriPage.evaluate<string>(
    `(document.querySelector('${CARD} [data-row-kind="${kind}"]')?.textContent ?? '').replace(/\\s+/g, ' ')`,
  )
}

test.describe('Tell me about this', () => {
  test.beforeEach(() => {
    buildFixture()
  })
  test.afterEach(() => {
    fs.rmSync(fixtureDir(), { recursive: true, force: true })
  })

  test('opens by shortcut on a downloaded file with its download host and type, and Escape gives focus back', async ({
    tauriPage,
  }) => {
    await openFixtureDir(tauriPage)
    await pressKey(tauriPage, defaultKey('file.tellMeAbout'))
    await waitForSettledCard(tauriPage)

    expect(await rowText(tauriPage, 'download')).toContain(HOST)
    // The host is the card's; the full address only rides behind the copy button.
    expect(await rowText(tauriPage, 'download')).not.toContain('/files/chart.png')
    expect(await rowText(tauriPage, 'realType')).toContain('Image')

    await dismissOverlay(tauriPage)
    await expect
      .poll(
        async () =>
          tauriPage.evaluate<boolean>(
            `!!document.activeElement && !!document.activeElement.closest('.file-pane.is-focused')`,
          ),
        { timeout: waitBudget(3000) },
      )
      .toBeTruthy()
  })

  test('opens from the command palette', async ({ tauriPage }) => {
    await openFixtureDir(tauriPage)
    await executeViaCommandPalette(tauriPage, 'Tell me about this')
    await waitForSettledCard(tauriPage)
    expect(await rowText(tauriPage, 'download')).toContain(HOST)
    await dismissOverlay(tauriPage)
  })

  test('opens from the File menu and context menu row, both of which dispatch the command', async ({ tauriPage }) => {
    await openFixtureDir(tauriPage)
    await dispatchMenuCommand(tauriPage, 'file.tellMeAbout')
    await waitForSettledCard(tauriPage)
    expect(await rowText(tauriPage, 'realType')).toContain('Image')
    await dismissOverlay(tauriPage)
  })

  test('a folder’s card leads with what’s inside and has its size row', async ({ tauriPage }) => {
    await openFixtureDir(tauriPage)
    expect(await moveCursorToFile(tauriPage, FOLDER)).toBe(true)
    await pressKey(tauriPage, defaultKey('file.tellMeAbout'))
    await waitForSettledCard(tauriPage)
    expect(
      await tauriPage.evaluate<string | null>(
        `document.querySelector('${CARD} [data-section]')?.getAttribute('data-section') ?? null`,
      ),
    ).toBe('contents')
    // Indexed or not, each row is there and says something: what's inside and a size, or why the index can't tell
    // yet. The folder holds a file, so it's never called empty.
    const inside = await rowText(tauriPage, 'folderDigest')
    expect(inside).not.toBe('')
    expect(inside).not.toContain('Empty')
    expect(await rowText(tauriPage, 'recursiveSize')).not.toBe('')
    // A folder's own entry size is never its size: the Size row is the index's, not identity's.
    expect(await tauriPage.count(`${CARD} [data-row-kind="size"]`)).toBe(0)
    await dismissOverlay(tauriPage)
  })
})
