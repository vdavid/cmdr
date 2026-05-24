// Tauri wrapper. Single source of truth for dev vs prod path separation and per-instance
// composition. See docs/specs/instance-isolation-plan.md for the design and
// docs/security.md#withglobaltauri for the security context.
//
// Responsibilities:
//   - Parse --worktree <slug> (or --worktree=<slug>) BEFORE the `--` separator.
//   - Resolve CMDR_INSTANCE_ID (existing env wins, then --worktree-derived, then "dev",
//     then unset for prod). Sanitization (lowercase ASCII, max 32 chars, etc.) lives in
//     instance-id.js so it's unit-testable.
//   - Compose CMDR_DATA_DIR to mirror the Tauri-side app_data_dir() for the same
//     identifier (so both routes agree without round-tripping through Tauri's API).
//   - Write a fresh tauri.instance.json under $TMPDIR/cmdr-tauri-instance-<rand>/ and pass
//     it as -c <absolute path>. /tmp self-prunes; the exit handler is the cheap
//     best-effort path.
//   - Force the file-backed secret store for any non-prod instance so dev/E2E never pop
//     the Keychain password dialog.
//
// What this wrapper does NOT do yet (P2+):
//   - Allocate ephemeral MCP / Tauri-MCP-bridge ports.
//   - Allocate the Vite dev port.
//   - Keychain SERVICE_NAME suffix (Rust side, in P3).
//   - Fixture root or clipboard mock plumbing (P5, owned by the E2E checker).

import { spawn } from 'child_process'
import { mkdtempSync, writeFileSync, rmSync } from 'fs'
import { tmpdir, homedir } from 'os'
import { join } from 'path'
import { extractWorktreeFlag, resolveInstanceId, deriveInstance } from './instance-id.js'

const args = process.argv.slice(2)
const isDev = args.includes('dev')
const isBuild = args.includes('build')

// Parse --worktree first so we can strip it from the args we forward to Tauri. Keeps
// anything after `--` (Tauri / cargo args like `--features virtual-mtp`) intact.
const { slug: rawWorktreeSlug, rest: forwardedArgs } = extractWorktreeFlag(args)

const env = { ...process.env }
/** @type {string | null} */
let instanceTmpDir = null

try {
  const instanceId = resolveInstanceId({
    isDev,
    envInstanceId: env.CMDR_INSTANCE_ID,
    worktreeSlug: rawWorktreeSlug,
  })

  if (instanceId) {
    const { identifier, dataDir, config } = deriveInstance({
      instanceId,
      platform: process.platform,
      home: homedir(),
      xdgDataHome: env.XDG_DATA_HOME,
    })

    env.CMDR_INSTANCE_ID = instanceId

    // CMDR_DATA_DIR is authoritative for direct file I/O (crash reports, logs, file-backed
    // secret store, etc.) per the precedence rules in instance-isolation-plan.md. Tauri's
    // own app_data_dir() honors the identifier in the generated config and lands on the
    // same path. Both routes agree.
    if (!env.CMDR_DATA_DIR) {
      env.CMDR_DATA_DIR = dataDir
    }

    // Non-prod uses the plain-file secret store so we never trigger the macOS Keychain
    // password dialog mid-test or mid-dev. Don't override an explicit setting.
    if (!env.CMDR_SECRET_STORE) {
      env.CMDR_SECRET_STORE = 'file'
    }

    if (config) {
      // Tauri reads identifier BEFORE any IPC handler exists, so the only way to override
      // it is `-c <path>` at startup. We put the file under $TMPDIR (NOT in the repo) so
      // the worktree stays clean even on a crash; /tmp self-prunes on macOS as a fallback.
      instanceTmpDir = mkdtempSync(join(tmpdir(), 'cmdr-tauri-instance-'))
      const configPath = join(instanceTmpDir, 'tauri.instance.json')
      writeFileSync(configPath, JSON.stringify(config, null, 2))

      const dashDashIndex = forwardedArgs.indexOf('--')
      if (dashDashIndex >= 0) {
        forwardedArgs.splice(dashDashIndex, 0, '-c', configPath)
      } else {
        forwardedArgs.push('-c', configPath)
      }

      console.log(`Using CMDR_INSTANCE_ID: ${instanceId} (identifier=${identifier})`)
      console.log(`Using CMDR_DATA_DIR: ${env.CMDR_DATA_DIR}`)
    }
  }
} catch (err) {
  console.error(err instanceof Error ? err.message : String(err))
  process.exit(1)
}

// macOS prod build: default to universal binary unless an explicit target is set.
const isMacOS = process.platform === 'darwin'
if (isBuild && isMacOS && !forwardedArgs.includes('--target') && !forwardedArgs.includes('-t')) {
  forwardedArgs.push('--target', 'universal-apple-darwin')
}

const tauriProcess = spawn('pnpm', ['exec', 'tauri', ...forwardedArgs], {
  stdio: 'inherit',
  env,
})

function cleanupInstanceTmpDir() {
  if (instanceTmpDir) {
    try {
      rmSync(instanceTmpDir, { recursive: true, force: true })
    } catch {
      // Best-effort. /tmp auto-prunes on macOS; Linux's tmpfs is also fine on reboot.
    }
    instanceTmpDir = null
  }
}

process.on('exit', cleanupInstanceTmpDir)
process.on('SIGINT', () => {
  cleanupInstanceTmpDir()
  process.exit(130)
})
process.on('SIGTERM', () => {
  cleanupInstanceTmpDir()
  process.exit(143)
})

tauriProcess.on('exit', (code) => {
  cleanupInstanceTmpDir()
  process.exit(code ?? 0)
})
