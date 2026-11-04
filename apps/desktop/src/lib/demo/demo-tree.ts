/**
 * The demo's file system: one home folder with enough variety to try sorting, hidden files,
 * type-to-jump, and a big folder. PLACEHOLDER content: David decides the real tree (issue #302).
 */

export type DemoEntryKind = 'file' | 'dir' | 'symlink'

/** One entry of the in-memory volume the WASM engine lists. Parent folders are listed explicitly. */
export interface DemoEntry {
  path: string
  kind: DemoEntryKind
  size?: number
  modifiedMs?: number
  createdMs?: number
  symlinkTarget?: string
}

export const DEMO_VOLUME_ID = 'root'
export const DEMO_ROOT = '/'
export const DEMO_HOME = '/Users/you'

const DAY = 24 * 60 * 60 * 1000
/** A fixed "now", so the demo's dates read the same for every visitor and in every test. */
const NOW = Date.UTC(2026, 9, 1, 12, 0, 0)

type Spec = [name: string, size: number, daysAgo: number]

function files(dir: string, specs: Spec[]): DemoEntry[] {
  return specs.map(([name, size, daysAgo]) => ({
    path: `${dir}/${name}`,
    kind: 'file',
    size,
    modifiedMs: NOW - daysAgo * DAY,
  }))
}

function dirs(parent: string, names: string[], daysAgo = 3): DemoEntry[] {
  return names.map((name, i) => ({ path: `${parent}/${name}`, kind: 'dir', modifiedMs: NOW - (daysAgo + i) * DAY }))
}

/** 1,200 numbered photos, so a visitor can feel how a big folder scrolls and sorts. */
function bigFolder(dir: string): DemoEntry[] {
  return Array.from({ length: 1200 }, (_, i) => ({
    path: `${dir}/IMG_${String(i + 1).padStart(4, '0')}.heic`,
    kind: 'file',
    size: 1_800_000 + ((i * 7919) % 2_400_000),
    modifiedMs: NOW - Math.floor(i / 40) * DAY - (i % 40) * 60_000,
  }))
}

const H = DEMO_HOME

export const demoEntries: DemoEntry[] = [
  ...dirs('', ['Users']),
  ...dirs('/Users', ['you']),
  ...dirs(H, ['Desktop', 'Documents', 'Downloads', 'Music', 'Pictures', 'Projects']),
  ...files(H, [
    ['.zshrc', 1_240, 40],
    ['.gitconfig', 310, 120],
    ['notes.md', 4_820, 0],
  ]),
  ...dirs(H, ['.config'], 60),

  ...files(`${H}/Desktop`, [
    ['Screenshot 2026-09-30 at 14.02.11.png', 812_455, 1],
    ['todo.txt', 512, 0],
  ]),

  ...dirs(`${H}/Documents`, ['Invoices', 'Taxes 2025', 'Travel']),
  ...files(`${H}/Documents`, [
    ['Annual report.pdf', 2_481_220, 12],
    ['Budget 2026.xlsx', 48_310, 5],
    ['Lease agreement.pdf', 912_004, 200],
    ['Meeting notes.docx', 21_877, 2],
    ['Recipe ideas.md', 3_102, 30],
    // A non-Latin name, so Brief mode's column widths get measured on more than ASCII.
    ['旅行の計画.txt', 1_844, 6],
  ]),
  ...files(`${H}/Documents/Invoices`, [
    ['invoice-2026-01.pdf', 88_120, 270],
    ['invoice-2026-02.pdf', 90_455, 240],
    ['invoice-2026-03.pdf', 87_902, 210],
    ['invoice-2026-10.pdf', 91_330, 1],
    ['invoice-2026-11.pdf', 89_771, 0],
  ]),
  ...files(`${H}/Documents/Taxes 2025`, [
    ['Tax return 2025.pdf', 1_204_331, 150],
    ['Receipts.zip', 14_880_120, 160],
  ]),
  ...files(`${H}/Documents/Travel`, [
    ['Boarding pass.pdf', 120_441, 20],
    ['Itinerary.md', 2_210, 22],
  ]),

  ...files(`${H}/Downloads`, [
    ['Cmdr.dmg', 38_402_118, 0],
    ['dataset.csv', 120_448_910, 3],
    ['font-pack.zip', 6_330_120, 8],
    ['podcast-episode-42.mp3', 54_120_448, 15],
  ]),

  ...dirs(`${H}/Music`, ['Playlists']),
  ...files(`${H}/Music`, [
    ['Morning run.m4a', 7_441_002, 45],
    ['Rain sounds.flac', 120_004_331, 90],
  ]),

  ...dirs(`${H}/Pictures`, ['Camera roll', 'Wallpapers']),
  ...bigFolder(`${H}/Pictures/Camera roll`),
  ...files(`${H}/Pictures/Wallpapers`, [
    ['aurora.jpg', 4_201_887, 300],
    ['fjord.jpg', 5_002_118, 310],
    ['forest.jpg', 3_877_010, 320],
  ]),

  ...dirs(`${H}/Projects`, ['website', 'rust-cli']),
  ...files(`${H}/Projects/website`, [
    ['index.html', 6_120, 4],
    ['styles.css', 3_880, 4],
    ['README.md', 1_420, 9],
  ]),
  ...files(`${H}/Projects/rust-cli`, [
    ['Cargo.toml', 612, 6],
    ['main.rs', 9_220, 1],
  ]),
]
