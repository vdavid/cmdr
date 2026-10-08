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
export const verifiedAgainst = { version: '0.50.0', date: '2026-10-05' }

/**
 * The app's designated requirement, exactly as `codesign -d -r-` prints it for a release build.
 * `/trust` shows it, and `public/mdm/cmdr-full-disk-access.mobileconfig` grants Full Disk Access to
 * it. A Rust test (`managed_policy/pppc_profile_test.rs`) checks both against the bundle id and the
 * Team ID in `tauri.conf.json`, so a signing change can't silently break the profile.
 */
export const codeRequirement =
  'identifier "com.veszelovszki.cmdr" and anchor apple generic and certificate 1[field.1.2.840.113635.100.6.2.6] /* exists */ and certificate leaf[field.1.2.840.113635.100.6.1.13] /* exists */ and certificate leaf[subject.OU] = "83H6YAQMNP"'

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
      '<strong>On by default.</strong> Turn off with Settings &gt; Updates &amp; privacy &gt; "Automatically check for updates". A found update downloads and installs by itself, then asks the user to restart. IT can turn off background checks, all updates, or updates past a version (<a href="#mdm">central management</a>).',
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
      '<strong>On by default</strong> during the open beta. The first-launch setup shows this, and the user can\'t skip that step. Turn off with Settings &gt; Updates &amp; privacy &gt; "Send usage stats". When it\'s off, nothing is sent. IT can turn it off for everyone with <code>DisableUsageStats</code>.',
  },
  {
    id: 'crash-reports',
    name: 'Crash reports',
    destination: '<code>api.getcmdr.com/crash-report</code>',
    when: 'On the next launch after a crash.',
    sends:
      "App and macOS version, where in Cmdr's code the crash happened, the crash message after it's cleaned of personal data on the Mac (at most 2,000 characters), memory addresses of the crashing code, and a random report id. From macOS's own crash report, only the one-line reason and the function names of the crashing thread. An email address only if the user ticks a box.",
    control:
      '<strong>On by default.</strong> Turn off with Settings &gt; Updates &amp; privacy &gt; "Send crash reports". IT can turn it off for everyone with <code>DisableCrashAndErrorReports</code>.',
  },
  {
    id: 'error-reports',
    name: 'Error reports',
    destination: '<code>api.getcmdr.com/error-report</code>',
    when: 'When the user picks Help &gt; Send error report, which shows a preview first. Automatic sending exists but is off by default.',
    sends:
      "A zip with the recent part of the app's log, the app and macOS version, and the user's note. Before it leaves the Mac, Cmdr replaces file and folder names in every path with placeholders, keeping only the extension and common folder names like Documents. A name repeated on the same log line as its path is replaced too. <strong>Known gap</strong>: a name logged on its own, with no path on its line, can get through.",
    control:
      'Sent by hand only, by default. Automatic sending is Settings &gt; Updates &amp; privacy &gt; "Send error reports automatically", off by default. IT can turn off both kinds for everyone with <code>DisableCrashAndErrorReports</code>.',
    devTodo:
      'Release timing: AI search and selection words stopped reaching the log in 3bfa0e246, and the redactor started covering paths under any prefix and a name repeated next to its path on 2026-10-06. Both ship in the release after 0.50.0, so this wording is only true from then. Remaining: a name logged with no path on its line (<code>redact/DETAILS.md</code> § "Names in prose"). If it ever closes, drop the "Known gap" sentence here, the matching gap in "Not in place yet", and the gap sentences in the privacy policy (section 2, the paragraph after the list, and the intro).',
  },
  {
    id: 'license',
    name: 'License check',
    destination: '<code>api.getcmdr.com</code> (older versions use <code>license.getcmdr.com</code>)',
    when: 'Only when a commercial license is installed: at activation, then once every seven days.',
    sends:
      "The license's transaction id, and a device id that's a one-way hash (SHA-256) of the Mac's hardware UUID. Activation sends the short license code.",
    control:
      "Free personal-use installs never make this call. The license itself is checked offline with a signature, and the check only learns whether a license was revoked, expired, or renewed. The server signs its answer, so nobody else can revoke your license. If the server can't be reached, nothing changes: a perpetual license keeps working forever, and a time-limited one runs to its end date. No managed preference turns it off, since a paid license has to stay checkable.",
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
    control:
      '<strong>Off by default.</strong> Settings &gt; AI &gt; Provider. IT can turn AI off, allow only on-device AI, or limit which hosts cloud AI may reach (<a href="#mdm">central management</a>).',
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
    destination:
      'Only servers and devices the user connects to: SMB, SFTP, and WebDAV servers, S3-compatible storage, and phones over USB.',
    when: 'When the user browses them. To find SMB servers on the local network, Cmdr uses Bonjour (mDNS). It runs only while the Servers view is open, while Cmdr looks up the server behind an SMB share it connects to, and for 10 seconds after launch.',
    sends: 'What the protocol needs: credentials the user entered and the file operations the user asked for.',
    control:
      'SMB, phone (MTP), and Android (ADB) support are on by default and each can be turned off in Settings &gt; File systems. Git support is local only and never fetches or pushes.',
  },
  {
    id: 's3-prices',
    name: 'S3 price table',
    destination: '<code>api.getcmdr.com/s3-prices/v1</code>',
    when: 'Only for users of S3 storage: when Cmdr estimates what an S3 copy, move, delete, or rename costs, and its stored price table is missing or over a day old. At most once a day.',
    sends: 'Nothing beyond the request itself. The server stores nothing about it.',
    control:
      'Happens only when the user works with S3. Without it, Cmdr uses the last stored table or the one built into the app. No managed preference turns it off, since it carries no user data.',
  },
]

export interface ManagedPreferenceKey {
  /** The key name in the `com.veszelovszki.cmdr` preference domain. */
  key: string
  type: 'Boolean' | 'String' | 'Array of strings'
  /** Inline HTML. */
  effect: string
}

/**
 * Every managed preference (MDM) key the app reads, in the order of the sample profile. A Rust
 * test (`managed_policy/public_docs_test.rs`) checks this list and both files in `public/mdm/`
 * against the key names in `managed_policy/keys.rs`, so a new key can't skip this page. The
 * canonical catalog is `apps/desktop/src-tauri/src/managed_policy/DETAILS.md`.
 */
export const managedPreferenceKeys: ManagedPreferenceKey[] = [
  {
    key: 'DisableUsageStats',
    type: 'Boolean',
    effect:
      'No usage stats. Cmdr sends no heartbeat and no feature events, and deletes the ones waiting on the Mac. "Send usage stats" stays off.',
  },
  {
    key: 'DisableCrashAndErrorReports',
    type: 'Boolean',
    effect:
      "No crash reports and no error reports, automatic or sent by hand. A crash report waiting from the last session is deleted without being offered. Users can still save an error report to disk. In-app feedback isn't covered: it's a message the user writes and sends on purpose.",
  },
  {
    key: 'DisableAutomaticUpdateChecks',
    type: 'Boolean',
    effect: 'No background update checks. A user can still check by hand.',
  },
  {
    key: 'DisableUpdates',
    type: 'Boolean',
    effect:
      'No update check, download, or install of any kind, so Cmdr never contacts the update servers. For teams that ship new versions themselves. Overrides the other two update keys.',
  },
  {
    key: 'MaxUpdateVersion',
    type: 'String',
    effect:
      "\"Never update past this version.\" <code>0.52</code> allows up to the last 0.52.x, <code>0.52.3</code> up to and including 0.52.3, and <code>1</code> anything below 2.0.0. A whole number works too. A value Cmdr can't read turns updates off. <strong>What it can't do</strong>: Cmdr only learns about its newest release, so once a release past the ceiling ships, the Mac gets no more updates, patches included, until you raise it. It holds Cmdr at a version; it isn't an update channel.",
  },
  {
    key: 'DisableAI',
    type: 'Boolean',
    effect:
      "No AI at all: no request to any AI provider, no on-device model download, no local AI server, and no Ask Cmdr. AI search over the MCP server refuses too. Image search isn't covered: its on-device model indexes images and doesn't generate anything.",
  },
  {
    key: 'DisableCloudAI',
    type: 'Boolean',
    effect:
      "On-device AI only: nothing goes to a cloud AI provider. An Ollama or LM Studio server counts as cloud, even on the same Mac, because <code>localhost</code> can be a tunnel to another machine. Cmdr's own on-device model still works.",
  },
  {
    key: 'AllowedCloudAIHosts',
    type: 'Array of strings',
    effect:
      "Cloud AI may reach only these hosts. Each entry is a host (<code>api.openai.com</code>), a host and port (<code>localhost:11434</code>), a pattern for any subdomain (<code>*.openai.azure.com</code>, which doesn't match <code>openai.azure.com</code> itself), or a pasted URL, of which only the host and port count. Case doesn't matter. Cmdr checks every request and every redirect. It skips entries it can't read, and an empty list allows no host, the same as <code>DisableCloudAI</code>.",
  },
]

/** Hosts to allow on a proxy or firewall, with whether Cmdr needs them to work. */
export const allowlistHosts: { host: string; purpose: string }[] = [
  { host: 'api.getcmdr.com', purpose: 'update checks, license checks, usage stats, reports, S3 prices' },
  { host: 'getcmdr.com', purpose: 'the update manifest (latest.json)' },
  { host: 'github.com, release-assets.githubusercontent.com', purpose: 'update downloads' },
  { host: 'license.getcmdr.com', purpose: 'license checks from older versions only' },
  {
    host: 'huggingface.co, *.hf.co',
    purpose: 'optional, only for local AI and image-search model downloads (hf.co serves the files)',
  },
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
    what: 'The website, self-hosted page analytics (Umami), the newsletter list (Listmonk), blog comments (Remark42), and the self-hosted mail server (mailcow). Every email sent to an @getcmdr.com address, including security@, lands there, and so do the notification emails below.',
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
    what: 'A private notification channel for new error reports, feedback, and signups. Never includes an email address. Can include a note the user wrote and a download link to an error report that expires after 24 hours.',
    where: 'United States.',
    inEu: 'no',
  },
  {
    name: 'Resend',
    what: 'Sends license emails (with the license key), newsletter and confirmation emails, and internal notification emails about reports and feedback.',
    where: "US company. It sends Cmdr's email from its Ireland region (eu-west-1).",
    inEu: 'partly',
  },
  {
    name: 'SMTP2Go',
    what: 'Relays the emails we send from @getcmdr.com addresses, like replies to support and security emails, so it sees who they go to and what they say. Mail we receive never passes through it.',
    where:
      'New Zealand company, which the EU recognizes as giving adequate data protection. SMTP2Go says it stores data of EEA customers only in the EEA.',
    inEu: 'partly',
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
    rule: 'Email, notes, and the issue-tracker comment removed after 90 days. The report zip itself is deleted after 90 days by a storage rule on its bucket.',
  },
  { what: 'Feedback', rule: 'Email removed after two years. The message is kept.' },
  { what: 'Issued licenses', rule: 'Buyer email and license record kept with no time limit.' },
  {
    what: 'App feature events (PostHog)',
    rule: "PostHog's free plan keeps them for at least one year and sets no end date. PostHog doesn't offer a shorter setting.",
  },
  { what: 'Website visit recordings (PostHog)', rule: 'Deleted after 30 days.' },
  { what: 'Website visits (Umami)', rule: 'Deleted after two years.' },
  { what: 'Purchase records at Paddle', rule: 'Seven years, as Swedish accounting law requires. Paddle keeps these.' },
]

/**
 * Honest gaps a strict review would find anyway. Inline HTML. Keep each item one or two short
 * sentences; the reviewer scans this list.
 */
export const notInPlaceYet: string[] = [
  '<strong>Usage stats and crash reports are on by default.</strong> Each user can turn them off, and IT can turn them off for everyone (<a href="#mdm">central management</a>).',
  "<strong>No update channels and no staged rollout.</strong> IT can turn updates off or hold Cmdr at a version, but can't install an older release through the updater.",
  '<strong>No managed preference turns off the remaining traffic</strong>: license checks, S3 prices, and the image-search model download (<a href="#mdm-not-covered">why each one stays</a>).',
  '<strong>No <code>.pkg</code> installer yet.</strong> The release pipeline is ready to build a signed one and waits on its signing certificate (<a href="#mdm-deploy">deploying Cmdr</a>).',
  "<strong>The Full Disk Access profile hasn't been tested on a real MDM yet.</strong> Its code requirement is checked against the released app.",
  "<strong>Data leaves the EU</strong> (see above), and Cloudflare storage isn't locked to the EU jurisdiction.",
  '<strong>No data processing agreement (DPA)</strong> ready to sign.',
  '<strong>Error-report cleaning has a known gap</strong>: a file name logged on its own, with no path on the same line, can be included.',
  '<strong>No reproducible builds.</strong> Each release publishes SHA-256 checksums, signed build provenance, and signed SBOMs, and its tag is signed.',
  '<strong>No second-person code review.</strong> Cmdr has one maintainer, and development is AI-assisted. Automated checks stand in for a reviewer (<a href="/trust/development#review">details</a>).',
  '<strong>One maintainer account can publish a release</strong> to every install, and the signing keys are GitHub repository secrets without a protected environment.',
  "<strong>Checks on GitHub run after a change lands</strong>, on Linux. A release waits for a full, green run of the commit it's cut from, checked by both the maintainer's release script and the release workflow on GitHub. The macOS-only code and the macOS end-to-end tests run only on the maintainer's Mac.",
  '<strong>No automated tests on older macOS versions.</strong> A release-time check catches system frameworks and functions that are too new for them.',
  '<strong>No third-party audit or penetration test, and no SOC 2 or ISO 27001.</strong>',
  "<strong>One person maintains Cmdr</strong> and holds all signing keys. There's no continuity clause in the terms and no written support commitment.",
  "<strong>Local data isn't encrypted by Cmdr</strong>, so it relies on FileVault. There's no option to exclude Cmdr's index from backups.",
  "<strong>No sign-in for proxies</strong> beyond a user name and password in the proxy's address (<code>HTTPS_PROXY</code> or <code>ALL_PROXY</code>).",
  "<strong>No published security advisories yet</strong>, so there's no track record of how Cmdr handles a reported issue.",
]

/** Response times for vulnerability reports. Keep in sync with the repo-root `SECURITY.md`. */
export const vulnerabilityResponse: string[] = [
  'Acknowledgment within five business days.',
  'A first assessment within 14 days.',
  'A fix for critical and high-severity issues within 30 days where technically possible, and within 90 days for the rest.',
  'Status updates at least every 14 days until the issue is closed.',
]
