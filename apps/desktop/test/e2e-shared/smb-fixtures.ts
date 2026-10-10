/**
 * SMB fixture helper for E2E tests.
 *
 * On macOS: manages Docker SMB containers and optionally pre-mounts shares
 * to avoid system password dialogs.
 * On Linux (Docker E2E): SMB containers are managed by e2e-linux.sh and
 * reachable by container name. Mounting uses `gio mount` (GVFS).
 */

import { execSync } from 'child_process'
import fs from 'fs'
import os from 'os'
import path from 'path'
import { fileURLToPath } from 'url'

const __dirname = path.dirname(fileURLToPath(import.meta.url))

// ── Constants ────────────────────────────────────────────────────────────────

const IS_LINUX = os.platform() === 'linux'

/** Host/port for SMB containers. Env vars override defaults for Docker networking.
 * Port defaults match smb2's consumer test harness (10480+). */
export const SMB_GUEST_HOST = process.env.SMB_E2E_GUEST_HOST ?? 'localhost'
export const SMB_GUEST_PORT = Number(process.env.SMB_E2E_GUEST_PORT ?? '10480')
export const SMB_AUTH_HOST = process.env.SMB_E2E_AUTH_HOST ?? 'localhost'
export const SMB_AUTH_PORT = Number(process.env.SMB_E2E_AUTH_PORT ?? '10481')

export const SMB_50SHARES_HOST = process.env.SMB_E2E_50SHARES_HOST ?? 'localhost'
export const SMB_50SHARES_PORT = Number(process.env.SMB_E2E_50SHARES_PORT ?? '10483')
export const SMB_UNICODE_HOST = process.env.SMB_E2E_UNICODE_HOST ?? 'localhost'
export const SMB_UNICODE_PORT = Number(process.env.SMB_E2E_UNICODE_PORT ?? '10484')
export const SMB_READONLY_HOST = process.env.SMB_E2E_READONLY_HOST ?? 'localhost'
export const SMB_READONLY_PORT = Number(process.env.SMB_E2E_READONLY_PORT ?? '10488')

export const SMB_AUTH_USERNAME = 'testuser'
export const SMB_AUTH_PASSWORD = 'testpass'

export const SMB_GUEST_SHARE = 'public'
export const SMB_AUTH_SHARE = 'private'

/**
 * Mount points differ by platform.
 * On Linux, gio mount (GVFS) creates mounts at /run/user/<uid>/gvfs/smb-share:server=<host>,share=<share>.
 * The E2E Docker container runs as root (uid 0).
 */
const LINUX_UID = String(IS_LINUX ? (process.getuid?.() ?? 0) : 0)
/**
 * Where the guest share is mounted when nothing else holds the path: GVFS's own path on Linux, `/Volumes/public` on
 * macOS. ❗ Not where it IS: on macOS it can land at `/Volumes/public-1`, and another share can sit at
 * `/Volumes/public`. Specs read `guestMountPoint()`.
 */
const SMB_GUEST_MOUNT = IS_LINUX
  ? `/run/user/${LINUX_UID}/gvfs/smb-share:server=${SMB_GUEST_HOST},share=${SMB_GUEST_SHARE}`
  : `/Volumes/${SMB_GUEST_SHARE}`
/**
 * Suite-specific subdirectory inside the SMB shares used by the E2E playwright
 * suite for any writes. The Rust integration tests already scope each test to
 * a unique `cmdr-test-{pid}-{ts}-{n}/` directory (see
 * `apps/desktop/src-tauri/src/file_system/volume/smb.rs::test_dir_name`), so
 * they can't collide with this subdir. Fixed name (not timestamped) so the
 * post-setup teardown step in `setupSmb` can reliably wipe stale state from
 * earlier runs.
 */
export const SMB_E2E_SUITE_DIR = 'e2e-playwright'

/** The fixture's guest share, as a mount source. */
const GUEST_SOURCE = { host: SMB_GUEST_HOST, port: SMB_GUEST_PORT, share: SMB_GUEST_SHARE }

/**
 * Where the fixture's guest share is mounted right now, found by its SOURCE, or `null` when it isn't. ❗ Specs write
 * and delete there, so a foreign share at `/Volumes/public` must never answer for it.
 */
export function guestMountPoint(): string | null {
  if (IS_LINUX) return fs.existsSync(SMB_GUEST_MOUNT) ? SMB_GUEST_MOUNT : null
  const mountOutput = execSync('mount', { encoding: 'utf-8', timeout: 10_000 })
  return fixtureMountPoints(mountOutput, [GUEST_SOURCE])[0] ?? null
}

/** What SMB mounts there are, for a failure message: `mount`'s smbfs lines on macOS, GVFS's shares on Linux. */
function describeSmbMounts(): string {
  try {
    const lines = IS_LINUX
      ? fs.readdirSync(path.dirname(SMB_GUEST_MOUNT)).filter((name) => name.startsWith('smb-share:'))
      : execSync('mount', { encoding: 'utf-8', timeout: 10_000 })
          .split('\n')
          .filter((line) => line.includes('(smbfs'))
    return lines.length > 0 ? lines.join('; ') : 'none'
  } catch {
    return 'none readable'
  }
}

/** The fixture's guest share as a URL, for messages. */
const GUEST_SOURCE_URL = `smb://${SMB_GUEST_HOST}:${String(SMB_GUEST_PORT)}/${SMB_GUEST_SHARE}`

/**
 * Where the fixture's guest share is mounted. ❗ Throws, naming the fixture and what IS mounted, when it isn't: a
 * spec that skipped here hid a mounting regression behind a green run.
 */
export function requireGuestMount(): string {
  const mount = guestMountPoint()
  if (mount === null) {
    throw new Error(`The SMB fixture ${GUEST_SOURCE_URL} isn't mounted. SMB mounts: ${describeSmbMounts()}`)
  }
  return mount
}

/**
 * The suite subdir on the mounted guest share, visible THROUGH the mount. Write through this so the writes land in
 * suite-specific space.
 *
 * ❗ `resetSmbSuiteDir` creates the subdir with smbclient, straight on the server, and a GVFS mount can go on not
 * showing it: both copy specs skipped on every Linux run because of it. So this creates it through the mount when it
 * isn't there (a subdir that already exists server-side answers EEXIST, which is fine), then waits, bounded, until
 * the mount shows it. Throws, naming the fixture and what's mounted, when it never does.
 */
export async function requireGuestSuite(timeoutMs = 10_000): Promise<string> {
  const suite = `${requireGuestMount()}/${SMB_E2E_SUITE_DIR}`
  const deadline = Date.now() + timeoutMs
  while (!fs.existsSync(suite)) {
    try {
      fs.mkdirSync(suite, { recursive: true })
    } catch {
      // Already there on the server, or the mount is still catching up: the next look tells.
    }
    if (Date.now() > deadline) {
      throw new Error(
        `The suite folder ${SMB_E2E_SUITE_DIR} on the SMB fixture ${GUEST_SOURCE_URL} never showed at ${suite}. SMB mounts: ${describeSmbMounts()}`,
      )
    }
    await new Promise((resolve) => setTimeout(resolve, 250))
  }
  return suite
}

const SMB_SERVERS_DIR = path.resolve(__dirname, '../smb-servers')
const DOCKER_COMPOSE_DIR = path.resolve(SMB_SERVERS_DIR, '.compose')

// ── Docker container management ──────────────────────────────────────────────

/** Checks whether the Docker SMB containers are running and healthy. */
export function areSmbContainersRunning(): boolean {
  try {
    const output = execSync('docker compose -p smb-consumer ps --format json 2>/dev/null', {
      cwd: DOCKER_COMPOSE_DIR,
      encoding: 'utf-8',
      timeout: 10_000,
    })
    const lines = output.trim().split('\n').filter(Boolean)
    return lines.some((l) => {
      const c = JSON.parse(l) as { Service: string; State: string }
      return c.Service === 'smb-consumer-guest' && c.State === 'running'
    })
  } catch {
    return false
  }
}

/** Starts SMB Docker containers.
 * @param mode - 'minimal' (guest+auth, default), 'core', or 'all' (14 containers) */
export function startSmbContainers(mode: 'minimal' | 'core' | 'all' = 'minimal'): void {
  console.log(`Starting SMB Docker containers (${mode})...`)
  execSync(`./start.sh ${mode}`, {
    cwd: SMB_SERVERS_DIR,
    encoding: 'utf-8',
    timeout: 120_000,
    stdio: 'inherit',
  })
}

/** Ensures Docker SMB containers are running, starts them if not. */
export function ensureSmbContainers(): void {
  if (!areSmbContainersRunning()) {
    startSmbContainers()
  }
}

// ── Mount management ─────────────────────────────────────────────────────────

/**
 * Pre-mounts the guest SMB share.
 * - macOS: uses mount_smbfs (avoids NetFSMountURLSync's permission dialog)
 * - Linux: uses gio mount (GVFS) so the mount appears in the GVFS folder that
 *   Cmdr's mount_linux.rs reads for existing mounts
 */
export function preMountGuestShare(): void {
  if (IS_LINUX) {
    preMountGuestShareLinux()
    return
  }
  const plan = preMountPlan(
    execSync('mount', { encoding: 'utf-8', timeout: 10_000 }),
    GUEST_SOURCE,
    SMB_GUEST_MOUNT,
    fs.existsSync(SMB_GUEST_MOUNT),
  )
  if (plan.kind === 'reuse') {
    console.log(`Guest share already mounted at ${plan.path}`)
    return
  }
  if (plan.kind === 'occupied') {
    // ❗ Someone else's share (or folder) holds the path: leave it alone. The app mounts
    // the fixture itself (NetFS picks a free path), and specs find it by source.
    console.log(`${SMB_GUEST_MOUNT} holds something that isn't the fixture; not pre-mounting over it`)
    return
  }
  fs.mkdirSync(plan.path, { recursive: true })
  execSync(`mount_smbfs //guest@${SMB_GUEST_HOST}:${String(SMB_GUEST_PORT)}/${SMB_GUEST_SHARE} ${plan.path}`, {
    encoding: 'utf-8',
    timeout: 15_000,
  })
  console.log(`Mounted guest share at ${plan.path}`)
}

/** Linux: `gio mount`, whose GVFS path already names the share's source, so the path answers for it. */
function preMountGuestShareLinux(): void {
  if (fs.existsSync(SMB_GUEST_MOUNT)) {
    console.log(`Guest share already mounted at ${SMB_GUEST_MOUNT}`)
    return
  }
  try {
    // Cmdr's mount_linux.rs finds existing mounts by their GVFS folder names.
    const smbUrl = `smb://${SMB_GUEST_HOST}/${SMB_GUEST_SHARE}`
    execSync(`gio mount --anonymous '${smbUrl}'`, { encoding: 'utf-8', timeout: 30_000 })
    console.log(`Mounted guest share at ${SMB_GUEST_MOUNT}`)
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : String(err)
    if (msg.includes('already mounted') || msg.includes('Location is already mounted')) {
      console.log(`Guest share already mounted at ${SMB_GUEST_MOUNT}`)
    } else {
      throw new Error(`Failed to mount guest share: ${msg}`, { cause: err })
    }
  }
}

/** What `preMountGuestShare` does on macOS. */
export type PreMountPlan = { kind: 'reuse'; path: string } | { kind: 'mount'; path: string } | { kind: 'occupied' }

/**
 * The macOS pre-mount's decision. ❗ "Already mounted" means the FIXTURE's share is mounted, found by source in
 * `mountOutput`, wherever it landed; a path that merely exists (`defaultPathTaken`) is someone else's and is left
 * alone, since E2E writes and deletes on the fixture share.
 */
export function preMountPlan(
  mountOutput: string,
  source: SmbShareSource,
  defaultPath: string,
  defaultPathTaken: boolean,
): PreMountPlan {
  const ours = fixtureMountPoints(mountOutput, [source])
  if (ours.length > 0) return { kind: 'reuse', path: ours[0] }
  return defaultPathTaken ? { kind: 'occupied' } : { kind: 'mount', path: defaultPath }
}

/** An SMB share on one server: what a fixture mount's source names. */
export interface SmbShareSource {
  host: string
  port: number
  share: string
}

/** One `mount` line for an SMB mount: `//[user@]host[:port]/share on <mount point> (smbfs, …)`. */
const SMBFS_MOUNT_LINE = /^\/\/(?:[^@]*@)?([^/:]+)(?::(\d+))?\/(.+?) on (.+) \(smbfs[,)]/

/**
 * The mount points, in `mountOutput` (the macOS `mount` command's), whose SOURCE is one of `shares`: that host (any
 * case), that port (445 when the line names none), and that share (any case), wherever the mount landed.
 *
 * ❗ By source, ❌ never by path: `/Volumes/public` is also where a person's NAS share or a dev session's mount of
 * another server lands, and a teardown that unmounted the path took theirs.
 */
export function fixtureMountPoints(mountOutput: string, shares: SmbShareSource[]): string[] {
  const points: string[] = []
  for (const line of mountOutput.split('\n')) {
    const match = SMBFS_MOUNT_LINE.exec(line)
    if (!match) continue
    const [, host, port, share, mountPoint] = match
    const mounted = { host: host.toLowerCase(), port: port ? Number(port) : 445, share: share.toLowerCase() }
    const ours = shares.some(
      (s) =>
        s.host.toLowerCase() === mounted.host && s.port === mounted.port && s.share.toLowerCase() === mounted.share,
    )
    if (ours) points.push(mountPoint)
  }
  return points
}

/**
 * Unmounts the fixture's own SMB shares, the ones the pre-mount helpers mount: on macOS every mount whose source is
 * the fixture's guest or auth share on the fixture's port, wherever it landed, and nothing else.
 */
export function unmountSmbShares(): void {
  if (IS_LINUX) {
    // On Linux, use gio mount -u with the SMB URL to unmount GVFS mounts.
    const urls = [`smb://${SMB_GUEST_HOST}/${SMB_GUEST_SHARE}`, `smb://${SMB_AUTH_HOST}/${SMB_AUTH_SHARE}`]
    for (const url of urls) {
      try {
        execSync(`gio mount -u '${url}'`, { encoding: 'utf-8', timeout: 10_000 })
        console.log(`Unmounted ${url}`)
      } catch {
        // Best-effort: may already be unmounted
      }
    }
    return
  }
  const mountOutput = execSync('mount', { encoding: 'utf-8', timeout: 10_000 })
  const ours = fixtureMountPoints(mountOutput, [
    { host: SMB_GUEST_HOST, port: SMB_GUEST_PORT, share: SMB_GUEST_SHARE },
    { host: SMB_AUTH_HOST, port: SMB_AUTH_PORT, share: SMB_AUTH_SHARE },
  ])
  for (const mountPoint of ours) {
    try {
      execSync(`umount '${mountPoint.replaceAll("'", "'\\''")}'`, { encoding: 'utf-8', timeout: 10_000 })
      console.log(`Unmounted ${mountPoint}`)
    } catch {
      // Best-effort: may already be unmounted
    }
    try {
      fs.rmdirSync(mountPoint)
    } catch {
      // Mount point may still be in use, already removed, or owned by the system
    }
  }
}

// ── Combined setup/teardown ──────────────────────────────────────────────────

/**
 * Wipes any leftover state from the suite subdir on the SMB share and
 * recreates it empty. Uses smbclient directly (not the GVFS mount) because
 * GVFS caching can mask just-written files for the first read. This makes
 * subsequent suite writes deterministic: every cross-storage test starts
 * with the same clean directory state, regardless of what an earlier run
 * left behind.
 */
function resetSmbSuiteDir(host: string, port: number, share: string): void {
  const cmd = [
    `cd ${SMB_E2E_SUITE_DIR} || mkdir ${SMB_E2E_SUITE_DIR}`,
    `cd ${SMB_E2E_SUITE_DIR}`,
    // Best-effort cleanup: list+delete every file. Subdirectories aren't
    // currently created here, but if they ever are this needs to be made
    // recursive (smbclient `dir` + `rmdir` per entry).
    'ls',
  ].join(';')
  try {
    const out = execSync(`smbclient '//${host}/${share}' -N -p ${String(port)} -c '${cmd}'`, {
      encoding: 'utf-8',
      timeout: 15_000,
    })
    // Parse the ls output and delete real files (ignore '.' / '..' and DR-flagged dir entries).
    const entries: string[] = []
    for (const line of out.split('\n')) {
      const m = line.match(/^\s+(\S.*?)\s+([ADHRS]+)\s+\d+\s+/)
      if (!m) continue
      const name = m[1].trim()
      const flags = m[2]
      if (name === '.' || name === '..') continue
      if (flags.includes('D')) continue // skip dirs (none expected today)
      entries.push(name)
    }
    if (entries.length > 0) {
      const delCmd = ['cd ' + SMB_E2E_SUITE_DIR, ...entries.map((n) => `del ${n}`)].join(';')
      execSync(`smbclient '//${host}/${share}' -N -p ${String(port)} -c '${delCmd}'`, {
        encoding: 'utf-8',
        timeout: 15_000,
      })
    }
    console.log(
      `SMB suite dir reset: //${host}/${share}/${SMB_E2E_SUITE_DIR} (cleared ${String(entries.length)} file(s))`,
    )
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : String(err)
    console.warn(`SMB suite dir reset failed (continuing anyway): ${msg}`)
  }
}

/**
 * Full SMB test setup.
 * - On macOS: ensures containers are running and optionally pre-mounts.
 * - On Linux (Docker): containers are managed externally by e2e-linux.sh,
 *   we only pre-mount.
 * - Always: resets the suite subdir on the guest share (write isolation
 *   from the Rust integration tests, which use their own `cmdr-test-{pid}-{ts}-{n}`
 *   subdirs).
 */
export function setupSmb(): void {
  if (!IS_LINUX) {
    ensureSmbContainers()
  }
  // On Linux in Docker, we use gio mount (GVFS) so paths match what Cmdr expects.
  // On macOS, mount_smbfs may fail if /Volumes/public can't be created
  // (requires sudo). In that case, skip pre-mount. The app handles it
  // via NetFSMountURLSync (which creates the mount point itself).
  try {
    preMountGuestShare()
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : String(err)
    console.warn(`Pre-mount skipped: ${msg}`)
  }
  resetSmbSuiteDir(SMB_GUEST_HOST, SMB_GUEST_PORT, SMB_GUEST_SHARE)
}

/** Full SMB test teardown: unmount shares (containers left running for reuse). */
export function teardownSmb(): void {
  unmountSmbShares()
}

// ── Server-side file operations ───────────────────────────────────────────────

/**
 * Writes a file directly to the SMB server via `smbclient`, bypassing GVFS.
 *
 * GVFS has a caching layer, so files written through the GVFS mount path
 * (`fs.writeFileSync` to `/run/user/.../gvfs/...`) may not be immediately
 * visible when the app reads the same path. Writing via `smbclient` puts the
 * file on the server directly, and GVFS discovers it naturally on the next browse.
 */
export function smbWriteFile(host: string, port: number, share: string, remoteName: string, content: string): void {
  // The base name only: `remoteName` may carry a folder (`e2e-playwright/…`), which isn't a folder in the temp dir.
  const tmpFile = path.join(os.tmpdir(), `smb-upload-${String(Date.now())}-${path.basename(remoteName)}`)
  try {
    fs.writeFileSync(tmpFile, content)
    execSync(`smbclient '//${host}/${share}' -N -p ${String(port)} -c 'put ${tmpFile} ${remoteName}'`, {
      encoding: 'utf-8',
      timeout: 15_000,
    })
  } finally {
    try {
      fs.unlinkSync(tmpFile)
    } catch {
      /* best-effort cleanup */
    }
  }
}

// Allow running directly: npx tsx apps/desktop/test/e2e-shared/smb-fixtures.ts
if (process.argv[1]?.endsWith('smb-fixtures.ts')) {
  try {
    setupSmb()
    console.log('SMB setup complete. Tearing down...')
    teardownSmb()
    console.log('Done.')
  } catch (err: unknown) {
    console.error('SMB setup failed:', err)
    process.exit(1)
  }
}
