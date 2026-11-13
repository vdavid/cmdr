/**
 * The "Tell me about this" QA fixture folder: one item per case the card must read well, built in one command on macOS
 * and Linux. David opens it in the dev app to QA the card in a stable state, and `about-this.spec.ts` builds the same
 * tree under its fixture root.
 *
 * ```sh
 * pnpm about:fixtures            # builds <repo>/_ignored/about-this-qa (git-ignored)
 * pnpm about:fixtures /tmp/qa    # or anywhere else
 * ```
 *
 * then start the dev app with the printed `CMDR_ABOUT_FIXTURE_HOME`, so the card reads the tree's `home/` as the home
 * folder: its fake `Library/Application Support/…` folders then read the way real ones do, ❌ with no write into the
 * real `~/Library` (`apps/desktop/src-tauri/src/test_mode.rs::about_fixture_home`).
 *
 * Every download record is synthesized, the way a browser writes one (❌ no real download tokens, hosts under
 * `.example`), and the photo's location is a public square, there to prove the location never leaves the machine.
 */

import fs from 'fs'
import path from 'path'
import { execFileSync } from 'child_process'
import { fileURLToPath } from 'url'

/** The items the tree holds, by what each exercises, as absolute paths. */
export interface AboutFixtureTree {
  root: string
  /** The folder the card should read as home (`CMDR_ABOUT_FIXTURE_HOME`). */
  home: string
  downloadedDiskImage: string
  executableNamedJpg: string
  bidiName: string
  photoWithLocation: string
  plainDocument: string
  hardLinkA: string
  hardLinkB: string
  emptyFolder: string
  repo: string
  repoClean: string
  repoModified: string
  repoUntracked: string
  nodeModules: string
  /** `Application Support/<bundle id>` of an app every Mac has (TextEdit). */
  appSupportById: string
  /** A file deep inside it: its card offers the app and the app's data folder. */
  insideAppSupport: string
  /** `Application Support/<bundle id>` of an app that isn't installed. */
  leftoverAppSupport: string
  /** `Application Support/Slack`: a folder named after an app, "probably" Slack's when Slack is installed. */
  appSupportByName: string
  /** `Application Support/Google`: a vendor's folder, ambiguous when two of its apps are installed. */
  appSupportVendor: string
  /** `Caches/<bundle id>` of Safari: a cache its app writes again. */
  cache: string
}

/** A host under `.example`, so no record here names a real site. */
const DMG_FILE_URL = 'https://downloads.tessera.example/releases/2.4.1/Tessera-2.4.1.dmg'
const DMG_PAGE_URL = 'https://tessera.example/download'

/**
 * A binary plist (`bplist00`) holding one array of short ASCII strings, as Chrome writes `kMDItemWhereFroms`. Strings
 * up to 255 bytes, at most 14 of them: plenty for a record.
 */
export function bplistOfStrings(strings: string[]): Buffer {
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
 * Sets a `user.*` xattr on Linux. The E2E container has no `attr` package, so this goes through Perl's `syscall` (Perl
 * is part of `build-essential`): `setxattr` is 188 on x86_64 and 5 on aarch64.
 */
export function setLinuxUserXattr(file: string, name: string, value: string): void {
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

/**
 * Writes the OS's download record onto `file` the way a browser leaves it: on macOS `kMDItemWhereFroms` (the file's
 * address, then the page's) and a quarantine naming Chrome and the time; on Linux Chromium's `user.xdg.*` pair.
 */
export function writeDownloadRecord(file: string, record: { fileUrl: string; pageUrl: string; at: string }): void {
  if (process.platform === 'darwin') {
    const whereFroms = bplistOfStrings([record.fileUrl, record.pageUrl]).toString('hex')
    execFileSync('xattr', ['-wx', 'com.apple.metadata:kMDItemWhereFroms', whereFroms, file])
    const seconds = Math.floor(Date.parse(record.at) / 1000).toString(16)
    execFileSync('xattr', [
      '-w',
      'com.apple.quarantine',
      `0081;${seconds};Chrome;0E0B8C55-0000-4000-8000-ABOUTTHIS000`,
      file,
    ])
  } else {
    setLinuxUserXattr(file, 'user.xdg.origin.url', record.fileUrl)
    setLinuxUserXattr(file, 'user.xdg.referrer.url', record.pageUrl)
  }
}

/** A disk image's shape: some bytes, then the 512-byte `koly` trailer the type row recognizes. */
function diskImageBytes(): Buffer {
  const body = Buffer.alloc(2 * 1024 * 1024, 0x5a)
  const trailer = Buffer.alloc(512)
  trailer.write('koly', 0, 'ascii')
  return Buffer.concat([body, trailer])
}

/** A 64-bit Mach-O header (what a program is), padded: the card says it isn't the picture its name claims. */
function programBytes(): Buffer {
  const header = Buffer.from('cffaedfe0c000001000000000200000000000000', 'hex')
  return Buffer.concat([header, Buffer.alloc(48 * 1024, 0x90)])
}

/** A real (empty-entry) ZIP: what the bidi-named "picture" really is. */
function zipBytes(): Buffer {
  // A local header for `notes.txt` (stored, no data), its central directory entry, and the end record.
  const name = Buffer.from('notes.txt', 'ascii')
  const local = Buffer.concat([
    Buffer.from('504b03040a00000000000000000000000000000000000000000009000000', 'hex'),
    name,
  ])
  const central = Buffer.concat([
    Buffer.from('504b01021e030a000000000000000000000000000000000000000000090000000000000000000000a481000000', 'hex'),
    Buffer.from('00', 'hex'),
    name,
  ])
  const centralStart = local.length
  const end = Buffer.alloc(22)
  end.writeUInt32LE(0x06054b50, 0)
  end.writeUInt16LE(1, 8)
  end.writeUInt16LE(1, 10)
  end.writeUInt32LE(central.length, 12)
  end.writeUInt32LE(centralStart, 16)
  return Buffer.concat([local, central, end])
}

/** The JPEG body after its headers: a 2×2 picture `sips` wrote, from its frame header to the end. */
const JPEG_BODY = Buffer.from(
  'ffc00011080002000203012200021101031101ffc4001f0000010501010101010100000000000000000102030405060708090a0bffc400b51000' +
    '02010303020403050504040000017d01020300041105122131410613516107227114328191a1082342b1c11552d1f02433627282090a1617' +
    '18191a25262728292a3435363738393a434445464748494a535455565758595a636465666768696a737475767778797a838485868788898a' +
    '92939495969798999aa2a3a4a5a6a7a8a9aab2b3b4b5b6b7b8b9bac2c3c4c5c6c7c8c9cad2d3d4d5d6d7d8d9dae1e2e3e4e5e6e7e8e9eaf1' +
    'f2f3f4f5f6f7f8f9faffc4001f0100030101010101010101010000000000000102030405060708090a0bffc400b511000201020404030407' +
    '05040400010277000102031104052131061241510761711322328108144291a1b1c109233352f0156272d10a162434e125f11718191a2627' +
    '28292a35363738393a434445464748494a535455565758595a636465666768696a737475767778797a82838485868788898a929394959697' +
    '98999aa2a3a4a5a6a7a8a9aab2b3b4b5b6b7b8b9bac2c3c4c5c6c7c8c9cad2d3d4d5d6d7d8d9dae2e3e4e5e6e7e8e9eaf2f3f4f5f6f7f8f9' +
    'faffdb0043000f0f0f0f0f0f1a0f0f1a241a1a1a243124242424313e31313131313e4b3e3e3e3e3e3e4b4b4b4b4b4b4b4b5a5a5a5a5a5a69' +
    '6969696976767676767676767676ffdb0043011213131e1c1e341c1c347b5445547b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b' +
    '7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7bffdd00040001ffda000c03010002110311003f00a145145711f4c7' +
    'ffd9',
  'hex',
)

/**
 * A JPEG whose EXIF holds a camera, a time, and a GPS position: the photo the card must never send the location of.
 * Big-endian TIFF: IFD0 (Make, Model, DateTime, the GPS pointer), then the GPS IFD (N, latitude, E, longitude).
 */
function photoWithLocationBytes(): Buffer {
  const ascii = (text: string): Buffer => Buffer.from(`${text}\0`, 'ascii')
  const make = ascii('Apple')
  const model = ascii('iPhone 15')
  const taken = ascii('2026:07:14 18:22:05')
  // A public square (Place de la Palud, Lausanne): no one's home.
  const latitude = [46, 31, 18] as const
  const longitude = [6, 37, 57] as const

  const ifd0Count = 4
  const ifd0Start = 8
  const ifd0Size = 2 + ifd0Count * 12 + 4
  let cursor = ifd0Start + ifd0Size
  const makeAt = cursor
  cursor += make.length
  const modelAt = cursor
  cursor += model.length
  const takenAt = cursor
  cursor += taken.length
  const gpsStart = cursor
  const gpsCount = 4
  const gpsSize = 2 + gpsCount * 12 + 4
  cursor += gpsSize
  const latitudeAt = cursor
  cursor += 24
  const longitudeAt = cursor
  cursor += 24

  const tiff = Buffer.alloc(cursor)
  tiff.write('MM', 0, 'ascii')
  tiff.writeUInt16BE(42, 2)
  tiff.writeUInt32BE(ifd0Start, 4)
  const entry = (at: number, tag: number, type: number, count: number, value: number | Buffer): void => {
    tiff.writeUInt16BE(tag, at)
    tiff.writeUInt16BE(type, at + 2)
    tiff.writeUInt32BE(count, at + 4)
    if (typeof value === 'number') tiff.writeUInt32BE(value, at + 8)
    else value.copy(tiff, at + 8)
  }
  tiff.writeUInt16BE(ifd0Count, ifd0Start)
  entry(ifd0Start + 2, 0x010f, 2, make.length, makeAt)
  entry(ifd0Start + 14, 0x0110, 2, model.length, modelAt)
  entry(ifd0Start + 26, 0x0132, 2, taken.length, takenAt)
  entry(ifd0Start + 38, 0x8825, 4, 1, gpsStart)
  make.copy(tiff, makeAt)
  model.copy(tiff, modelAt)
  taken.copy(tiff, takenAt)
  tiff.writeUInt16BE(gpsCount, gpsStart)
  entry(gpsStart + 2, 0x0001, 2, 2, Buffer.from('N\0\0\0', 'ascii'))
  entry(gpsStart + 14, 0x0002, 5, 3, latitudeAt)
  entry(gpsStart + 26, 0x0003, 2, 2, Buffer.from('E\0\0\0', 'ascii'))
  entry(gpsStart + 38, 0x0004, 5, 3, longitudeAt)
  const rationals = (at: number, parts: readonly number[]): void => {
    parts.forEach((part, i) => {
      tiff.writeUInt32BE(part, at + i * 8)
      tiff.writeUInt32BE(1, at + i * 8 + 4)
    })
  }
  rationals(latitudeAt, latitude)
  rationals(longitudeAt, longitude)

  const exif = Buffer.concat([Buffer.from('Exif\0\0', 'binary'), tiff])
  const app1 = Buffer.alloc(4)
  app1.writeUInt16BE(0xffe1, 0)
  app1.writeUInt16BE(exif.length + 2, 2)
  return Buffer.concat([Buffer.from('ffd8', 'hex'), app1, exif, JPEG_BODY])
}

function write(file: string, content: string | Buffer, mode?: number): void {
  fs.mkdirSync(path.dirname(file), { recursive: true })
  fs.writeFileSync(file, content, mode === undefined ? undefined : { mode })
}

/** Runs git in `cwd` with a fixed author and no config of the person's. */
function git(cwd: string, args: string[]): void {
  execFileSync('git', ['-c', 'init.defaultBranch=main', '-c', 'commit.gpgsign=false', ...args], {
    cwd,
    stdio: 'ignore',
    env: {
      ...process.env,
      GIT_CONFIG_GLOBAL: '/dev/null',
      GIT_CONFIG_SYSTEM: '/dev/null',
      GIT_AUTHOR_NAME: 'Tessera Team',
      GIT_AUTHOR_EMAIL: 'team@tessera.example',
      GIT_COMMITTER_NAME: 'Tessera Team',
      GIT_COMMITTER_EMAIL: 'team@tessera.example',
      GIT_AUTHOR_DATE: '2026-10-28T10:00:00Z',
      GIT_COMMITTER_DATE: '2026-10-28T10:00:00Z',
    },
  })
}

/** A `node_modules` with some weight: a few packages, each a handful of real files. */
function writeNodeModules(dir: string): void {
  const chunk = 'export const value = 1\n'.repeat(4000) // about 90 KB
  for (const pkg of ['react', 'react-dom', 'scheduler', 'lodash', 'date-fns', 'zod']) {
    write(path.join(dir, pkg, 'package.json'), `{ "name": "${pkg}", "version": "1.0.0" }\n`)
    for (let i = 0; i < 12; i++) write(path.join(dir, pkg, 'dist', `chunk-${String(i)}.js`), chunk)
  }
}

/** Builds the tree under `root`, replacing whatever was there. Returns where each item is. */
export function buildAboutFixtures(root: string): AboutFixtureTree {
  fs.rmSync(root, { recursive: true, force: true })
  const at = (...parts: string[]): string => path.join(root, ...parts)
  const home = at('home')
  const support = path.join(home, 'Library', 'Application Support')

  const tree: AboutFixtureTree = {
    root,
    home,
    downloadedDiskImage: at('Downloads', 'Tessera-2.4.1.dmg'),
    executableNamedJpg: at('Downloads', 'beach-sunset.jpg'),
    // RIGHT-TO-LEFT OVERRIDE flips what follows it: a list shows "holiday-photos-piz.png".
    bidiName: at('Downloads', 'holiday-photos-‮gnp.zip'),
    photoWithLocation: at('Photos', 'IMG_4821.jpg'),
    plainDocument: at('Documents', 'Packing list.txt'),
    hardLinkA: at('Documents', 'Q3 budget.xlsx'),
    hardLinkB: at('Documents', 'Shared with Anna', 'Q3 budget.xlsx'),
    emptyFolder: at('Documents', 'Tax return 2026'),
    repo: at('Projects', 'tessera-site'),
    repoClean: at('Projects', 'tessera-site', 'README.md'),
    repoModified: at('Projects', 'tessera-site', 'src', 'index.html'),
    repoUntracked: at('Projects', 'tessera-site', 'notes-draft.md'),
    nodeModules: at('Projects', 'tessera-site', 'node_modules'),
    appSupportById: path.join(support, 'com.apple.TextEdit'),
    insideAppSupport: path.join(support, 'com.apple.TextEdit', 'Autosave Information', 'Untitled.rtf'),
    leftoverAppSupport: path.join(support, 'com.tessera.desktop'),
    appSupportByName: path.join(support, 'Slack'),
    appSupportVendor: path.join(support, 'Google'),
    cache: path.join(home, 'Library', 'Caches', 'com.apple.Safari'),
  }

  // A download, with its record.
  write(tree.downloadedDiskImage, diskImageBytes())
  writeDownloadRecord(tree.downloadedDiskImage, {
    fileUrl: DMG_FILE_URL,
    pageUrl: DMG_PAGE_URL,
    at: '2026-03-02T14:20:00Z',
  })
  // A program wearing a picture's name, executable, downloaded too.
  write(tree.executableNamedJpg, programBytes(), 0o755)
  writeDownloadRecord(tree.executableNamedJpg, {
    fileUrl: 'https://cdn.free-wallpapers.example/hd/beach-sunset.jpg',
    pageUrl: 'https://free-wallpapers.example/beach',
    at: '2026-11-02T21:05:00Z',
  })
  // A name with a bidi override, which is really a ZIP.
  write(tree.bidiName, zipBytes())

  write(tree.photoWithLocation, photoWithLocationBytes())
  write(tree.plainDocument, 'Passports\nChargers\nSunscreen\nSwimsuits\nThe good knife\n')

  // Two names for one file.
  write(tree.hardLinkA, Buffer.alloc(18 * 1024, 0x41))
  fs.mkdirSync(path.dirname(tree.hardLinkB), { recursive: true })
  fs.linkSync(tree.hardLinkA, tree.hardLinkB)

  fs.mkdirSync(tree.emptyFolder, { recursive: true })

  // A repo: a clean file, a modified one, an untracked one, and its installed dependencies (ignored).
  write(tree.repoClean, '# Tessera\n\nThe Tessera website.\n')
  write(tree.repoModified, '<!doctype html>\n<title>Tessera</title>\n<h1>Tessera</h1>\n')
  write(path.join(tree.repo, '.gitignore'), 'node_modules/\n')
  write(path.join(tree.repo, 'package.json'), '{ "name": "tessera-site", "private": true }\n')
  git(tree.repo, ['init', '-q'])
  git(tree.repo, ['add', '-A'])
  git(tree.repo, ['commit', '-q', '-m', 'Start the site'])
  write(tree.repoModified, '<!doctype html>\n<title>Tessera</title>\n<h1>Tessera 2.4</h1>\n<p>New: tiles.</p>\n')
  write(tree.repoUntracked, 'Ideas for the next release.\n')
  writeNodeModules(tree.nodeModules)

  // A fixture home's Library, named the way apps name their folders.
  write(tree.insideAppSupport, '{\\rtf1\\ansi A note I never saved.}\n')
  write(path.join(tree.leftoverAppSupport, 'library.sqlite'), Buffer.alloc(640 * 1024, 0x00))
  write(path.join(tree.leftoverAppSupport, 'Thumbnails', 'thumb-0001.png'), Buffer.alloc(24 * 1024, 0x11))
  write(path.join(tree.appSupportByName, 'Cache', 'Cache_Data', 'data_1'), Buffer.alloc(256 * 1024, 0x22))
  write(path.join(tree.appSupportByName, 'storage', 'root-state.json'), '{}\n')
  write(path.join(tree.appSupportVendor, 'Chrome', 'Local State'), '{}\n')
  write(path.join(tree.cache, 'Cache.db'), Buffer.alloc(512 * 1024, 0x33))

  return tree
}

/** `<repo>/_ignored/about-this-qa`: `_ignored/` is git-ignored at any depth. */
function defaultRoot(): string {
  const here = path.dirname(fileURLToPath(import.meta.url))
  return path.resolve(here, '..', '..', '..', '..', '_ignored', 'about-this-qa')
}

if (process.argv[1]?.endsWith('about-fixtures.ts')) {
  const root = path.resolve(process.argv[2] ?? defaultRoot())
  const tree = buildAboutFixtures(root)
  console.log(`Built the "Tell me about this" QA folder at ${tree.root}`)
  console.log('Start the dev app with its fixture home, then open the folder:')
  console.log(`  CMDR_ABOUT_FIXTURE_HOME=${JSON.stringify(tree.home)} pnpm dev`)
}
