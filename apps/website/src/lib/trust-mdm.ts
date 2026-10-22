/**
 * The `/trust` content that is a contract with the app binary rather than page copy: the code
 * requirement the Full Disk Access profile grants, and every managed preference (MDM) key the app
 * reads. Two Rust tests embed this file (`managed_policy/public_docs_test.rs` and
 * `pppc_profile_test.rs`), so CI's `rust` filter and the Rust check lanes list it as an input. Page
 * copy goes in `trust.ts`, so editing prose there doesn't run the Rust suite.
 *
 * Same truth rule as `trust.ts`: every claim must hold for the released app.
 */

/**
 * The app's designated requirement, exactly as `codesign -d -r-` prints it for a release build.
 * `/trust` shows it, and `public/mdm/cmdr-full-disk-access.mobileconfig` grants Full Disk Access to
 * it. A Rust test (`managed_policy/pppc_profile_test.rs`) checks both against the bundle id and the
 * Team ID in `tauri.conf.json`, so a signing change can't silently break the profile.
 */
export const codeRequirement =
  'identifier "com.veszelovszki.cmdr" and anchor apple generic and certificate 1[field.1.2.840.113635.100.6.2.6] /* exists */ and certificate leaf[field.1.2.840.113635.100.6.1.13] /* exists */ and certificate leaf[subject.OU] = "83H6YAQMNP"'

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
  {
    key: 'DisableSecurityLogOptOut',
    type: 'Boolean',
    effect:
      'The security event log stays on, and users can\'t turn it off. It\'s a small local file of security-relevant changes and refusals (a policy taking effect, an update installed or refused, the MCP server switching on, a rejected access attempt). It never leaves the Mac, and "Delete all Cmdr data" still removes it.',
  },
]
