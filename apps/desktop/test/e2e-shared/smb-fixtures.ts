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

/** Host/port for SMB containers. Env vars override defaults for Docker networking. */
export const SMB_GUEST_HOST = process.env.SMB_E2E_GUEST_HOST ?? 'localhost'
export const SMB_GUEST_PORT = Number(process.env.SMB_E2E_GUEST_PORT ?? '9445')
export const SMB_AUTH_HOST = process.env.SMB_E2E_AUTH_HOST ?? 'localhost'
export const SMB_AUTH_PORT = Number(process.env.SMB_E2E_AUTH_PORT ?? '9446')

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
export const SMB_GUEST_MOUNT = IS_LINUX
  ? `/run/user/${LINUX_UID}/gvfs/smb-share:server=${SMB_GUEST_HOST},share=${SMB_GUEST_SHARE}`
  : `/Volumes/${SMB_GUEST_SHARE}`
export const SMB_AUTH_MOUNT = IS_LINUX
  ? `/run/user/${LINUX_UID}/gvfs/smb-share:server=${SMB_AUTH_HOST},share=${SMB_AUTH_SHARE}`
  : `/Volumes/${SMB_AUTH_SHARE}`

const DOCKER_COMPOSE_DIR = path.resolve(__dirname, '../smb-servers')

// ── Docker container management ──────────────────────────────────────────────

/** Checks whether the Docker SMB containers are running and healthy. */
export function areSmbContainersRunning(): boolean {
  try {
    const output = execSync('docker compose ps --format json 2>/dev/null', {
      cwd: DOCKER_COMPOSE_DIR,
      encoding: 'utf-8',
      timeout: 10_000,
    })
    const lines = output.trim().split('\n').filter(Boolean)
    return lines.some((l) => {
      const c = JSON.parse(l) as { Service: string; State: string }
      return c.Service === 'smb-guest' && c.State === 'running'
    })
  } catch {
    return false
  }
}

/** Starts SMB Docker containers (minimal profile: guest + auth). */
export function startSmbContainers(): void {
  // eslint-disable-next-line no-console
  console.log('Starting SMB Docker containers (minimal)...')
  execSync('./start.sh minimal', {
    cwd: DOCKER_COMPOSE_DIR,
    encoding: 'utf-8',
    timeout: 60_000,
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
 * - Linux: uses gio mount (GVFS) so the mount appears at the same path that
 *   Cmdr's mount_linux.rs will detect via `gio mount -l`
 */
export function preMountGuestShare(): void {
  if (fs.existsSync(SMB_GUEST_MOUNT)) {
    // eslint-disable-next-line no-console
    console.log(`Guest share already mounted at ${SMB_GUEST_MOUNT}`)
    return
  }

  try {
    if (IS_LINUX) {
      // Use gio mount so GVFS manages the mount — Cmdr's mount_linux.rs checks
      // `gio mount -l` for existing mounts and derives paths from GVFS.
      const smbUrl = `smb://${SMB_GUEST_HOST}/${SMB_GUEST_SHARE}`
      execSync(`gio mount --anonymous '${smbUrl}'`, { encoding: 'utf-8', timeout: 30_000 })
    } else {
      fs.mkdirSync(SMB_GUEST_MOUNT, { recursive: true })
      execSync(
        `mount_smbfs //guest@${SMB_GUEST_HOST}:${String(SMB_GUEST_PORT)}/${SMB_GUEST_SHARE} ${SMB_GUEST_MOUNT}`,
        {
          encoding: 'utf-8',
          timeout: 15_000,
        },
      )
    }
    // eslint-disable-next-line no-console
    console.log(`Mounted guest share at ${SMB_GUEST_MOUNT}`)
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : String(err)
    if (
      msg.includes('File exists') ||
      msg.includes('already mounted') ||
      msg.includes('Device or resource busy') ||
      msg.includes('Location is already mounted')
    ) {
      // eslint-disable-next-line no-console
      console.log(`Guest share already mounted at ${SMB_GUEST_MOUNT}`)
    } else {
      throw new Error(`Failed to mount guest share: ${msg}`, { cause: err })
    }
  }
}

/** Unmounts SMB shares mounted by pre-mount helpers. */
export function unmountSmbShares(): void {
  if (IS_LINUX) {
    // On Linux, use gio mount -u with the SMB URL to unmount GVFS mounts.
    const urls = [`smb://${SMB_GUEST_HOST}/${SMB_GUEST_SHARE}`, `smb://${SMB_AUTH_HOST}/${SMB_AUTH_SHARE}`]
    for (const url of urls) {
      try {
        execSync(`gio mount -u '${url}'`, { encoding: 'utf-8', timeout: 10_000 })
        // eslint-disable-next-line no-console
        console.log(`Unmounted ${url}`)
      } catch {
        // Best-effort: may already be unmounted
      }
    }
  } else {
    for (const mountPoint of [SMB_GUEST_MOUNT, SMB_AUTH_MOUNT]) {
      if (fs.existsSync(mountPoint)) {
        try {
          execSync(`umount ${mountPoint}`, { encoding: 'utf-8', timeout: 10_000 })
          // eslint-disable-next-line no-console
          console.log(`Unmounted ${mountPoint}`)
        } catch {
          // Best-effort: may already be unmounted
        }
        try {
          fs.rmdirSync(mountPoint)
        } catch {
          // Mount point may still be in use or already removed
        }
      }
    }
  }
}

// ── Combined setup/teardown ──────────────────────────────────────────────────

/**
 * Full SMB test setup.
 * - On macOS: ensures containers are running and optionally pre-mounts.
 * - On Linux (Docker): containers are managed externally by e2e-linux.sh,
 *   we only pre-mount.
 */
export function setupSmb(): void {
  if (!IS_LINUX) {
    ensureSmbContainers()
  }
  // On Linux in Docker, we use gio mount (GVFS) so paths match what Cmdr expects.
  // On macOS, mount_smbfs may fail if /Volumes/public can't be created
  // (requires sudo). In that case, skip pre-mount — the app handles it
  // via NetFSMountURLSync (which creates the mount point itself).
  try {
    preMountGuestShare()
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : String(err)
    // eslint-disable-next-line no-console
    console.warn(`Pre-mount skipped: ${msg}`)
  }
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
 * file on the server directly — GVFS discovers it naturally on the next browse.
 */
export function smbWriteFile(host: string, port: number, share: string, remoteName: string, content: string): void {
  const tmpFile = path.join(os.tmpdir(), `smb-upload-${String(Date.now())}-${remoteName}`)
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
    // eslint-disable-next-line no-console
    console.log('SMB setup complete. Tearing down...')
    teardownSmb()
    // eslint-disable-next-line no-console
    console.log('Done.')
  } catch (err: unknown) {
    // eslint-disable-next-line no-console
    console.error('SMB setup failed:', err)
    process.exit(1)
  }
}
