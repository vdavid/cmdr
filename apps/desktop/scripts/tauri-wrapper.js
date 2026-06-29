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
// What this wrapper does NOT do yet (P5+):
//   - Fixture root or clipboard mock plumbing (P5, owned by the E2E checker).

import { spawn, execFileSync } from 'child_process'
import { mkdtempSync, writeFileSync, rmSync } from 'fs'
import { tmpdir, homedir } from 'os'
import { join } from 'path'
import {
  extractWorktreeFlag,
  resolveInstanceId,
  resolveWorktreeLabel,
  deriveInstance,
  pickEphemeralPort,
  writePortFile,
  removePortFile,
} from './instance-id.js'

const TAURI_MCP_PORT_FILE = 'tauri-mcp.port'

const args = process.argv.slice(2)
const isDev = args.includes('dev')
const isBuild = args.includes('build')

// Keep `dev` out of the main clone: a dev launch regenerates bindings.ts and runs
// against the plain "dev" instance, dirtying/confusing the main checkout. The
// solo-dev workflow always runs dev from a worktree (`pnpm dev --worktree <slug>`).
// `build` is exempt (CI release builds run in the main checkout). Override with
// --allow-main / -m.
const allowMain = args.includes('--allow-main') || args.includes('-m')
const inMainWorkingTree = isMainWorkingTree()
if (isDev && !allowMain && inMainWorkingTree) {
  console.error(
    'Refusing to run dev in the main clone.\n' +
      'Dev runs in a worktree — use `pnpm dev --worktree <slug>` from a worktree, ' +
      'or pass --allow-main (-m) if you really mean it.',
  )
  process.exit(1)
}

// Parse --worktree first so we can strip it from the args we forward to Tauri. Keeps
// anything after `--` (Tauri / cargo args like `--features virtual-mtp`) intact.
const { slug: rawWorktreeSlug, rest: forwardedArgs0 } = extractWorktreeFlag(args)
// Strip our own --allow-main / -m too, so they never reach Tauri/cargo.
const forwardedArgs = forwardedArgs0.filter((a) => a !== '--allow-main' && a !== '-m')

// isMainWorkingTree reports whether the cwd is the repo's MAIN clone rather than a
// linked `git worktree`. In the main clone, --git-dir and --git-common-dir resolve
// to the same .git; in a linked worktree, --git-dir is .git/worktrees/<slug>.
// Returns false when git is absent / not a repo, so a non-git context never blocks.
function isMainWorkingTree() {
  try {
    /** @param {string} p */
    const abs = (p) => (p.startsWith('/') ? p : join(process.cwd(), p))
    const gitDir = execFileSync('git', ['rev-parse', '--git-dir'], { encoding: 'utf8' }).trim()
    const commonDir = execFileSync('git', ['rev-parse', '--git-common-dir'], { encoding: 'utf8' }).trim()
    return abs(gitDir) === abs(commonDir)
  } catch {
    return false
  }
}

// Basename of the current working tree's toplevel, e.g. "colorful-tags" for a worktree at
// `.claude/worktrees/colorful-tags`. Used as the dev-title label for a plain `pnpm dev` from
// a worktree (no `--worktree` slug). Returns null when git is absent / not a repo.
function worktreeDirName() {
  try {
    const top = execFileSync('git', ['rev-parse', '--show-toplevel'], { encoding: 'utf8' }).trim()
    return top.length > 0 ? (top.split('/').pop() ?? null) : null
  } catch {
    return null
  }
}

// Dev-only virtual MTP opt-in. `CMDR_VIRTUAL_MTP=1 pnpm dev` (or `=<dir>` for a custom
// backing dir) registers a fake Android device so MTP flows (drag&drop, transfers,
// conflict dialogs) are testable without real hardware. The Rust side is feature-gated
// (`#[cfg(feature = "virtual-mtp")]`), so we must compile the feature in AND let the
// matching env var through. The var is already in `env` (inherited); here we just append
// the cargo feature to the dev build. Adding a feature changes the feature set, so the
// first `CMDR_VIRTUAL_MTP=1 pnpm dev` after a plain run triggers a full-ish rebuild.
// Release `pnpm build` never reads this, so prod binaries stay feature-free.
// See docs/tooling/virtual-mtp.md and src-tauri/src/mtp/virtual_device.rs.
const wantsVirtualMtp = isDev && !!process.env.CMDR_VIRTUAL_MTP && process.env.CMDR_VIRTUAL_MTP.trim() !== ''
if (wantsVirtualMtp && !forwardedArgs.includes('virtual-mtp')) {
  // Cargo features live after the `--` separator that splits Tauri-CLI args from
  // `cargo run` args. Reuse an existing `--features <list>` (append, comma-joined) so we
  // don't clobber a user-passed feature set; otherwise add a fresh `-- --features` block.
  const dashDash = forwardedArgs.indexOf('--')
  const featuresIdx = dashDash >= 0 ? forwardedArgs.indexOf('--features', dashDash) : -1
  if (featuresIdx >= 0 && featuresIdx + 1 < forwardedArgs.length) {
    forwardedArgs[featuresIdx + 1] = `${forwardedArgs[featuresIdx + 1]},virtual-mtp`
  } else if (dashDash >= 0) {
    forwardedArgs.push('--features', 'virtual-mtp')
  } else {
    forwardedArgs.push('--', '--features', 'virtual-mtp')
  }
}

const env = { ...process.env }

// Dev-only: label which working tree this session runs against, so the dev-mode title bar
// can mark side-by-side worktree windows apart (e.g. "(colorful-tags) DEV MODE - …"). Vite
// bakes it into the frontend as `__CMDR_WORKTREE_LABEL__`. Skipped under E2E (CMDR_E2E_MODE)
// so E2E window titles stay as-is, and never set for prod builds. An explicit env value wins.
if (isDev && env.CMDR_E2E_MODE !== '1' && !env.CMDR_WORKTREE_LABEL) {
  const label = resolveWorktreeLabel({
    isDev,
    worktreeSlug: rawWorktreeSlug,
    isMainWorkingTree: inMainWorkingTree,
    worktreeDirName: worktreeDirName(),
  })
  if (label) env.CMDR_WORKTREE_LABEL = label
}

/** @type {string | null} */
let instanceTmpDir = null
/** Path to the per-instance data dir we wrote the tauri-mcp port file into. */
/** @type {string | null} */
let tauriMcpPortFileDir = null

try {
  const instanceId = resolveInstanceId({
    isDev,
    envInstanceId: env.CMDR_INSTANCE_ID,
    worktreeSlug: rawWorktreeSlug,
  })

  if (instanceId) {
    // P2: reserve an ephemeral port for the tauri-MCP bridge plugin (debug builds only)
    // and write `<data_dir>/tauri-mcp.port` BEFORE Tauri launches. The plugin has no
    // public bound-port accessor and silently falls back to `base_port` on exhaustion,
    // so we own discovery from the outside. Allocation goes first so we can thread the
    // chosen port through env + write the file before the long-running spawn below.
    //
    // Only allocate when we're going to dev-build (`isDev`): the bridge is gated by
    // `#[cfg(debug_assertions)]` on the Rust side, so a release `pnpm build` doesn't
    // need the port or the file.
    if (isDev && !env.CMDR_MCP_BRIDGE_PORT) {
      env.CMDR_MCP_BRIDGE_PORT = String(await pickEphemeralPort())
    }

    // P4: reserve an ephemeral port for the Vite dev server (dev only). Threaded through
    // both `CMDR_VITE_PORT` (read by `vite.config.js`) AND the generated config's
    // `build.devUrl` (read by Tauri to point the webview). Both routes must see the same
    // number or the webview loads a blank page.
    //
    // The race window between `net.createServer().listen(0)` close and Vite's actual bind
    // is small (tens of ms). `strictPort: true` in `vite.config.js` turns any collision
    // into a loud `EADDRINUSE` instead of a silent migration to a different port. See
    // docs/specs/instance-isolation-plan.md § "Wrapper-allocated ephemeral ports: race
    // and mitigation".
    /** @type {number|undefined} */
    let vitePort
    if (isDev) {
      if (env.CMDR_VITE_PORT) {
        vitePort = Number(env.CMDR_VITE_PORT)
      } else {
        vitePort = await pickEphemeralPort()
        env.CMDR_VITE_PORT = String(vitePort)
      }
    }

    const { identifier, dataDir, config } = deriveInstance({
      instanceId,
      platform: process.platform,
      home: homedir(),
      xdgDataHome: env.XDG_DATA_HOME,
      vitePort,
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

    // Write the tauri-MCP bridge port file BEFORE launching Tauri (we already have both
    // the port and the data dir). External readers see the file appear at the same moment
    // as the Tauri process; an early reader gets ECONNREFUSED on the right port and
    // retries. See docs/specs/instance-isolation-plan.md § "Wrapper-allocated ephemeral
    // ports: race and mitigation".
    if (isDev && env.CMDR_MCP_BRIDGE_PORT) {
      const bridgePort = Number(env.CMDR_MCP_BRIDGE_PORT)
      try {
        writePortFile(dataDir, TAURI_MCP_PORT_FILE, bridgePort)
        tauriMcpPortFileDir = dataDir
        console.log(`Wrote ${TAURI_MCP_PORT_FILE} = ${String(bridgePort)} to ${dataDir}`)
      } catch (err) {
        console.warn(
          `Could not write tauri-MCP port file at ${dataDir}: ${err instanceof Error ? err.message : String(err)}`,
        )
      }
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
      if (vitePort !== undefined) {
        console.log(`Using CMDR_VITE_PORT: ${String(vitePort)}`)
      }
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

function cleanupTauriMcpPortFile() {
  if (tauriMcpPortFileDir) {
    removePortFile(tauriMcpPortFileDir, TAURI_MCP_PORT_FILE)
    tauriMcpPortFileDir = null
  }
}

function cleanupAll() {
  cleanupInstanceTmpDir()
  cleanupTauriMcpPortFile()
}

process.on('exit', cleanupAll)
process.on('SIGINT', () => {
  cleanupAll()
  process.exit(130)
})
process.on('SIGTERM', () => {
  cleanupAll()
  process.exit(143)
})

tauriProcess.on('exit', (code) => {
  cleanupAll()
  process.exit(code ?? 0)
})
