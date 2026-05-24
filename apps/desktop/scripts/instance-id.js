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
 * @param {string|null} instanceId
 * @returns {string}
 */
export function productName(instanceId) {
  return instanceId ? `Cmdr (${instanceId})` : PROD_PRODUCT_NAME
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
 * }} InstanceConfig
 */

/**
 * Build the Tauri config payload that the wrapper writes to disk and passes via -c.
 *
 * For prod (instanceId null), returns null: the wrapper omits -c entirely so canonical
 * tauri.conf.json governs the build.
 *
 * @param {string|null} instanceId
 * @returns {InstanceConfig|null}
 */
export function buildInstanceConfig(instanceId) {
  if (!instanceId) return null
  return {
    $schema: 'https://schema.tauri.app/config/2',
    productName: productName(instanceId),
    identifier: bundleIdentifier(instanceId),
    app: {
      withGlobalTauri: true,
    },
    plugins: {
      updater: {
        // Dead URL so non-prod instances never phone home accidentally. P4 will replace
        // this with a real per-instance stub when the Vite dev port also lands here.
        endpoints: ['https://localhost.invalid/no-updater'],
      },
    },
  }
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
 * @returns {{ identifier: string, dataDir: string, config: InstanceConfig|null }}
 */
export function deriveInstance({ instanceId, platform, home, xdgDataHome }) {
  const identifier = bundleIdentifier(instanceId)
  const dataDir = computeAppDataDir({ identifier, platform, home, xdgDataHome })
  const config = buildInstanceConfig(instanceId)
  return { identifier, dataDir, config }
}

// Re-export the platform default for callers that need to detect "no override".
export const PRODUCTION_IDENTIFIER = PROD_IDENTIFIER
