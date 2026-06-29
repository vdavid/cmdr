// Pure helpers for the CMDR_INSTANCE_ID primitive. Kept side-effect-free so vitest can
// exercise them without spawning Tauri. See docs/specs/instance-isolation-plan.md for the
// full design.
//
// One env var (CMDR_INSTANCE_ID) drives every per-instance suffix the wrapper composes:
//   - prod (pnpm build):            unset
//   - pnpm dev (no --worktree):     "dev"
//   - pnpm dev --worktree foo:      "dev-<sanitized slug>"
//   - E2E checker (set externally): preserved as-is
//
// The wrapper derives CMDR_DATA_DIR, bundle identifier, productName, and (in later phases)
// Vite port + MCP ports from this single string.

import { createServer } from 'net'
import { mkdirSync, openSync, writeSync, fsyncSync, closeSync, renameSync, unlinkSync } from 'fs'
import { join } from 'path'

const PROD_IDENTIFIER = 'com.veszelovszki.cmdr'
const PROD_PRODUCT_NAME = 'Cmdr'
const MAX_SLUG_LEN = 32

/**
 * Sanitize a --worktree slug to lowercase ASCII [a-z0-9-], collapsed dashes, trimmed, max 32 chars.
 * The wrapper does NOT validate the slug against the actual worktree directory name. The user
 * picks their own slug; this just makes sure the result is safe for a CFBundleIdentifier.
 *
 * Returns the sanitized slug, or null if the input collapses to empty (caller throws).
 *
 * @param {unknown} raw
 * @returns {string|null}
 */
export function sanitizeWorktreeSlug(raw) {
  if (typeof raw !== 'string') return null
  // Lowercase, replace any non-[a-z0-9-] with '-', collapse runs, trim leading/trailing '-'.
  const cleaned = raw
    .toLowerCase()
    .replace(/[^a-z0-9-]+/g, '-')
    .replace(/-+/g, '-')
    .replace(/^-|-$/g, '')
    .slice(0, MAX_SLUG_LEN)
    .replace(/-+$/, '') // re-trim in case slice cut mid-run
  return cleaned.length > 0 ? cleaned : null
}

/**
 * Derive CMDR_INSTANCE_ID for a dev or prod invocation.
 *
 * Precedence:
 *   1. existing env (caller responsible for setting, e.g. the E2E checker)
 *   2. --worktree <slug> in dev mode → "dev-<sanitized>"
 *   3. dev mode with no flag → "dev"
 *   4. prod / anything else → null (caller leaves the env unset)
 *
 * @param {object} opts
 * @param {boolean} opts.isDev
 * @param {string|undefined} opts.envInstanceId  the current value of CMDR_INSTANCE_ID, if any
 * @param {string|null|undefined} opts.worktreeSlug  raw --worktree argument (pre-sanitization)
 * @returns {string|null}
 */
export function resolveInstanceId({ isDev, envInstanceId, worktreeSlug }) {
  if (envInstanceId && envInstanceId.length > 0) return envInstanceId
  if (!isDev) return null
  if (worktreeSlug != null) {
    const slug = sanitizeWorktreeSlug(worktreeSlug)
    if (slug === null) {
      throw new Error(
        `--worktree must be 1-${MAX_SLUG_LEN} alphanumeric or dash characters after sanitization (got: ${JSON.stringify(worktreeSlug)})`,
      )
    }
    return `dev-${slug}`
  }
  return 'dev'
}

/**
 * Human label of which working tree a dev session runs against, shown in the dev-mode
 * title bar (baked into the frontend via `__CMDR_WORKTREE_LABEL__`) so side-by-side
 * worktree windows are tellable apart. Mirrors how the session was launched:
 *   - `--worktree <slug>`                     → that slug (sanitized, same as the instance ID)
 *   - `-m` / main clone, no slug              → "main"
 *   - plain dev launch from a linked worktree → the worktree directory name
 *
 * Returns null when there's nothing meaningful to show (prod, or no info available), so the
 * caller leaves `CMDR_WORKTREE_LABEL` unset.
 *
 * @param {object} opts
 * @param {boolean} opts.isDev
 * @param {string|null|undefined} opts.worktreeSlug  raw --worktree argument (pre-sanitization)
 * @param {boolean} opts.isMainWorkingTree  cwd is the repo's main clone, not a linked worktree
 * @param {string|null|undefined} opts.worktreeDirName  basename of the worktree toplevel (linked worktrees)
 * @returns {string|null}
 */
export function resolveWorktreeLabel({ isDev, worktreeSlug, isMainWorkingTree, worktreeDirName }) {
  if (!isDev) return null
  if (worktreeSlug != null) {
    // Show the sanitized slug so the label matches the actual instance identity (data dir,
    // Dock name). Returns null for an unsanitizable slug; the wrapper then bails anyway when
    // resolveInstanceId throws on the same input, so no label is the harmless interim state.
    return sanitizeWorktreeSlug(worktreeSlug)
  }
  if (isMainWorkingTree) return 'main'
  return worktreeDirName != null && worktreeDirName.length > 0 ? worktreeDirName : null
}

/**
 * Compute the Tauri-equivalent app_data_dir for an identifier on this OS. Mirrors the
 * platform branches in resolved_app_data_dir on the Rust side and the legacy block this
 * file replaces in tauri-wrapper.js.
 *
 * @param {object} opts
 * @param {string} opts.identifier  e.g. com.veszelovszki.cmdr-dev
 * @param {NodeJS.Platform} opts.platform
 * @param {string} opts.home  homedir()
 * @param {string|undefined} opts.xdgDataHome  process.env.XDG_DATA_HOME
 * @returns {string}
 */
export function computeAppDataDir({ identifier, platform, home, xdgDataHome }) {
  if (platform === 'darwin') {
    return join(home, 'Library', 'Application Support', identifier)
  }
  const base = xdgDataHome && xdgDataHome.length > 0 ? xdgDataHome : join(home, '.local', 'share')
  return join(base, identifier)
}

/**
 * Compose the bundle identifier from an instance ID. Unset → prod default.
 *
 * @param {string|null} instanceId
 * @returns {string}
 */
export function bundleIdentifier(instanceId) {
  return instanceId ? `${PROD_IDENTIFIER}-${instanceId}` : PROD_IDENTIFIER
}

/**
 * Compose the Dock / process label (productName) from an instance ID. Unset → "Cmdr".
 *
 * Special-cases E2E instance IDs of the shape `e2e-<kind>-<pid>` (set by the Go checker,
 * see scripts/check/checks/desktop-svelte-e2e-playwright.go) into `Cmdr (E2E <kind>)` so
 * cleanup scripts can target only the right processes via `pgrep -f 'Cmdr (E2E '`. The
 * `<pid>` is dropped from the label because it bloats the Dock string and is recoverable
 * from `ps` anyway. Dev / dev-<slug> / other inputs stringify as-is.
 *
 * @param {string|null} instanceId
 * @returns {string}
 */
export function productName(instanceId) {
  if (!instanceId) return PROD_PRODUCT_NAME
  const e2eMatch = /^e2e-([a-z0-9-]+?)-(\d+)$/.exec(instanceId)
  if (e2eMatch) return `Cmdr (E2E ${e2eMatch[1]})`
  return `Cmdr (${instanceId})`
}

/**
 * Pull a --worktree value out of an argv array, returning { slug, rest }.
 * - Honors the `--` separator: anything after it is left as-is for Tauri.
 * - Recognizes `--worktree foo` and `--worktree=foo`.
 * - Removes the consumed tokens from the returned `rest`.
 *
 * @param {string[]} argv
 * @returns {{ slug: string|null, rest: string[] }}
 */
export function extractWorktreeFlag(argv) {
  const sepIdx = argv.indexOf('--')
  const beforeSep = sepIdx >= 0 ? argv.slice(0, sepIdx) : argv.slice()
  const afterSep = sepIdx >= 0 ? argv.slice(sepIdx) : []

  let slug = null
  const kept = []
  for (let i = 0; i < beforeSep.length; i++) {
    const a = beforeSep[i]
    if (a === '--worktree') {
      slug = beforeSep[i + 1] ?? null
      i += 1 // skip the value
      continue
    }
    if (a.startsWith('--worktree=')) {
      slug = a.slice('--worktree='.length)
      continue
    }
    kept.push(a)
  }
  return { slug, rest: [...kept, ...afterSep] }
}

/**
 * @typedef {{
 *   $schema: string,
 *   productName: string,
 *   identifier: string,
 *   app: { withGlobalTauri: boolean },
 *   plugins: { updater: { endpoints: string[] } },
 *   build?: { devUrl: string },
 * }} InstanceConfig
 */

/**
 * Build the Tauri config payload that the wrapper writes to disk and passes via -c.
 *
 * For prod (instanceId null), returns null: the wrapper omits -c entirely so canonical
 * tauri.conf.json governs the build.
 *
 * When `vitePort` is set (dev only; the wrapper reserves an ephemeral port in P4), the
 * config also overrides `build.devUrl` so Tauri points the webview at the same port Vite
 * actually binds. Without the override, Tauri falls back to the static
 * `http://localhost:1420` in `tauri.conf.json` and the webview loads a blank page when Vite
 * is on a different port.
 *
 * The updater endpoints list always points at a dead URL for non-prod instances so a stray
 * dev or E2E build never phones home to the real `api.getcmdr.com`.
 *
 * @param {string|null} instanceId
 * @param {object} [opts]
 * @param {number} [opts.vitePort]
 * @returns {InstanceConfig|null}
 */
export function buildInstanceConfig(instanceId, opts = {}) {
  if (!instanceId) return null
  /** @type {InstanceConfig} */
  const config = {
    $schema: 'https://schema.tauri.app/config/2',
    productName: productName(instanceId),
    identifier: bundleIdentifier(instanceId),
    app: {
      withGlobalTauri: true,
    },
    plugins: {
      updater: {
        // Dead URL so non-prod instances never phone home accidentally. Real prod
        // (instanceId null) returns the whole-null config above so canonical
        // tauri.conf.json's getcmdr.com endpoint applies.
        endpoints: ['https://localhost.invalid/no-updater'],
      },
    },
  }
  if (typeof opts.vitePort === 'number') {
    if (!Number.isInteger(opts.vitePort) || opts.vitePort < 1 || opts.vitePort > 65535) {
      throw new Error(`buildInstanceConfig: vitePort must be a 1-65535 integer, got ${String(opts.vitePort)}`)
    }
    config.build = { devUrl: `http://localhost:${String(opts.vitePort)}` }
  }
  return config
}

/**
 * Convenience for tests + the wrapper: from an instance ID, compute the (identifier, data dir,
 * config payload) triple in one place so the precedence rules can't drift.
 *
 * @param {object} opts
 * @param {string|null} opts.instanceId
 * @param {NodeJS.Platform} opts.platform
 * @param {string} opts.home
 * @param {string|undefined} opts.xdgDataHome
 * @param {number} [opts.vitePort]  threaded into `build.devUrl` (dev only)
 * @returns {{ identifier: string, dataDir: string, config: InstanceConfig|null }}
 */
export function deriveInstance({ instanceId, platform, home, xdgDataHome, vitePort }) {
  const identifier = bundleIdentifier(instanceId)
  const dataDir = computeAppDataDir({ identifier, platform, home, xdgDataHome })
  const config = buildInstanceConfig(instanceId, typeof vitePort === 'number' ? { vitePort } : {})
  return { identifier, dataDir, config }
}

/**
 * Reserve an ephemeral TCP port via `net.createServer().listen(0)`. Closes the listener
 * immediately so the caller can rebind it. There's a small race window between close and
 * the downstream bind, identical to the trick we'll use for Vite in P4. Mitigation in P2's
 * case is on the Rust side (500 ms post-bind probe + warn log).
 *
 * @returns {Promise<number>}
 */
export function pickEphemeralPort() {
  return new Promise((resolve, reject) => {
    const server = createServer()
    server.unref() // don't hold the Node event loop open if anything goes wrong
    server.on('error', reject)
    server.listen({ host: '127.0.0.1', port: 0 }, () => {
      const addr = server.address()
      if (addr === null || typeof addr === 'string') {
        server.close()
        reject(new Error(`net.createServer returned unexpected address: ${JSON.stringify(addr)}`))
        return
      }
      const { port } = addr
      server.close((closeErr) => {
        if (closeErr) {
          reject(closeErr)
        } else {
          resolve(port)
        }
      })
    })
  })
}

/**
 * Atomically write `<dir>/<name>` containing `port + "\n"`. Mirrors the Rust
 * `port_file::write_port_file` protocol (tempfile + fsync + rename) so wrapper-written
 * files and Rust-written files use the same on-disk contract.
 *
 * @param {string} dir
 * @param {string} name  e.g. "tauri-mcp.port"
 * @param {number} port
 */
export function writePortFile(dir, name, port) {
  if (!Number.isInteger(port) || port < 0 || port > 65535) {
    throw new Error(`writePortFile: port must be a u16, got ${String(port)}`)
  }
  mkdirSync(dir, { recursive: true })
  const finalPath = join(dir, name)
  const tmpPath = join(dir, `${name}.tmp.${String(process.pid)}`)
  let fd = null
  try {
    // 0o600 (owner-only) baked in at create time (no umask 0o644 window). The atomic
    // rename below preserves the mode, so the final file is owner-only too. Mirrors the
    // Rust `port_file.rs` hardening. The third `openSync` arg is ignored on platforms
    // without POSIX mode bits (Windows).
    fd = openSync(tmpPath, 'w', 0o600)
    writeSync(fd, `${String(port)}\n`)
    fsyncSync(fd)
  } finally {
    if (fd !== null) closeSync(fd)
  }
  try {
    renameSync(tmpPath, finalPath)
  } catch (err) {
    try {
      unlinkSync(tmpPath)
    } catch {
      // best-effort cleanup
    }
    throw err
  }
}

/**
 * Best-effort delete of `<dir>/<name>`. Swallows "file not found" so callers can use this
 * unconditionally on exit (the file may have been already cleaned up by Rust shutdown).
 *
 * @param {string} dir
 * @param {string} name
 */
export function removePortFile(dir, name) {
  try {
    unlinkSync(join(dir, name))
  } catch (err) {
    // ENOENT is fine: the file may have been removed by Rust shutdown or never written.
    if (err instanceof Error && /** @type {NodeJS.ErrnoException} */ (err).code !== 'ENOENT') {
      console.warn(`Could not remove port file ${join(dir, name)}: ${err.message}`)
    }
  }
}

// Re-export the platform default for callers that need to detect "no override".
export const PRODUCTION_IDENTIFIER = PROD_IDENTIFIER
