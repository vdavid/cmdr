/**
 * Issue a Cmdr license by hand: an evaluation for a prospect, a free seat for a partner company, a
 * thank-you for a testimonial or a bug report, or customer-service recovery.
 *
 * Run: node scripts/mint-license.js --email someone@example.com --note "why"
 *
 * The note is required and lands in the `license_issuance` ledger beside the key. That ledger is
 * what `/validate` answers from, so it's also what makes the license work at all.
 */

import { defaultApiUrl, describe, die, parseArgs, postAdmin, readAdminToken } from './admin-api.js'

const usage = `Issue a Cmdr license by hand.

Usage:
  node scripts/mint-license.js --email <address> --note <why> [options]

Required:
  --email <address>   Who the license is for
  --note <why>        Why it's being issued, for example "Test Customer, Acme, evaluation"

Options:
  --name <name>       How the email greets them (default: "there")
  --org <name>        Organization the license is issued to, shown in the app
  --expires <when>    2027-01-31, or a span: 30d, 6m, 1y. Omit for a perpetual license
  --send              Email the key to --email. Without it, the key is only printed here
  --api <url>         API base URL (default: ${defaultApiUrl})
  --dry-run           Print what would be minted, and stop
  --help              This text

Examples:
  node scripts/mint-license.js --email test_user_13@example.com --org Acme \\
    --note "Test Customer, Acme, evaluation" --expires 90d --send

  node scripts/mint-license.js --email test_user_1@example.com --note "found the SMB reconnect bug"
`

const args = parseArgs(process.argv.slice(2), {
  booleans: ['send', 'dry-run', 'help'],
  known: ['email', 'note', 'name', 'org', 'expires', 'send', 'api', 'dry-run', 'help'],
})

if (args.help) {
  console.log(usage)
  process.exit(0)
}

if (!args.email) die(`Who is this license for? Pass --email.\n\n${usage}`)
if (!args.note || !args.note.trim()) {
  die(`Every hand-issued license needs a note saying who it's for and why. Pass --note.\n\n${usage}`)
}

const request = {
  email: args.email,
  note: args.note.trim(),
  customerName: args.name,
  organizationName: args.org,
  expiresAt: resolveExpiry(args.expires),
  sendEmail: args.send === true,
}

const apiUrl = args.api ?? defaultApiUrl

if (args['dry-run']) {
  console.log(`Would mint against ${apiUrl}:`)
  console.log(JSON.stringify(request, null, 2))
  process.exit(0)
}

const minted = await postAdmin('/admin/generate', request, { apiUrl, token: readAdminToken() })

console.log(`\nLicense issued for ${args.email}:\n`)
console.log(describe(minted))
console.log(`  Note:        ${request.note}`)
console.log(
  minted.emailed
    ? `\nThe key is on its way to ${args.email}.`
    : `\nNothing was emailed. Send ${minted.code} over yourself, or re-run with --send.`,
)
console.log(`\nTo take it back later: node scripts/revoke-license.js --code ${minted.code}\n`)

/** `2027-01-31` (through the end of that day), or a span from now: `30d`, `6m`, `1y`. */
function resolveExpiry(value) {
  if (!value) return undefined

  const span = /^(\d+)([dmy])$/i.exec(value.trim())
  if (span) {
    const amount = Number(span[1])
    const date = new Date()
    if (span[2].toLowerCase() === 'd') date.setUTCDate(date.getUTCDate() + amount)
    if (span[2].toLowerCase() === 'm') date.setUTCMonth(date.getUTCMonth() + amount)
    if (span[2].toLowerCase() === 'y') date.setUTCFullYear(date.getUTCFullYear() + amount)
    return date.toISOString()
  }

  // A bare date means the whole of that day, which is what "valid until the 31st" means to a
  // person. `Date.parse` would otherwise put the expiry at midnight, a day early.
  const whole = /^\d{4}-\d{2}-\d{2}$/.test(value.trim()) ? `${value.trim()}T23:59:59.999Z` : value.trim()
  const parsed = Date.parse(whole)
  if (Number.isNaN(parsed)) die(`Can't read "${value}" as a date. Use 2027-01-31, or a span like 30d / 6m / 1y.`)
  return new Date(parsed).toISOString()
}
