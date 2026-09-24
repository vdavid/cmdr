/**
 * The `/trust` page's list content: every network connection the app makes, where backend data
 * lives, the subprocessors, retention, and the "Not in place yet" gaps. `src/pages/trust.astro` is
 * a template over this and owns the layout plus the prose sections.
 *
 * ❗ Every claim here must be true of the RELEASED app on the date in `verifiedAgainst`. The audience
 * is a security reviewer who will check it against the source, so an overstatement costs more than
 * a gap. The evidence behind each line is David's fact sheet
 * (`projects/Cmdr/business/2026-09-23 Cmdr security posture fact sheet.md` in his vault) and
 * `docs/security.md`. When a gap closes, move it out of `notInPlaceYet` and update the section.
 *
 * Strings marked "Inline HTML" render through `set:html` (for `<code>`, `<strong>`, and links).
 * `devTodo` fields are for David only: they render through `DevTodo.astro`, which prints nothing
 * in a production build. Write straight quotes; `smart-quotes.ts` curls them in the built HTML.
 */

/** The release and date the page's claims were checked against. */
export const verifiedAgainst = { version: '0.46.1', date: '2026-09-23' }

export interface NetworkConnection {
  id: string
  name: string
  /** Inline HTML. Hosts the app contacts. */
  destination: string
  /** Inline HTML. */
  when: string
  /** Inline HTML. */
  sends: string
  /** Inline HTML. Default state plus how a user turns it off. */
  control: string
  /** Inline HTML. Shown to David in dev only. */
  devTodo?: string
}

/** Every outbound connection of a release build, in the order a reviewer asks about them. */
export const networkConnections: NetworkConnection[] = [
  {
    id: 'updates',
    name: 'Update check and download',
    destination:
      '<code>api.getcmdr.com</code>, then <code>getcmdr.com/latest.json</code>. The update itself downloads from <code>github.com</code> and <code>release-assets.githubusercontent.com</code>.',
    when: 'At most once every three hours, starting at launch. Users can set the interval from five minutes to 24 hours.',
    sends:
      'The app version and CPU architecture, in the URL. The server keeps a one-way hash of the IP address, the date, the version, and the architecture. It deletes each record after seven days and keeps only daily totals.',
    control:
      '<strong>On by default.</strong> Turn off with Settings &gt; Updates &amp; privacy &gt; "Automatically check for updates". A found update downloads and installs by itself, then asks the user to restart.',
  },
  {
    id: 'usage-stats',
    name: 'Usage stats',
    destination:
      "<code>api.getcmdr.com/heartbeat</code>. The app doesn't contact PostHog: our server passes the feature events on to PostHog's EU cloud.",
    when: 'At most once every three hours while the app is open. Feature events wait on the Mac and go with the next send.',
    sends:
      'A random install id created on the Mac (not linked to a name, email, or license), app version, macOS version, CPU architecture, the names of features used, and a fixed list of settings values (like light or dark mode). No file names, paths, file contents, search terms, or AI prompts. That list is enforced by code review, not by an automatic filter.',
    control:
      '<strong>On by default</strong> during the open beta. The first-launch setup shows this, and the user can\'t skip that step. Turn off with Settings &gt; Updates &amp; privacy &gt; "Send usage stats". When it\'s off, nothing is sent.',
  },
  {
    id: 'crash-reports',
    name: 'Crash reports',
    destination: '<code>api.getcmdr.com/crash-report</code>',
    when: 'On the next launch after a crash.',
    sends:
      "App and macOS version, where in Cmdr's code the crash happened, the crash message after it's cleaned of personal data on the Mac (at most 2,000 characters), memory addresses of the crashing code, and a random report id. From macOS's own crash report, only the one-line reason and the function names of the crashing thread. An email address only if the user ticks a box.",
    control:
      '<strong>On by default.</strong> Turn off with Settings &gt; Updates &amp; privacy &gt; "Send crash reports".',
  },
  {
    id: 'error-reports',
    name: 'Error reports',
    destination: '<code>api.getcmdr.com/error-report</code>',
    when: 'When the user picks Help &gt; Send error report, which shows a preview first. Automatic sending exists but is off by default.',
    sends:
      "A zip with the recent part of the app's log, the app and macOS version, and the user's note. Before it leaves the Mac, Cmdr replaces file and folder names in paths with placeholders, keeping only the extension and common folder names like Documents. <strong>Known gaps</strong>: a file name that appears in free text (outside a path) can get through, and the text of a natural-language search is in the log and can be included.",
    control:
      'Sent by hand only, by default. Automatic sending is Settings &gt; Updates &amp; privacy &gt; "Send error reports automatically", off by default.',
    devTodo:
      'Close the two gaps in the code: the redactor misses a file name in free text (<code>redact/CLAUDE.md</code>), and <code>commands/search.rs</code> logs <code>query=</code> at debug, which always reaches the file. Then drop the "Known gaps" sentence here, the matching gap in "Not in place yet", and the gap sentences in the privacy policy (section 2 and the intro).',
  },
  {
    id: 'license',
    name: 'License check',
    destination: '<code>api.getcmdr.com</code> (older versions use <code>license.getcmdr.com</code>)',
    when: 'Only when a commercial license is installed: at activation, then once every seven days.',
    sends:
      "The license's transaction id, and a device id that's a one-way hash (SHA-256) of the Mac's hardware UUID. Activation sends the short license code.",
    control:
      "Free personal-use installs never make this call. The license itself is checked offline with a signature. If the server can't be reached, the license keeps working for 30 days, then the app falls back to the free personal tier until a check succeeds.",
  },
  {
    id: 'feedback',
    name: 'Feedback and newsletter signup',
    destination: '<code>api.getcmdr.com/feedback</code> and <code>api.getcmdr.com/beta-signup</code>',
    when: 'Only when the user sends feedback or types an email address to stay in touch.',
    sends: 'The message, app and macOS version, and an email address if the user adds one. No install id.',
    control: 'Nothing is sent unless the user sends it.',
  },
  {
    id: 'ai',
    name: 'AI features (optional)',
    destination:
      'The AI provider the user picks, straight from the Mac. None of it goes through Cmdr\'s servers. Details in <a href="#ai">AI features and your files</a>.',
    when: 'Only after the user sets up a cloud provider with their own API key and turns on "Allow cloud AI".',
    sends: 'File and folder names and, when asked, parts of file contents. See the AI section.',
    control: '<strong>Off by default.</strong> Settings &gt; AI &gt; Provider.',
  },
  {
    id: 'models',
    name: 'Local AI model and image-search model downloads (optional)',
    destination: '<code>huggingface.co</code> and its download servers',
    when: 'Only when the user chooses local AI, or turns on semantic image search.',
    sends:
      'A normal file download. The image-search model is checked against a fixed SHA-256 hash. The local AI model is checked by file size only.',
    control: 'Nothing is downloaded unless the user asks for it.',
  },
  {
    id: 'file-access',
    name: 'Remote files the user opens',
    destination: 'Only servers and devices the user connects to: SMB, SFTP, and WebDAV servers, and phones over USB.',
    when: 'When the user browses them. To find SMB servers on the local network, Cmdr uses Bonjour (mDNS), which starts only once the user first uses a network feature. After that it runs at every launch while SMB support is on.',
    sends: 'What the protocol needs: credentials the user entered and the file operations the user asked for.',
    control:
      'SMB, phone (MTP), and Android (ADB) support are on by default and each can be turned off in Settings &gt; File systems. Git support is local only and never fetches or pushes.',
  },
]

/** Hosts to allow on a proxy or firewall, with whether Cmdr needs them to work. */
export const allowlistHosts: { host: string; purpose: string }[] = [
  { host: 'api.getcmdr.com', purpose: 'update checks, license checks, usage stats, reports' },
  { host: 'getcmdr.com', purpose: 'the update manifest (latest.json)' },
  { host: 'github.com, release-assets.githubusercontent.com', purpose: 'update downloads' },
  { host: 'license.getcmdr.com', purpose: 'license checks from older versions only' },
  { host: 'huggingface.co', purpose: 'optional, only for local AI and image-search model downloads' },
  { host: 'your AI provider', purpose: 'optional, only if a user sets up cloud AI' },
]

export interface DataLocation {
  name: string
  /** Inline HTML. */
  what: string
  where: string
  inEu: 'yes' | 'no' | 'partly'
}

/** Where data that reaches Cmdr's backend or its vendors ends up. */
export const dataLocations: DataLocation[] = [
  {
    name: 'Cloudflare (Workers, D1, R2, KV)',
    what: 'The API server. Its database holds download records, update checks, usage stats, crash reports, feedback, and the list of issued licenses. Its file storage holds error-report zips.',
    where:
      "The database and file storage are placed in Eastern Europe, but not locked to the EU by contract. The API code and the KV key-value store run on Cloudflare's global network.",
    inEu: 'partly',
  },
  {
    name: 'PostHog',
    what: 'Usage stats from the app, which our API server passes on, and visit recordings and heatmaps from the website.',
    where: 'PostHog EU cloud (Frankfurt, according to PostHog).',
    inEu: 'yes',
  },
  {
    name: 'Hetzner',
    what: 'The website, self-hosted page analytics (Umami), the newsletter list (Listmonk), and blog comments (Remark42).',
    where: 'Helsinki, Finland.',
    inEu: 'yes',
  },
  {
    name: 'GitHub',
    what: "Source code, release downloads and updates (GitHub sees the IP address), and a private issue tracker where error reports and feedback become issues. The email address and message sit in a separate comment that's deleted on a schedule.",
    where: 'United States.',
    inEu: 'no',
  },
  {
    name: 'Discord',
    what: 'A private notification channel for new crash reports, error reports, feedback, and signups. Includes an email address if the user attached one, and a download link to an error report that expires after 24 hours.',
    where: 'United States.',
    inEu: 'no',
  },
  {
    name: 'Resend',
    what: 'Sends license emails (with the license key) and internal notification emails about reports and feedback.',
    where: "US company. It sends Cmdr's email from its Ireland region (eu-west-1).",
    inEu: 'partly',
  },
  {
    name: 'Google (Gmail)',
    what: 'Every email sent to an @getcmdr.com address, including security@, lands in a Gmail inbox. So do the notification emails above.',
    where: 'United States.',
    inEu: 'no',
  },
  {
    name: 'Amazon Web Services (SES)',
    what: 'Sends newsletter emails.',
    where: 'Stockholm, Sweden (eu-north-1).',
    inEu: 'yes',
  },
  {
    name: 'Paddle',
    what: "Payments, as merchant of record. Paddle keeps the buyer's payment details; Cmdr never sees card numbers.",
    where: 'United Kingdom.',
    inEu: 'no',
  },
]

export interface RetentionRule {
  what: string
  /** Inline HTML. */
  rule: string
}

/** Server-side retention. A daily job enforces the D1 rows; the rest say what enforces them. */
export const serverRetention: RetentionRule[] = [
  { what: 'Update checks', rule: 'Deleted after seven days. Daily totals per version are kept.' },
  { what: 'Usage stats (heartbeats and feature events)', rule: 'Deleted after two years.' },
  {
    what: 'Download records',
    rule: 'IP hash and browser user agent removed after 90 days. Version, architecture, country, and referrer are kept.',
  },
  {
    what: 'Crash reports',
    rule: 'Email and report id removed after 90 days. The technical part (versions, crash location) is kept with no time limit.',
  },
  {
    what: 'Error reports',
    rule: "Email, notes, and the issue-tracker comment removed after 90 days. <strong>The 90-day deletion of the report zip itself isn't active yet</strong>, so older zips currently exist.",
  },
  { what: 'Feedback', rule: 'Email removed after two years. The message is kept.' },
  { what: 'Issued licenses', rule: 'Buyer email and license record kept with no time limit.' },
  {
    what: 'App feature events (PostHog)',
    rule: "PostHog's free plan keeps them for at least one year and sets no end date. PostHog doesn't offer a shorter setting.",
  },
  { what: 'Website visit recordings (PostHog)', rule: 'Deleted after 30 days.' },
  { what: 'Purchase records at Paddle', rule: 'Seven years, as Swedish accounting law requires. Paddle keeps these.' },
]

/**
 * Honest gaps a strict review would find anyway. Inline HTML. Keep each item one or two short
 * sentences; the reviewer scans this list.
 */
export const notInPlaceYet: string[] = [
  '<strong>No central administration.</strong> No MDM configuration profile or managed preferences. Every setting (usage stats, crash reports, updates, AI) is per user, and the user can change it back.',
  '<strong>Usage stats and crash reports are on by default.</strong> Each user can turn them off.',
  '<strong>No update control for IT.</strong> Updates install automatically. There are no update channels, no staged rollout, and no way to pin a version centrally.',
  '<strong>No <code>.pkg</code> installer and no published PPPC profile</strong> for granting Full Disk Access through MDM. The code-signing requirement on this page is what such a profile needs.',
  "<strong>Data leaves the EU</strong> (see above), and Cloudflare storage isn't locked to the EU jurisdiction.",
  '<strong>No data processing agreement (DPA)</strong> ready to sign.',
  "<strong>The 90-day deletion of error-report zips isn't active yet.</strong>",
  '<strong>Error-report cleaning has known gaps</strong>: a file name in free text, or a search query, can be included.',
  "<strong>No central control over AI.</strong> IT can't disable AI or limit which providers users can pick.",
  "<strong>No reproducible builds</strong>, and release tags aren't signed. Each release publishes SHA-256 checksums. Signed build provenance and SBOMs start with the next release, so no release has them yet.",
  '<strong>No second-person code review.</strong> Cmdr has one maintainer, and development is AI-assisted. Automated checks stand in for a reviewer (<a href="/trust/development#review">details</a>).',
  '<strong>One maintainer account can publish a release</strong> to every install, and the signing keys are GitHub repository secrets without a protected environment.',
  '<strong>No threat model for the app as a whole.</strong> Security decisions are written down per part of the app.',
  "<strong>Checks on GitHub run after a change lands</strong>, on Linux. A release waits for a full, green run of the commit it's cut from, but the maintainer's release script enforces that, not GitHub. The macOS-only code and the macOS end-to-end tests run only on the maintainer's Mac.",
  '<strong>No automated tests on older macOS versions.</strong> A release-time check catches system frameworks and functions that are too new for them.',
  '<strong>No third-party audit or penetration test, and no SOC 2 or ISO 27001.</strong>',
  '<strong>No fuzzing</strong>, although Cmdr parses untrusted input (network protocols, archives, PDFs, images).',
  "<strong>One person maintains Cmdr</strong> and holds all signing keys. There's no continuity clause in the terms and no written support commitment.",
  "<strong>No offline license file.</strong> A commercial license that can't reach <code>api.getcmdr.com</code> for 30 days falls back to the free personal tier.",
  "<strong>Local data isn't encrypted by Cmdr</strong>, so it relies on FileVault. There's no option to exclude Cmdr's index from backups.",
  '<strong>Not tested behind a TLS-inspecting proxy</strong>, and PAC files are untested.',
  "<strong>No published security advisories yet</strong>, so there's no track record of how Cmdr handles a reported issue.",
]

/** Response times for vulnerability reports. Keep in sync with the repo-root `SECURITY.md`. */
export const vulnerabilityResponse: string[] = [
  'Acknowledgment within five business days.',
  'A first assessment within 14 days.',
  'A fix for critical and high-severity issues within 30 days where technically possible, and within 90 days for the rest.',
  'Status updates at least every 14 days until the issue is closed.',
]
