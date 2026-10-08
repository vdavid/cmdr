# Managed policy: details

The canonical key catalog for Cmdr's MDM support. `/trust` and the sample profile mirror this file; the plan behind it
is `docs/specs/mdm-managed-preferences-plan.md`.

## The public mirror

- `apps/website/public/mdm/cmdr-managed-preferences.mobileconfig` (a whole unsigned profile, one payload of type
  `com.veszelovszki.cmdr`) and `apps/website/public/mdm/com.veszelovszki.cmdr.plist` (the bare key dictionary, for MDM
  "preference file" uploads), served at `getcmdr.com/mdm/` with their own types (`apps/website/nginx.conf`).
- `/trust#mdm` renders its key list from `managedPreferenceKeys` in `apps/website/src/lib/trust.ts`.
- **Drift guard** (`public_docs_test.rs`): all three key sets equal `ALL_KEYS`, every example value parses without a
  warning, and the two files carry the same values. A new key fails it until the files and `/trust` list it. The Rust
  lanes and CI's `rust` filter list the three files as inputs, so editing only them isn't a cached pass.
- Example values: the telemetry keys and background checks on, the rest at their permissive value (`false`,
  `MaxUpdateVersion` `"1"`, a short host list), so an admin who uploads the file unchanged gets "telemetry off" and
  nothing surprising.
- `apps/website/public/mdm/cmdr-full-disk-access.mobileconfig`: a PPPC profile (`com.apple.TCC.configuration-profile-policy`)
  granting Full Disk Access plus the five narrower file services the app's `Info.plist` asks for. It's not a managed
  preference and Cmdr never reads it; it lives here because it's the other MDM file admins download. **Guard**
  (`pppc_profile_test.rs`): every grant names `config::BUNDLE_ID` and a code requirement built from `tauri.conf.json`
  (bundle id plus the Team ID in `signingIdentity`), `codeRequirement` in `trust.ts` equals it too, entries use
  `Allowed` (`Authorization` needs macOS 11; the floor is 10.15), and neither profile reuses the other's identifiers or
  UUIDs. Which TCC services it carries and why: the comment at the top of the file and `/trust#mdm-deploy`. Not yet
  tested on an MDM-enrolled Mac (only the requirement, against the shipped 0.50.0 with `codesign -v -R=`, 2026-10-07);
  that's the open "test it" item on #118. PPPC payloads are deprecated from macOS 27 but still apply, and the
  declarative replacement (`com.apple.configuration.app.settings` → `Privacy`) has no Full Disk Access key yet
  (verified against Apple's device-management docs, 2026-10-07).

## The preference domain

- Domain: `com.veszelovszki.cmdr` (`config::BUNDLE_ID`), passed explicitly, never `kCFPreferencesCurrentApplication`, so
  dev builds and tests read the same domain as release.
- Profiles write `/Library/Managed Preferences/com.veszelovszki.cmdr.plist` (device scope) or
  `/Library/Managed Preferences/<user>/com.veszelovszki.cmdr.plist` (user scope). `CFPreferencesCopyAppValue` merges
  both with the user's own layer; `CFPreferencesAppValueIsForced` tells the managed layer apart. `CfPrefsSource` asks
  `IsForced` first and only then copies the value.
- Each read pass starts with `CFPreferencesAppSynchronize`, or a profile installed while Cmdr runs stays invisible.
- Linux and other platforms: `NoManagedPrefs`, "no restriction". A future Linux source (for example
  `/etc/cmdr/policy.json`) slots in behind `ManagedPrefsSource`.

## Key catalog

Every key only turns something off or narrows it. Absent or forced to its permissive value means "no restriction".

- **`DisableUsageStats`** (bool): no heartbeat, no events; spool and unreported uptime deleted. Locks
  `analytics.enabled` to off.
- **`DisableCrashAndErrorReports`** (bool): no crash report, no "Attach logs", no automatic or manual error report, no
  amend; a pending crash file is discarded at launch. "Save to disk" stays. Locks `updates.crashReports` and
  `updates.errorReports` to off. In-app feedback is NOT covered: it's text the person typed and sent on purpose (decided
  2026-10-05).
- **`DisableAutomaticUpdateChecks`** (bool): no background check; a manual check still works. Locks `updates.autoCheck`
  to off.
- **`DisableUpdates`** (bool): no check of any kind, no download, no install. Overrides the other two update keys. Locks
  `updates.autoCheck` to off.
- **`MaxUpdateVersion`** (string, or an integer read as a major): "never update past this version". `"0.52"` allows up
  to the last 0.52.x (core `< 0.53.0`), `"0.52.3"` up to and including 0.52.3, `"1"` anything below 2.0.0. Grammar: one
  to three dot-separated non-negative integers, optional leading `v`, surrounding whitespace trimmed. A `<real>` is
  refused (`0.5` and `0.50` are the same real). Unparseable or empty: behaves as `DisableUpdates`. Compared on the
  release core only, so `0.53.0-rc.1` doesn't pass `"0.52"`. ❗ The updater only knows the NEWEST release
  (`latest.json`), so once a release past the ceiling ships, a held Mac gets no more patches: it's "hold here until IT
  moves the ceiling", not a channel. Combines with `DisableAutomaticUpdateChecks`.
- **`DisableAI`** (bool): no AI at all, local model included: no LLM call, no model download, no `llama-server`, no Ask
  Cmdr, MCP `ai_search` refuses. The media index's on-device CLIP model is NOT covered: it's search indexing, not a
  generative feature (decided 2026-10-05). Locks `ai.provider` to `off` and `askCmdr.enabled` to off.
- **`DisableCloudAI`** (bool): the local model stays allowed; nothing goes to a cloud provider, loopback endpoints
  (Ollama, LM Studio) included, since `localhost` can be a tunnel to anywhere. `ai.provider` can't be `cloud`; a stored
  `cloud` reads as `off` (❌ never `local`: that would start a multi-GB download).
- **`AllowedCloudAIHosts`** (array of strings): cloud AI may only reach these hosts. Entries: a host
  (`api.openai.com`), `host:port`, a `*.` suffix pattern (`*.openai.azure.com`: any subdomain, not the apex), or a
  pasted URL (only its host and an explicit port count). Matching: IDNA to punycode, case-insensitive, one trailing dot
  ignored, compared as `url::Host` values (so `::1` matches `[::1]`); an entry with a port matches only that port, one
  without matches any. Malformed entries (`*`, `*.`, a pattern on an IP, a `*` elsewhere, an unparseable port, a path
  without a scheme) are dropped with a warning. An empty list, or a non-array value, allows no host, which equals
  `DisableCloudAI`. Recipe for "local Ollama only": leave `DisableCloudAI` off and list `localhost` and `127.0.0.1`.

Not keys (stay out until an admin asks; decided 2026-10-05): `DisableMCPServer`, a Jamf JSON schema, and "force on"
keys such as a pre-configured corporate AI gateway.

Egress no key turns off: license validation, the S3 price list (`Egress::S3PriceList`, always allowed), the CLIP model
download, and the user-initiated feedback and beta signup.

## Value parsing

- **Bools** accept `<true/>` / `<false/>`, an integer (`0` false, anything else true), and the strings `true`, `false`,
  `yes`, `no`, `1`, `0` in any case, trimmed. `defaults write … Key true` without `-bool` stores a string, hence the
  coercion. Anything else reads as `true` (restrictive) and warns.
- A forced value Core Foundation can't convert reads as `plist::Value::Data`, the wrong type for every key, so each key
  takes its restrictive reading.
- Unknown keys are ignored with one `debug!` (sources that can list their keys only), so a profile written for a newer
  Cmdr doesn't break an older one.
- Warnings go to the log only when they differ from the previous read: a bad profile warns once, not on every refresh.

## Precedence

- AI: `DisableAI` > `DisableCloudAI` > `AllowedCloudAIHosts`, folded into `ManagedPolicy::ai()` (`Allowed` /
  `LocalOnly` / `Off`) and `ai_destination()`. Only Cmdr's own `llama-server` (`AiDestination::LocalServer`) counts as
  on-device.
- Updates: `DisableUpdates` wins; `MaxUpdateVersion` and `DisableAutomaticUpdateChecks` combine, which is why
  `UpdatePolicy::Enabled` carries both.
- Per gate: allowed = policy allows AND the user's own setting allows.

## Data flow

- `ManagedPolicy` (fields as parsed) → typed answers → `ManagedPolicyView` (one payload for `get_managed_policy` and the
  `managed-policy-changed` event, carrying the lock list and what the Settings "what your organization manages" summary
  needs: update ceiling, AI mode, allowed hosts).
- `locked_settings()` maps policy to settings-registry ids: `Fixed { value }` or `DisallowedValues { values, fallback }`.
  `LockedValue` is a typed `bool | string` because `serde_json::Value` can't cross IPC. The fallback rides along so
  neither the frontend nor `overlay()` decides the safe value.
- `refuses_write(policy, id, value)` reads the same list for MCP `set_setting`, before its frontend round trip: a
  `Fixed` lock refuses every write, `DisallowedValues` only its values. The frontend's settings store applies the list
  the same way on reads and writes (`apps/desktop/src/lib/managed-policy/DETAILS.md`).
- `overlay()` applies the locks to a raw `settings.json` map in memory. A missing or non-object file becomes an object
  holding just the pinned values when anything is locked, since its readers fall back to defaults (`analytics.enabled`
  defaults to on).

## Refresh

- **Lazy first read**: `current()` initializes through a `OnceLock`, so no caller observes an unloaded policy. `init()`
  in `setup()` (before `crash_reporter::init`) forces that read at a known point; it's the one CF read on the main
  thread.
- **Activation**: `NSApplicationDidBecomeActiveNotification` on the DEFAULT center, read on a blocking thread.
- **Folder watch**: FSEvents on `/Library/Managed Preferences`, recursive, skipped when the folder doesn't exist (the
  first profile creates it; activation catches that one). Activation alone misses an MDM push while Cmdr stays
  frontmost through a long Ask Cmdr turn or a model download. Chromium watches the same path and also reloads
  periodically, calling it "undocumented and therefore fragile".
- **Every egress**: `for_egress().await`, coalesced to one read per second, behind a lock so racing callers make one
  trip.
- A change calls `apply_change` (cache.rs): it logs, emits `managed-policy-changed`, and hands the change to
  `ai::managed::apply_policy_change`, which cancels in-flight AI calls on any narrowing and stops `llama-server` plus a
  model download when `DisableAI` arrives (`ai/DETAILS.md` § Managed policy).

## Where the gates live

- **Api-server egress**: `server_request::send(Egress, request)` asks `allows` on a fresh read and refuses with
  `ServerRequestError::BlockedByPolicy` before anything leaves. Every api-server sender goes through it, the heartbeat
  included. Commands that build a bundle first ask the same decision through `server_request::check_policy`.
- **Usage stats**: `analytics::send_permission` and the heartbeat's config shape read `settings.json` through
  `overlay`, so a managed off is an ordinary opt-out and the heartbeat reports effective values plus a coarse
  `managedByOrganization` bool.
- **Ask Cmdr's switch**: `settings::load_ask_cmdr_switch` (the send gate and the wake readiness) reads through
  `overlay` too, as `AskCmdrSwitch::ManagedOff` when `DisableAI` pins a stored on. That's the policy's off, not the
  person's: the wake loop goes quiet but keeps its stored backlog, and a send names the organization.
- **Crash reports**: `check_pending_crash_report` discards the pending file unoffered under `DisableCrashAndErrorReports`.
- **Updates**: `ManagedPolicy::update_to(version)` is the one version decision (`UpdateRefusal::Disabled` /
  `AboveCeiling`). The updater asks `updates()` before a check (no request under `DisableUpdates`, none for a
  background trigger under `DisableAutomaticUpdateChecks`), then `update_to` for the offered release, again before the
  download and the install, and once more for the version the extracted archive names. `updater/DETAILS.md` § Managed
  policy.
- **AI**: `ai_destination` is the one decision, asked by `resolve_backend` (the typed reason, before consent) and by
  the LLM client before every request (the backstop, on a fresh read), plus each redirect hop.
  `any_cloud_refusal` is its host-independent half, for the consent predicate and cloud-only features.
  `ai/DETAILS.md` § Managed policy.

## Accepted residuals

- **`DisableAutomaticUpdateChecks` trusts the frontend's trigger.** `check_for_update(trigger)` refuses an automatic
  trigger, but the trigger is a frontend argument, so a bypassed or buggy frontend that labels its poll `command` runs
  background checks under that key (rule 1). Decided 2026-10-05 to accept it rather than move the poll into Rust: the
  frontend is our own code (no third party drives that command; MCP has no update tool), a manual check is allowed under
  this key anyway, so the most a mislabeled poll does is ask more often, nothing above `MaxUpdateVersion` installs, and
  `DisableUpdates` still blocks every request in the backend whatever the trigger says. Moving the decision would mean a
  Rust-side scheduler that owns the startup check, the poll interval, and the onboarding hold, all of which live in
  `src/lib/updates/updater.svelte.ts` today. Revisit if a non-UI caller ever gets an update-check path.

## Testing

- Tests elsewhere put a policy in force with `testing::override_for_test(testing::forcing(&[KEY]))`: a guard that makes
  `current()` and `for_egress()` answer that policy on the test's own thread (a `#[tokio::test]` runs its tasks there),
  so parallel tests never see each other's policy.
- `public_docs_test.rs` is the drift guard (§ The public mirror).
- Unit tests run over `FakeSource`. `source.rs` has a scratch-domain CF test (`com.getcmdr.policytest.<tag>`) proving a
  value in the user's own layer isn't reported, and a `PlistFileSource` round trip. `view.rs` tests
  `get_managed_policy` end to end through `CMDR_MANAGED_PREFS_FILE`.
- Dev and E2E: `CMDR_MANAGED_PREFS_FILE=/path/to/policy.plist pnpm dev` (debug and `playwright-e2e` builds only). The
  file is a plain plist dictionary of keys; every key in it counts as forced. `override_watch.rs` watches the file's
  folder and re-reads on any event naming the file, so an edit applies live; write it atomically (temp + rename) or a
  read can catch half a plist (it reads as no policy, with a warning, until the next event). A missing file is no
  policy, quietly: the macOS E2E lane points every shard at one that doesn't exist, and `managed-policy.spec.ts` writes
  and removes it.
- Real managed preferences need `sudo`, so an agent can't run them:
  `sudo defaults write "/Library/Managed Preferences/com.veszelovszki.cmdr" DisableUsageStats -bool true`, then
  re-activate Cmdr (or rely on the folder watch). If `IsForced` doesn't see it, make the file `root:wheel` `0644` like a
  profile-written one, then `sudo killall cfprefsd`. Undo with
  `sudo defaults delete "/Library/Managed Preferences/com.veszelovszki.cmdr"`. Unverified until David runs it; record
  which steps were needed here, dated.
