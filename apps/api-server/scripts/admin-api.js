/**
 * Shared plumbing for the admin CLI scripts: the token, the request, and argument parsing.
 *
 * The token is `ADMIN_API_TOKEN`, the same wrangler secret every `/admin/*` route takes. It's read
 * from sops (`secret CMDR_ADMIN_API_TOKEN`) when that entry exists, and from the environment
 * otherwise, so nobody has to paste a credential into a shell command.
 */

import { execFileSync } from 'node:child_process'

export const defaultApiUrl = 'https://api.getcmdr.com'

const sopsName = 'CMDR_ADMIN_API_TOKEN'

/** The admin token, or an exit with instructions. */
export function readAdminToken() {
  const fromEnv = process.env[sopsName]
  if (fromEnv) return fromEnv.trim()

  try {
    const fromSops = execFileSync('secret', [sopsName], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] })
    if (fromSops.trim()) return fromSops.trim()
  } catch {
    // `secret` isn't on PATH, or has no such entry. The message below covers both.
  }

  die(
    `No admin token. Add it to sops as ${sopsName} (\`secret edit\`, the value is the Worker's\n` +
      `ADMIN_API_TOKEN), or pass it for this run:\n\n  ${sopsName}=… node scripts/<this script>`,
  )
}

/** POST to an admin route and return the parsed body. Exits with the server's message on failure. */
export async function postAdmin(path, body, options) {
  const url = `${options.apiUrl}${path}`
  const response = await fetch(url, {
    method: 'POST',
    headers: { Authorization: `Bearer ${options.token}`, 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })

  const text = await response.text()
  let parsed
  try {
    parsed = JSON.parse(text)
  } catch {
    die(`${url} answered ${response.status} with something that isn't JSON:\n${text}`)
  }

  if (!response.ok) {
    const reason = parsed.error ?? text
    // A mint whose only failure was delivery still hands back a usable code, so print it.
    if (parsed.code) {
      die(
        `${reason} (HTTP ${response.status})\n\nThe license itself exists, so pass it on by hand:\n${describe(parsed)}`,
      )
    }
    die(`${reason} (HTTP ${response.status})`)
  }

  return parsed
}

/** One-line summary of a minted license, for the failure path and the success path alike. */
export function describe(minted) {
  const lines = [
    `  Code:        ${minted.code}`,
    `  Transaction: ${minted.transactionId}`,
    `  Type:        ${minted.type}`,
  ]
  if (minted.organizationName) lines.push(`  Licensed to: ${minted.organizationName}`)
  lines.push(`  Expires:     ${minted.expiresAt ? minted.expiresAt.slice(0, 10) : 'never'}`)
  // The signed key activates without our server; hand it over with the code when replying by hand.
  if (minted.fullKey) lines.push(`  Offline key: ${minted.fullKey}`)
  return lines.join('\n')
}

/**
 * Parse `--flag value` and `--flag` pairs. Flags listed in `booleans` take no value; everything
 * else does. An unknown flag is an exit, so a typo can't silently drop an argument.
 */
export function parseArgs(argv, { booleans = [], known }) {
  const args = {}
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i]
    if (!arg.startsWith('--')) die(`Unexpected argument: ${arg}`)
    const name = arg.slice(2)
    if (!known.includes(name)) die(`Unknown flag: ${arg}`)
    if (booleans.includes(name)) {
      args[name] = true
      continue
    }
    const value = argv[++i]
    if (value === undefined || value.startsWith('--')) die(`${arg} needs a value`)
    args[name] = value
  }
  return args
}

export function die(message) {
  console.error(message)
  process.exit(1)
}
