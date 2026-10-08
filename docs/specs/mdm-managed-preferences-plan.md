# MDM managed preferences: IT turns off telemetry, updates, and AI for every user

Tracks the "MDM deployment" section of [#118](https://github.com/vdavid/cmdr/issues/118) (its first three checkboxes,
plus the `/trust` line). Why: an IT team rolling Cmdr out through Jamf, Kandji, or Intune needs to switch off usage
stats, crash reports, updates, and cloud AI for everyone, and see that the app honors it. "On by default" is fine once
IT can turn it off. Out of scope: the `.pkg` installer and the PPPC (Full Disk Access) profile, which have their own
checkboxes.

## Loud rules (read before every milestone)

1. **The backend enforces; the frontend only displays.** Every send, check, and AI call consults the policy in Rust at
   the point where data would leave the Mac. A bypassed UI, an MCP client, or a stale `settings.json` must change
   nothing. IPC stays a pass-through.
2. **Policy only restricts.** Every key can only turn something off or narrow it. No key can force telemetry, updates,
   or cloud AI ON. Absent, `false`, or empty-where-meaningless means "no restriction".
3. **Only FORCED values count.** A value is policy only when `CFPreferencesAppValueIsForced` says so (a configuration
   profile put it in `/Library/Managed Preferences`). A user's own `defaults write com.veszelovszki.cmdr …` must never
   show up as "Managed by your organization".
4. **A key we can't read restricts as much as that key can.** Wrong type, unparseable version, garbage host list: apply
   the most restrictive reading of that one key and `warn!` once per change. An admin who meant "off" must never get
   "on" because of a typo.
5. **The policy overlays, it never rewrites.** Nothing writes a forced value into `settings.json` or the consent record.
   Remove the profile and every user's own choices come back untouched.
6. **Fresh read at egress, cached read everywhere else.** The cache drives the UI and local bookkeeping; every path that
   sends bytes off the Mac (heartbeat, crash and error report upload, update check and download, every LLM request)
   refreshes the policy first. A profile pushed while Cmdr runs must stop the next send, not the next launch.
7. **Typed, never string-matched.** New refusals are enum variants (`BackendResolution`, `AiTranslateErrorKind`, MCP
   `data.reason`, update and report outcomes). The `error-string-match` checks apply.
8. **TDD, real red first.** The parse rules, precedence, ceiling compare, host matching, and every gate get a failing
   test before the code.
9. **i18n: English plus `@key` descriptions only.** A translator agent does the other languages later
   (`.claude/rules/translations-by-a-translator-agent.md`). Copy below is a draft for David's review.
10. **Docs single-sourced.** The key catalog lives in exactly one canonical doc: `managed_policy/DETAILS.md`. The
    `/trust` page and the sample profile are the public mirror, guarded by a test (M8).

## The preference domain and keys

Domain: **`com.veszelovszki.cmdr`**, the bundle identifier (`tauri.conf.json` → `identifier`, `config::BUNDLE_ID`).
Every MDM defaults to "custom settings for bundle id", so this is what admins expect. Managed values land in
`/Library/Managed Preferences/com.veszelovszki.cmdr.plist` (device scope) or
`/Library/Managed Preferences/<user>/com.veszelovszki.cmdr.plist` (user scope); `CFPreferencesCopyAppValue` merges both
and `CFPreferencesAppValueIsForced` tells them apart from the user's own layer. Pass the domain explicitly
(`config::BUNDLE_ID`), never `kCFPreferencesCurrentApplication`, so dev builds and tests read the same domain as release
(Edge hit exactly this with per-channel domains:
<https://learn.microsoft.com/en-us/deployedge/configure-microsoft-edge-on-mac>). Cmdr isn't sandboxed, and the hardened
runtime needs no entitlement for CFPreferences reads.

Precedent: Chromium's macOS policy loader does exactly this (`CFPreferencesAppSynchronize`, then per key
`CopyAppValue` + `AppValueIsForced`, scope from whether the device-level plist holds the key), watches
`/Library/Managed Preferences/<user>/<bundle>.plist` with a file watcher, and ALSO reloads periodically because that
path is "undocumented and therefore fragile"
(<https://chromium.googlesource.com/chromium/src/+/main/components/policy/core/common/policy_loader_mac.mm>, read
2026-10-05). Apple's own contract is one sentence: "use this function to determine whether or not to disable UI
elements" (<https://developer.apple.com/documentation/corefoundation/cfpreferencesappvalueisforced(_:_:)>).

### Value parsing (applies to every key)

- **Bools** accept `<true/>`/`<false/>`, an integer (`0` is false, anything else true), and the strings `true`, `false`,
  `yes`, `no`, `1`, `0` (case-insensitive). Admins hand-write plists, and `defaults write … Key true` without `-bool`
  stores a STRING. Anything else (a dict, `"maybe"`) is rule 4: restrictive (`true`) plus a warn. Without the string
  coercion, rule 4 would turn an admin's `<string>false</string>` into "disabled", which is safe but baffling.
- **Strings** are trimmed; empty is "absent" only where the key says so.
- An unknown key in the domain is ignored with one `debug!` (a profile written for a newer Cmdr must not break an older
  one).

### Telemetry

- **`DisableUsageStats`** (bool). `true`: no heartbeat, no events, spool and unreported uptime deleted (exactly what a
  user's own opt-out does today). Locks `analytics.enabled` to off.
- **`DisableCrashAndErrorReports`** (bool). `true`: no crash report (automatic or from the dialog), no "Attach logs"
  after a crash, no automatic error report (Flow B), no manual "Send error report" (Flow A), no amend. A pending crash
  file from the last session is discarded at launch without an offer. "Save to disk" in the error-report dialog stays
  (nothing leaves the Mac). Locks `updates.crashReports` and `updates.errorReports` to off.

One key for both report kinds because both ship diagnostics to us and an admin turning off one would always turn off the
other. Feedback is NOT covered (see Decisions).

### Updates

- **`DisableAutomaticUpdateChecks`** (bool). `true`: no background check; "Check for updates" by hand still works. Locks
  `updates.autoCheck` to off.
- **`DisableUpdates`** (bool). `true`: no check of any kind (so no request to `api.getcmdr.com/update-check`), no
  download, no install. IT ships new versions itself. Implies the previous key.
- **`MaxUpdateVersion`** (string). "Never update past this version." `"0.52"` allows anything up to the last 0.52.x
  (ceiling `< 0.53.0`); `"0.52.3"` allows up to and including 0.52.3; `"1"` allows anything below 2.0.0. A newer release
  isn't offered; the UI says one exists and that the organization holds Cmdr back. Grammar: one to three dot-separated
  non-negative integers, optional leading `v`, nothing else. An integer plist value is accepted as a major (`1` →
  `"1"`); a `<real>` is rejected (`0.5` and `0.50` are the same real, so the admin's intent is lost). Unparseable or
  empty → behaves as `DisableUpdates` (rule 4).
  - ❗ **Compare on the release core only** (`major.minor.patch`, prerelease and build metadata stripped) against the
    exclusive bound. In semver `0.53.0-rc.1 < 0.53.0`, so a plain `Version` compare against `< 0.53.0` would let a 0.53
    prerelease through a `"0.52"` ceiling. `"0.52.3"` means core `<= 0.52.3`.

Why "ceiling" and not "pin to exactly X": the updater only knows `latest.json`, which names the newest release. It can't
fetch an older one, so "pin" can only mean "don't go past". ❗ The same fact limits the ceiling: `"0.52"` delivers
0.52.x patches only while the NEWEST release is still a 0.52.x. Once 0.53.0 ships, a Mac held at `"0.52"` gets nothing
more (Cmdr doesn't backport, and the updater can't see an older patch). So it's a "hold here until IT moves the ceiling"
switch, not Chrome's `TargetVersionPrefix` (Chrome serves older channels). `/trust` and the key catalog say exactly
that, so no admin expects patch releases to keep coming.

### AI

- **`DisableAI`** (bool). `true`: no AI at all. No LLM call, no local model download, no `llama-server`, no Ask Cmdr, no
  "try local AI" offer, MCP `ai_search` refuses. Locks `ai.provider` to `off` and `askCmdr.enabled` to off.
- **`DisableCloudAI`** (bool). `true`: the local model is still allowed; nothing goes to a cloud provider. The `cloud`
  provider option is disabled; the "Allow cloud AI" switch is locked off. A user whose stored provider is `cloud`
  behaves as `off` (❌ never silently switch them to `local`: that starts a multi-GB download).
- **`AllowedCloudAIHosts`** (array of strings). When present, cloud AI may only reach these hosts. Each entry is a
  hostname (`api.openai.com`, matched case-insensitively and exactly) or a `*.` suffix pattern (`*.openai.azure.com`,
  matches any subdomain, not the bare domain). An empty array means no host is allowed, which equals `DisableCloudAI`.
  Loopback endpoints (Ollama, LM Studio) are `cloud` providers in Cmdr, so they need `localhost` / `127.0.0.1` listed
  like any other host. Under `DisableCloudAI` they stay blocked (decided: `localhost` can be an SSH tunnel to anywhere,
  so "on-device" can't be proven). The recipe for "local Ollama only" is: leave `DisableCloudAI` off and set
  `AllowedCloudAIHosts` to `localhost` and `127.0.0.1`. `/trust` documents that recipe (M8). Malformed entries are
  dropped with a warn; a non-array value reads as an empty list (rule 4). Normalization, identical on both sides (entry
  and request URL):
  - An entry may be a bare host, `host:port`, or a pasted URL (`https://api.openai.com/v1`): admins paste URLs, and
    dropping those would silently block the provider they meant to allow. Only host and an explicit port are used.
  - Hosts go through `url::Host::parse` (IDNA to punycode, lowercase, IPv4/IPv6 parsed), and one trailing dot is
    stripped (`api.openai.com.` is the same host). Compare `url::Host` values, never `host_str()` strings: `host_str()`
    keeps IPv6 brackets (`[::1]`, see `host_is_loopback` in `ai/connection_check.rs`) and an entry `::1` must match it.
  - An entry WITH a port matches only that port (`port_or_known_default()` on the URL side); an entry without one
    matches any port.
  - `*`, `*.`, and a pattern on a bare IP are malformed (dropped). `*.com` is legal: the admin's call.

Why hosts and not provider ids: the host is where the data actually goes, and it's what a security review asks about.
Provider ids are a frontend preset table (`cloud-providers.ts`), and the `custom` and `azure-openai` presets take any
URL, so an id allowlist would either leak (allow `custom`) or forbid a company's own LLM gateway. A host list covers a
corporate gateway (`llm.corp.example.com`) and Azure tenants (`*.openai.azure.com`) with the same rule.

### Precedence

`DisableAI` > `DisableCloudAI` > `AllowedCloudAIHosts`. `DisableUpdates` overrides the other two update keys, but
`MaxUpdateVersion` and `DisableAutomaticUpdateChecks` are ORTHOGONAL and combine (a ceiling with manual checks only is
the common IT setup), so the typed update policy must hold both at once (see M1). Then, per gate: allowed = policy
allows AND the user's own setting allows. Policy never turns a user's "off" into "on".

### Linux and other platforms

The source is macOS-only for now. Elsewhere the policy reads as "no restriction" (a `NoManagedPrefs` source), and the
docs say so. A future Linux source (for example `/etc/cmdr/policy.json`) slots in behind the same trait. The test-build
`PlistFileSource` override (§ Architecture) works on EVERY platform, so the Linux Docker E2E lane can run the policy
specs too (the `plist` crate parses XML plists anywhere).

## Fresh grep: where each gate lives today

Verified at `5c8f7cec6`; line numbers drift, names don't.

- CFPreferences precedent: `dock/prefs.rs` (`is_forced` ~:80, `to_plist` / `from_plist` ~:86–114, the scratch-domain
  test pattern `com.getcmdr.docktest.<tag>`), `glass_tint.rs` (activation observer ~:113, sync off the main thread
  ~:123), `reveal/registration.rs`. `objc2-core-foundation` already has the `CFPreferences` and `CFPropertyList`
  features (`src-tauri/Cargo.toml` ~:408). No new dependency.
- Usage stats: `analytics/mod.rs` `send_permission` (~:68, the one gate for capture AND heartbeat),
  `analytics/events.rs:35` and `analytics/heartbeat.rs:163` (`SendPermission::OptedOut` arms; the heartbeat one deletes
  the spool).
- Crash reports: `commands/crash_reporter.rs` (`check_pending_crash_report` :12, `dismiss_crash_report` :19,
  `send_crash_report` :32); the actual send is `crash_reporter/pending_delivery.rs` (`send_pending_crash_report` :36 →
  `post_crash_report` :157, the ONE function that POSTs `/crash-report`); `crash_reporter::init` (`lib.rs:284`, BEFORE
  `load_settings` at :434) runs `next_launch::process_pending_crash`; frontend decision in
  `src/lib/crash-reporter/pending-crash-report.ts`; panic courier and survival paths in
  `crash_reporter/panic_courier.rs`, `survival.rs`.
- Error reports: `error_reporter/auto_dispatcher.rs` (`set_enabled` ~:103, the Flow B master switch, seeded in
  `lib.rs:469`), `commands/error_reporter.rs` (`send_error_report` :156, `amend_error_report` :205,
  `save_error_report_to_disk` :226 stays allowed, `send_crash_log_report` :282). The actual sends:
  `error_reporter::upload` (`error_reporter/mod.rs` :532, called from all three report paths: auto-dispatcher :289, Flow
  A :166, crash log :298) and `error_reporter/auto_sent.rs` `send_amend` (:173, its own `reqwest` client).
- Shared api-server error type: `server_request.rs` `ServerRequestError` (crash, error, amend, update check), mapped to
  copy in `src/lib/error-messages/server-request.ts`. `server_request::send(request)` is ALREADY the one function six
  senders go through: `post_crash_report`, `error_reporter::upload`, `auto_sent::send_amend`, the updater's
  `fetch_manifest` and `fetch_verified_tarball`, and `s3_costs/price_source.rs`. The heartbeat (`analytics/heartbeat.rs`
  `send_payload`, its own `reqwest` client and `BeatOutcome::from_status`) and license validation
  (`licensing/validation_client.rs`) don't use it.
- Settings the backend reads straight from `settings.json` (no frontend in the loop): `analytics.enabled`
  (`send_permission` via `settings::load_settings`), `askCmdr.enabled` (`settings/ai_gates.rs` `load_ask_cmdr_enabled`,
  read per send and by wake readiness), and the whole raw map for the heartbeat's config shape
  (`analytics::read_raw_settings` → `config_shape::build_config_shape`). `ai.provider` reaches the backend only through
  the frontend's `configure_ai` push. The api-server stores the heartbeat `config` blob verbatim
  (`telemetry/heartbeat.ts` `validateConfig`: any plain object under the size cap), so a new bool in it needs no Worker
  change.
- Updates: `updater/mod.rs` (`skip_reason` :89, `check_for_update` :109 → `Result<Option<UpdateInfo>, …>`,
  `download_update` :236, `install_update` :267 → `Result<(), String>`, so its new refusal needs a typed error),
  `updater/manifest.rs:47` (semver compare). ❗ `download_update` takes `url` and `signature` FROM THE FRONTEND and
  `UpdateState` stores only the tarball path, so today the backend doesn't know which version it staged (see M3). On
  Linux the frontend calls `@tauri-apps/plugin-updater` `check` directly (`tauri_builder.rs` `register_updater`), which
  bypasses all of this; frontend loop and `updates.autoCheck` in `src/lib/updates/updater.svelte.ts`; menu "Check for
  updates" → `runMenuTriggeredCheck()`.
- AI: `ai/manager.rs` (`resolve_backend` :179, `resolve_backend_inner` :299, `compute_ai_status` :114 with its `Offer`
  branch, `configure_ai` :404), `ai/server.rs:49` `start_ai_server`, `ai/install.rs:85` `start_ai_download`,
  `ai/cloud_consent.rs` (`has_current_cloud_consent` :65, already documented as "a future managed preference becomes one
  more argument here"; `CloudAiConsentStatus` :94; `revoke_cloud_ai_consent` :193 shows the cancel-everything sequence),
  `ai/connection_check.rs` (`check_ai_connection`, `validate_ai_base_url`), `ai/translate_error.rs` +
  `src/lib/ai/translate-error-toast.ts` (lockstep enums), `agent/wake/readiness.rs` (`NeedsCloudConsent`). Callers of
  `resolve_backend*`: `ai/suggestions.rs:214`, `ai/manager.rs` `resolve_translate_backend`, `agent/chat/session.rs:113`
  (Ask Cmdr: resolved ONCE per turn, then many LLM calls in the tool loop), `agent/wake/snapshot.rs:104` (background
  wakes). Every remote request leaves through one of three functions in `ai/client.rs`: `exec_chat_stream_request` :136
  (Ask Cmdr via `agent/llm/genai_impl.rs`), `chat_completion` :365, `chat_completion_stream` :478. `AiBackend::remote`
  is `pub(in crate::ai)`, so nothing outside `ai/` builds one. ❗ `AiBackend` holds only `client`, `model`, and
  `log_ctx`: it doesn't remember its base URL or whether it's the local server, and both `local()` and `remote()` build
  their `genai::Client` with `Client::builder().with_service_target_resolver(..).build()` (no `with_reqwest` yet). Ask
  Cmdr DOES run on the local provider (`definitions/ai.ts`: "On Local it starts a proactive loop"). `ai/download.rs`
  fetches the local model. Provider presets: `src/lib/settings/cloud-providers.ts` (Azure's placeholder
  `https://{resource-name}.openai.azure.com/openai/v1` doesn't parse as a URL host).
- MCP: `mcp/executor/search.rs:304` `execute_ai_search` (typed `data.reason: "cloudAiNotAllowed"` at :296),
  `mcp/executor/async_tools.rs:536` `execute_set_setting` (round-trips `mcp-set-setting` to the frontend, no backend
  check today); frontend half in `src/lib/settings/mcp-main-bridge.ts` (`mcp-get-all-settings`, `mcp-set-setting`).
- Settings UI: `src/lib/settings/settings-store.ts` (`getSetting` :322, `setSetting` :378, `resetSetting` :473),
  `components/SettingRow.svelte` (already has `disabled` + `disabledNote` + `disabledNoteId`),
  `components/boolean-setting.svelte.ts` (`useBooleanSetting`), sections pass `disabled` / `disabledNote` to
  `SettingRow` themselves today. The `ai.provider` row in `AiSection.svelte` is a bespoke segmented radiogroup that
  already disables one option with a tooltip (`local` on Intel, `settings.ai.tooltipLocalDisabled`); `cloud` under
  `LocalOnly` reuses that path. `askCmdr.enabled` is already `mcpSettable: false` (the bridge's `notSettableOverMcp`).
  Sections `UpdatesSection.svelte`, `AiSection.svelte`, `AiCloudSection.svelte`, `AskCmdrSection.svelte`; shared
  provider setup in `src/lib/ai-provider-setup/`; `src/lib/ai/AiCloudConsentToggle.svelte`; onboarding `StepAi.svelte`,
  `StepBeta.svelte`.
- Website: `apps/website/src/lib/trust.ts:242` (the "No central administration" gap) and :245 (the `.pkg`/PPPC gap,
  which stays), `src/pages/trust.astro:70` ("There's no central (MDM) control yet.").
- Egress NO key covers (IT will ask what's left; `/trust` must say it, see M8): license validation
  (`licensing/validation_client.rs`), the S3 price list (`s3_costs/price_source.rs`), the CLIP model download from
  Hugging Face (`crates/cmdr-index/src/media_index/clip/install.rs`), and the user-initiated feedback
  (`commands/feedback.rs`) and beta signup (`commands/beta_signup.rs`).

## Architecture

New backend module **`src-tauri/src/managed_policy/`** (named after the UI phrase "Managed by your organization"):

- `mod.rs`: `ManagedPolicy` (typed, `Default` = no restriction), `current()` (cached), `refresh()` (fresh read; updates
  the cache; returns whether it changed), `for_egress()` (async: the coalesced fresh read of rule 6, run in
  `spawn_blocking` so a slow `cfprefsd` never stalls a tokio worker), `get_managed_policy` command,
  `ManagedPolicyChanged` event (tauri-specta). The command and the event carry ONE payload,
  `ManagedPolicyView { managed: bool, usageStatsDisabled, reportsDisabled, updates, ai, lockedSettings }`, so the
  frontend gets the lock list from the same place as everything else and never derives it.
- `egress.rs`:
  `enum Egress { Heartbeat, CrashReport, ErrorReport, ErrorReportAmend, UpdateCheck, UpdateDownload, S3PriceList }` and
  `ManagedPolicy::allows(Egress) -> bool`, one exhaustive `match`. The arms that are always `true` (`S3PriceList`) are
  the code form of the "traffic no key turns off" list on `/trust`: adding a variant forces a decision.
- `refusal.rs`: `enum ManagedAiRefusal { AiOff, CloudAiOff, HostNotAllowed }` (specta-exported, camelCase). The ONE AI
  refusal type: `BackendResolution::Managed(_)`, `AiTranslateErrorKind::Managed` (with the refusal as a field),
  `SlotRefusal::Managed(_)`, the MCP `data.reason`, `check_ai_connection`, and the client backstop all carry it, so the
  frontend has one copy map (`managedAiRefusalMessage`) and a fourth surface can't spell it differently.
- `keys.rs`: the key-name constants and the pure parse `ManagedPolicy::from_source(&dyn ManagedPrefsSource)`. The ONE
  place a key name is spelled in Rust.
- `source.rs`: `trait ManagedPrefsSource { fn forced_value(&self, key: &str) -> Option<plist::Value>; }` with
  `CfPrefsSource { domain }` (macOS: `CFPreferencesAppSynchronize` once per read pass, then per key
  `CFPreferencesAppValueIsForced` → `CFPreferencesCopyAppValue` → `plist::Value`), `PlistFileSource` (test builds only,
  below), `NoManagedPrefs` (other platforms), and a `FakeSource` for unit tests.
- `locked.rs`: `locked_settings(&ManagedPolicy) -> Vec<LockedSetting>` where `LockedSetting { id, lock }` and `lock` is
  `Fixed(serde_json::Value)` or `DisallowedValues(Vec<serde_json::Value>)` (for `ai.provider` under `DisableCloudAI`:
  `cloud` disallowed). The ONE mapping from policy to registry setting ids; the frontend overlay and MCP `set_setting`
  both read it. Plus `overlay(&ManagedPolicy, &mut serde_json::Value)`, the Rust twin of the frontend overlay over a raw
  `settings.json` map (pure, never writes). Every backend reader of a lockable id goes through it:
  `analytics::read_raw_settings` (so the heartbeat's config shape reports EFFECTIVE values, decided), the consent read
  in `send_permission`, and `settings::load_ask_cmdr_enabled` (so the wake loop goes quiet under `DisableAI` without a
  policy check of its own).
- `ceiling.rs`: `UpdateCeiling` parse and `allows(&semver::Version)`. `hosts.rs`: `HostPattern` parse and
  `allows(&url::Url)`, comparing `url::Host` values per § AI (❌ not `host_str()` strings, which keep IPv6 brackets).

The CF↔`plist::Value` conversion in `dock/prefs.rs` moves to a shared crate-level helper (for example `cf_plist.rs`)
that both `dock` and `managed_policy` use, rather than a second copy (jscpd would flag it anyway).

Refresh triggers:

- **First read is lazy and blocking**: `current()` initializes the cache with a synchronous read on first use (a
  `OnceLock`/`LazyLock`), so there is NO window where a caller sees `Default` (= no restriction) because setup hadn't
  reached the load yet. Fail-open-until-loaded is exactly the bug an ordering comment can't prevent. `setup()` still
  calls it explicitly before `crash_reporter::init` (`lib.rs:284`) so the cost lands at a known point. That one read
  runs on the main thread (one `cfprefsd` XPC round trip, same as `glass_tint`'s initial read); every LATER read goes
  off the main thread.
- **Activation**: `NSApplicationDidBecomeActiveNotification` (the `glass_tint.rs` pattern, `spawn_blocking`).
- **A file watch on `/Library/Managed Preferences`** (recursive, so the per-user subfolder counts; skip when it doesn't
  exist). Activation alone misses the common case: an MDM pushes while Cmdr stays frontmost for hours with a long Ask
  Cmdr turn or a model download running, and nothing would stop it until the next send. Chromium watches the same path
  for the same reason (see § The preference domain). The watcher only triggers `refresh()`; CF stays the source of
  truth.
- **Every egress point** (rule 6). Coalescing to at most one CF read per second is fine (an Ask Cmdr tool loop or a
  suggestion stream fires many requests).

On a change, `refresh()` emits `ManagedPolicyChanged` and calls one explicit `apply_change(app, old, new)` that does the
immediate stops: AI cancels in-flight cloud turns and suggestion streams whose backend the NEW policy would refuse
(cloud off, or its host no longer allowed), stops `llama-server` and cancels an in-progress local model download when
`DisableAI` arrived. No observer registry: one function, a visible call list.

Egress gates sit in the LOWEST send function, not only in the commands. The commands still refuse early with a typed
outcome (good UX, no wasted bundle build), but the guarantee comes from the send function, so a new caller can't forget
it:

- api-server senders: **`server_request::send` takes a required `Egress` argument**
  (`send(Egress::CrashReport, request)`) and refuses with a new `ServerRequestError::BlockedByPolicy` before the request
  leaves, after `ManagedPolicy::for_egress()`. Six senders already funnel through it (§ Fresh grep), so the gate is
  written once and a new caller can't compile without naming its pipeline. The heartbeat moves onto it too (`Ok` →
  `Acknowledged`, `Refused { status }` → `BeatOutcome::from_status(status)`, transport errors → `Failed`,
  `BlockedByPolicy` → the `OptedOut` arm's cleanup). This replaces five hand-placed checks in five low-level functions,
  which is exactly the duplication that drifts. License validation stays on its own client (no key covers it).
  `BlockedByPolicy` maps in `server-request.ts`; it's never logged at warn or error.
- LLM: the three `ai/client.rs` request functions check `ManagedPolicy::for_egress()` against the backend's own
  destination right before `exec_chat*`. That needs `AiBackend` to know where it points: it gains
  `destination: AiDestination { LocalServer, Remote(url::Url) }`, set by `local()` / `remote()` (parsed once there, so
  the check compares the exact URL `genai` will call). `LocalServer` is refused only under `DisableAI`; `Remote` is
  refused under `DisableAI`, `DisableCloudAI`, or a host outside the list. This is what makes rule 6 hold for Ask Cmdr,
  whose backend is resolved once per turn and reused across the tool loop, and it closes the
  `resolve_backend_with_model` re-read of `get_cloud_config()`. `resolve_backend` keeps its check too: it produces the
  typed, user-facing reason; the client check is the backstop. Both call the same
  `ManagedPolicy::ai_destination(&AiDestination) -> Result<(), ManagedAiRefusal>`, so there's one decision with two call
  sites, not two decisions.

Test-build override: **`CMDR_MANAGED_PREFS_FILE=<path to a plist>`** replaces the CF source with `PlistFileSource`,
honored ONLY under `cfg(debug_assertions)` or the `playwright-e2e` feature. ❌ Never honor it in a plain release build:
it replaces IT's policy wholesale. ❌ Don't add it to `prod_instance::NON_PROD_ENV_VARS`; it isn't a harness signal.

Frontend: **`src/lib/managed-policy/`** holds a reactive store fed by `get_managed_policy` at `initWindowSettings()` and
refreshed on `ManagedPolicyChanged`. The settings store reads it: `getSetting(id)` returns the `Fixed` value for a
locked id (and maps a disallowed stored value to the setting's safe value, `off` for `ai.provider`), `setSetting` and
`resetSetting` refuse a locked id without writing, and `SettingRow` plus the row primitives render locked ids disabled
with the managed note. A managed lock WINS over a reason the section passes (`disabled` / `disabledNote`, for example
"Apple Silicon only"): one note per row, the managed one. Restricted windows (viewer, queue) don't render any of these
settings, so they need nothing. Because `ai.provider` is overlaid before `pushConfigToBackend()` reads it, the backend's
`configure_ai` receives the effective provider with no extra plumbing.

## Draft copy (David reviews; English only)

- Row note, generic: "Your organization manages this setting."
- Updates off: "Your organization manages updates for Cmdr."
- Ceiling: "Cmdr {available} is out, but your organization keeps this Mac on {ceiling} or earlier."
- AI off: "Your organization turned off AI in Cmdr."
- Cloud AI off: "Your organization allows only on-device AI."
- Host not allowed (picker row, connection check, translate toast): "Your organization doesn't allow this AI service.
  Your IT team can tell you which ones you can use."
- Reports off (crash dialog never shows; Help › Send error report dialog): "Your organization turned off sending
  reports. You can still save one to disk and share it yourself."
- Usage stats off (onboarding step): "Your organization turned off usage stats."
- Section line (top of Updates & privacy, and of AI, when any row in it is managed): "Your organization manages some of
  these settings." Plain text in reading order, so a keyboard or VoiceOver user meets the reason before reaching the
  disabled controls (native `disabled` takes them out of the Tab order, so a per-row `aria-describedby` note alone is
  only heard by someone who arrows through the page).
- Cloud option under `LocalOnly` (shown as a visible line under the provider row, not only the option tooltip, since a
  tooltip on a disabled radio isn't reliably announced): "Your organization allows only on-device AI."
- `LocalOnly` on an Intel Mac (local AI needs Apple Silicon, so nothing is left): "Your organization allows only
  on-device AI, and this Mac can't run it."
- Held update, Settings status line and the result of a manual check only: the ceiling sentence above. A background
  check that finds only a held update stays silent (no toast): the person can't act on it, and it would come back every
  poll.
- MCP refusal texts can be plain English (agent-facing), with the typed `data.reason`.

## Milestones

Sequential, one agent each. Each runs `pnpm check --fast` while iterating and plain `pnpm check` (Rust milestones:
clippy included) before committing, and updates the `CLAUDE.md` / `DETAILS.md` of every directory it touches.

### M1. Policy core: read, parse, cache, expose

- **Scope**: `managed_policy/` as above, the shared CF↔plist helper (moved out of `dock/prefs.rs`), lazy first read,
  activation refresh, the `/Library/Managed Preferences` watch, `get_managed_policy` command + `ManagedPolicyChanged`
  event + generated bindings, `PlistFileSource` override, `egress.rs` and `refusal.rs` (types only, no callers yet),
  `locked.rs` with `overlay`, `managed_policy/CLAUDE.md` + `DETAILS.md` (the canonical key catalog, parse rules,
  precedence, manual test recipe), a line in `docs/architecture.md`. No gate changes yet.
- **Intentions**: everything downstream asks `managed_policy::current()` / `refresh()` and never touches CFPreferences
  or key names itself. `ManagedPolicy` exposes typed answers (`usage_stats_disabled()`,
  `ai() -> AiPolicy { Allowed, LocalOnly, Off }`, `ai_destination(..)`, `updates() -> UpdatePolicy`), not raw values.
  `UpdatePolicy` is `Disabled | Enabled { automatic_checks: bool, ceiling: Option<UpdateCeiling> }`: a flat
  `{ Allowed, NoAutomaticChecks, Ceiling, Disabled }` enum can't express "a ceiling AND no background checks", which is
  the most common combination.
- **Landmines**: `CFPreferencesCopyAppValue` merges user, any-user, and managed layers, so ALWAYS gate on `IsForced`
  first (rule 3). Call `CFPreferencesAppSynchronize` before reading, or a profile installed while running stays
  invisible to this process (the `glass_tint.rs` gotcha). CF calls go off the main thread except the one lazy first read
  (§ Architecture). A forced `false` is "no restriction", not "managed with value false": no key locks anything when
  it's not restrictive. Moving `to_plist` must keep `dock`'s tests green.
- **Test plan**: pure tests over `FakeSource` for every key: absent, forced `true`/`false`, the bool coercions
  (`"false"`, `"YES"`, `0`, `2`, a dict), wrong type (rule 4), the ceiling grammar (`"0.52"`, `"0.52.3"`, `"1"`, integer
  `1`, real `0.52`, `"v0.52"`, `""`, `"latest"`, `"0.52.3.1"`) and its prerelease edge (`0.53.0-rc.1` vs `"0.52"` is
  REFUSED), host patterns (exact, `*.` suffix, bare apex vs `*.`, case, trailing dot, entry with port vs URL with and
  without port, pasted-URL entry, IPv6 `::1` vs `[::1]`, userinfo trick `https://api.openai.com@evil.com`, look-alike
  `api.openai.com.evil.com`, IDN, `*` alone), precedence combos (ceiling + no automatic checks together),
  `locked_settings` and `overlay` output (overlay leaves unlocked keys and an absent file alone), the `Egress` table
  (every variant, each policy), and that `current()` before any explicit load returns the real policy, not `Default`. A
  CF integration test on a scratch domain (`com.getcmdr.policytest.<tag>`, torn down like the dock tests) proving a
  value the USER layer holds is NOT reported. `PlistFileSource` round-trip. The real `/Library/Managed Preferences`
  recipe needs `sudo`, so an agent can't run it: it moves to M9 (David).
- **DONE**: `get_managed_policy` returns the right `ManagedPolicyView` under the file override (a Rust test through
  `PlistFileSource`, plus one manual `pnpm dev` with `CMDR_MANAGED_PREFS_FILE`); nothing else in the app behaves
  differently yet; `pnpm check` green.
- **Implementation notes** (M1 as built):
  - `LockedSetting.lock` carries a typed `LockedValue` (`bool | string`), not `serde_json::Value`, which can't cross IPC
    under specta rc.24 (`src/lib/ipc/CLAUDE.md`). `DisallowedValues` also carries a `fallback` (`off` for
    `ai.provider`), so neither the frontend overlay nor Rust `overlay` decides the safe value itself.
  - `overlay` turns a missing or non-object settings map into an object holding just the pinned values when anything is
    locked: its readers fall back to defaults, and `analytics.enabled` defaults to on. With no locks it leaves the map
    alone.
  - `AiDestination` and `ManagedPolicy::ai_destination` live in `managed_policy/` already (pure, tested); M4 stores the
    destination on `AiBackend` and calls it.
  - The command is registered as `managed_policy::view::get_managed_policy` (Tauri's command macro needs the defining
    module path). The module carries a temporary `allow(dead_code, unused_imports)` until M2–M4 call in; the last of
    them removes it.
  - `plist` moved from the macOS-only dependency table to the macOS+Linux one (no new crate): the parse runs on Linux
    too, for the E2E file override.

### M2. Telemetry enforcement

- **Scope**: first the shared gate: `server_request::send` takes `Egress` and refuses with
  `ServerRequestError::BlockedByPolicy` (§ Architecture); update its six callers mechanically and move the heartbeat
  onto it. That alone makes invariant 1 hold for every api-server path. Then the early, user-facing refusals:
  `send_permission` reads consent through `locked::overlay`, so a managed off is an ordinary `OptedOut` (capture drops,
  the heartbeat deletes spool and unreported uptime) with no new variant. Crash reports: `check_pending_crash_report`
  discards the pending file (via `dismiss_pending_crash_report`) and answers `None` when reports are disabled;
  `send_crash_report`, `send_crash_log_report`, `send_error_report`, `amend_error_report` return `BlockedByPolicy`
  before building a bundle; Flow B's dispatcher checks `allows(Egress::ErrorReport)` at send time (in addition to its
  `set_enabled` atomic) so a forced off wins over a stored on. `save_error_report_to_disk` unchanged. Docs:
  `analytics/`, `crash_reporter/`, `error_reporter/`, `server_request.rs`'s module doc, and its frontend mapping.
- **Intentions**: one predicate per pipeline, read where the send happens. A user's stored `true` in `settings.json` is
  irrelevant while the policy says off.
- **Landmines**: the panic hook and signal handler must not touch the policy (allocation, locks, XPC): capture stays as
  is; only delivery checks. The auto-dispatcher's no-flush-on-shutdown rule stays. ❌ Don't move the crash-file stamp
  (`error_reporter/CLAUDE.md`). A refusal is not an error: log at `info`/`debug`, never via `log_error!` (that IS the
  auto-report path). The crash discard must use the existing claim/delete helpers, not a raw `remove_file` that races a
  newer crash.
- **Test plan**: red first on `server_request::send` itself: a `wiremock` server (`crash_reporter/tests.rs:910` is the
  template) receives ZERO requests for each gated `Egress` under the matching policy, and one for `S3PriceList` under
  every key; `send_permission` with each policy × consent combo (through `overlay`); heartbeat test that a managed-off
  beat sends nothing and clears the spool (existing localhost-Worker harness) and that the migrated status mapping keeps
  `Acknowledged` / `Refused` / `Failed` as before; command-level tests that each send command returns `BlockedByPolicy`
  without calling `upload`; dispatcher test with `set_enabled(true)` and a managed off; config-shape test that a stored
  `analytics.enabled: true` under the policy reports `false` and that `managedByOrganization: true` appears (decided:
  effective values plus that one coarse bool, never which keys are set).
- **DONE**: with `DisableUsageStats` / `DisableCrashAndErrorReports` forced, no request reaches `api.getcmdr.com`
  `/heartbeat`, `/crash-report`, or `/error-report` from any path, verified by tests.
- **Implementation notes** (M2 as built):
  - `server_request::check_policy(egress)` is the one decision; `send` calls it, and so do the commands that refuse
    early (and Flow B's `take_window_to_send`). The early refusals surface as `BlockedByPolicy` too (nested in
    `ErrorReportSendError::Server` for the error-report commands).
  - Tests outside the module use `managed_policy::testing::override_for_test`, a per-thread override of `current()` and
    `for_egress()`.
  - `Settings.analytics_enabled` is removed: consent reads raw `settings.json` through `overlay`.
  - `managedByOrganization` is always in the config shape (`false` when nothing is managed), like `fdaGranted`.
  - Frontend: `serverRequestLogLevel` gained `'info'` for `blockedByPolicy`, and its callers log at info. The copy key
    `errors.serverRequest.blockedByPolicy` is English plus the en-GB/en-AU spelling overlays; the other locales need the
    translator (the `i18n-coverage` lane flags it until then).
  - An update check or download under `DisableUpdates` now ends as `BlockedByPolicy` at `send` (logged at info); M3
    replaces that with typed outcomes before the request.

### M3. Update enforcement

- **Scope**: `check_for_update` returns a typed outcome that distinguishes "up to date", "update available", "held by
  policy (available X, ceiling Y)", and "updates disabled by policy" (reshape the `Option<UpdateInfo>` return, update
  bindings and the frontend callers mechanically). `DisableUpdates` short-circuits BEFORE the network request.
  `download_update` and `install_update` re-check the policy and the ceiling against the staged version (a staged update
  from before the profile arrived must not install). That needs a reshape: today `download_update(url, signature)`
  trusts a frontend-supplied URL and `UpdateState` keeps only the tarball path, so the backend can't know the staged
  version and a bypassed frontend could stage anything. `check_for_update` stores the offered `UpdateInfo` (version,
  url, signature) in `UpdateState`; `download_update` takes no URL and downloads only what was offered (re-checking the
  ceiling against that version); the staged slot records the version; `install_update` refuses when the CURRENT policy
  disallows it, which needs a typed `UpdateInstallError { BlockedByPolicy, NothingStaged, Failed { detail } }` in place
  of today's `Result<(), String>` (rule 7). A held update (newer than the ceiling) never toasts from a background check;
  only the Settings status line and a manual check's result say it (§ Draft copy). The frontend background loop treats
  `DisableAutomaticUpdateChecks` as `updates.autoCheck = false` via the overlay; the backend still refuses a
  background-triggered check if called anyway (the trigger is already passed for analytics; pass it to the command).
  Docs: `updater/` and `src/lib/updates/` C+D.md.
- **Intentions**: the backend decides; the frontend renders the outcome. The `update_check` analytics event gets a phase
  for each new outcome (categorical, no versions in props).
- **Landmines**: `checkForUpdates()` must never early-return on `ready` (`src/lib/updates/CLAUDE.md`); a managed outcome
  is a new terminal phase, not a failure (no error toast, no `log_error!`). Ceiling compare uses the same `semver` parse
  as `manifest.rs`, but on the release core only (§ Updates: a `0.53.0-rc.1` must not pass a `"0.52"` ceiling). The
  Linux plugin path is out of scope (macOS only, per § Linux); say so in the doc.
- **Test plan**: ceiling compare matrix; `check_for_update` with each rule using an injected manifest fetch (no network)
  proving `Disabled` never fetches; install refusal for a staged version above the ceiling (and for a policy that
  arrived between download and install); `download_update` with nothing offered refuses; frontend `updater.*.test.ts`
  for each outcome's state and that the background loop doesn't call the command under `NoAutomaticChecks`.
- **DONE**: each update rule behaves as specified with tests; the menu check under `DisableUpdates` shows the managed
  message instead of a check.
- **Implementation notes** (M3 as built):
  - One version decision, `ManagedPolicy::update_to(version) -> Result<(), UpdateRefusal { Disabled, AboveCeiling }>`,
    asked at the check (held vs offered), before the download, before the install, and once more inside the installer.
  - A fifth outcome, `AutomaticChecksDisabledByPolicy`: a background trigger refused under
    `DisableAutomaticUpdateChecks` needed its own typed answer. `UpdateCheckTrigger` is now a Rust enum (snake_case, the
    same analytics tokens), and the frontend's type aliases it.
  - The install also judges the version the extracted `Info.plist` names (`installer::vet_staged_bundle`, after the
    d7745e822 rollback check): `latest.json` isn't signed, so a manifest could offer a version under the ceiling and
    serve a genuine signed release above it.
  - `UpdateDownloadError` gained `NothingOffered` and `BlockedByPolicy`; `install_update` answers
    `UpdateInstallError { BlockedByPolicy, NothingStaged, Failed { detail } }` as planned.
  - The policy is asked BEFORE `skip_reason`, so a dev build with `CMDR_MANAGED_PREFS_FILE` shows the managed answer.
  - Frontend: managed answers land in `updateState.managed` (a terminal phase); `formatUpdateStatus` words them with two
    new keys (`updates.status.managedOff`, `updates.status.heldByPolicy`, the § Draft copy sentences), so Settings and
    the menu check's toast already say it. M6 owns any further surface work. A refused background check stops the poll
    loop (the backstop until M5's overlay keeps it from starting). Analytics outcomes: `updates_disabled_by_policy`,
    `held_by_policy`, `automatic_checks_disabled_by_policy`, `blocked_by_policy`.
  - A build already synced into the bundle (frontend `ready`) applies at the next restart whatever the policy says: on
    macOS the install IS the sync, so "staged" in this milestone means downloaded-not-yet-installed.

### M4. AI enforcement

- **Scope**: `resolve_backend` reads the policy via `ManagedPolicy::ai_destination`: any refusal becomes ONE new
  `BackendResolution::Managed(ManagedAiRefusal)` (`AiOff` / `CloudAiOff` / `HostNotAllowed`). It maps to one new
  `AiTranslateErrorKind::Managed` carrying the refusal (keep `translate-error-toast.ts` in lockstep), a quiet empty for
  suggestions, `SlotRefusal::Managed(_)` for Ask Cmdr, and an MCP `ai_search` `data.reason` that is the refusal's own
  camelCase name (`aiOff`, `cloudAiOff`, `hostNotAllowed`). `AiBackend` gains `destination` (§ Architecture).
  `has_current_cloud_consent` gains the policy argument (as its doc anticipates) and `CloudAiConsentStatus` a `managed`
  field. `check_ai_connection` refuses a blocked host or managed-off cloud without a request. `configure_ai` stores
  config as today but never spawns `llama-server` under `Off`; `start_ai_server` and `start_ai_download` refuse under
  `Off`; `compute_ai_status` never `Offer`s under `Off`. Wake readiness maps `Off` / cloud-blocked to a silent state.
  `apply_change` (from M1) cancels in-flight cloud work and stops the server, reusing the `revoke_cloud_ai_consent`
  sequence. The per-request backstop in the three `ai/client.rs` request functions (§ Architecture). Under
  `AllowedCloudAIHosts`, the remote client gets a `reqwest::redirect::Policy::custom` that stops at any hop whose host
  the policy refuses (genai 0.6.5 takes our own client via `ClientBuilder::with_reqwest`); without that, a 3xx from an
  allowed host reaches any host and invariant 3 is false. A batch command
  `cloud_ai_hosts_allowed(base_urls) -> Vec<bool>` for the frontend picker. Docs: `ai/` C+D.md (§ Cloud AI consent and §
  Provider routing), `mcp/DETAILS.md`, agent wake docs.
- **Intentions**: `resolve_backend` stays the one chokepoint; no caller adds its own policy check. Ask Cmdr runs on the
  local provider, so `LocalOnly` leaves `askCmdr.enabled` alone; only `Off` locks it. `apply_change` doesn't try to work
  out which in-flight call talks to which host: ANY narrowing of the AI policy calls the existing
  `stop_in_flight_cloud_calls()` (a rare event; the next request re-resolves and gets the typed reason).
- **Landmines**: the policy check comes BEFORE consent, key, and endpoint checks, so the user sees the managed reason,
  not "add a key". The host check uses the same base URL `AiBackend::remote` will use (including Ask Cmdr's
  `resolve_backend_with_model`, which rebuilds the backend from a second `get_cloud_config()` read; the client-level
  backstop covers it structurally). An Ask Cmdr turn resolves its backend once and then makes many requests, so a policy
  arriving mid-turn is only caught by the client-level check or `apply_change`, never by `resolve_backend`.
  `configure_ai` must not block (existing must-know).
- **Test plan**: `ManagedPolicy::ai_destination` matrix (`LocalServer` and `Remote` × every AI key);
  `resolve_backend_inner` matrix with the policy as a parameter (pure, no lock); translate-kind mapping tests on both
  sides; MCP `ai_search` tests asserting each typed `data.reason` (the existing `cloudAiNotAllowed` test is the
  template); consent predicate tests; `start_ai_download` refusal; a test that a policy change to `Off` cancels
  registered streams; a client-level test that a backend built while allowed refuses its NEXT request after the policy
  flips (the mid-turn case); a `wiremock` redirect test (allowed host 302s to a disallowed one, the second server
  receives nothing).
- **DONE**: under each AI key no request leaves for a disallowed destination from suggestions, both translate commands,
  Ask Cmdr (rail and wake), MCP `ai_search`, or the connection check, all proven by tests.
- **Implementation notes** (M4 as built):
  - `DisableAI` refuses EVERY provider, `off` included (`Managed(AiOff)`): under M5's overlay `ai.provider` reads `off`
    exactly then, and a translate toast or MCP client should name the organization, not "turn AI on".
  - `ManagedPolicy::any_cloud_refusal()` is `ai_destination`'s host-independent half. The consent predicate takes it (a
    host list leaves consent to the user), `CloudAiConsentStatus.managed` is that `Option<ManagedAiRefusal>`, and AI
    selection (cloud-only) answers it on a non-cloud provider. `cloud_host_allowed` is gone (nothing called it).
  - Ask Cmdr's wire refusal is ONE view kind, `managedByOrganization`: `AgentErrorKindView` is a unit enum the rail maps
    1:1 to copy, so `SlotRefusal::Managed(_)` keeps the refusal on the Rust side only. Under `DisableAI` the overlaid
    `askCmdr.enabled` answers `askCmdrOff` first anyway. A refusal from the client backstop mid-turn ends the turn as a
    provider failure (rare: `apply_change` cancels running turns first).
  - `AiTranslateError` gained `managed: Option<ManagedAiRefusal>` beside the `managed` kind. The toast has one generic
    line for now; M7 can word it per refusal from `err.managed`.
  - The redirect guard sits on every remote client and reads the CACHED policy per hop (reqwest's callback is sync; the
    request's own egress check refreshed it moments before). A refused hop stops, returning the 3xx.
  - `start_ai_server` / `start_ai_download` keep `Result<(), String>`: the frontend only logs their errors and never
    classifies them, and M7's UI doesn't offer either under `Off`.
  - "Narrowing" in `apply_change` means the AI part changed AND the new policy restricts AI at all, so even adding a
    host to a list stops in-flight calls (conservative; they re-resolve). The wake loop maps `Managed` to the silent
    `ProviderGate::Off`, keeping the stored backlog.

### M5. Frontend overlay and the MCP settings paths

- **Scope**: `src/lib/managed-policy/` store; settings-store overlay (`getSetting` / `setSetting` / `resetSetting` as in
  § Architecture); `SettingRow` and every row primitive pick up the lock themselves (disabled, managed note via
  `disabledNote`, no reset pip), so no section has to remember; `onSettingChange` fires for an id whose effective value
  changed when the policy changes, so `settings-applier.ts` re-pushes (for example `ai.*` through
  `pushConfigToBackend()`). MCP: `execute_set_setting` checks `locked_settings` in Rust BEFORE the round trip and
  refuses with `data.reason: "managedByOrganization"`; `mcp-get-all-settings` reports effective values plus a
  `managed: true` marker. Docs: `src/lib/settings/` C+D.md, `components/` C+D.md, `docs/guides/adding-a-new-setting.md`
  (one line: a policy-lockable setting gets its lock in `managed_policy/locked.rs`).
- **Intentions**: a locked row is visible and greyed with its reason, never hidden (the settings OS-backed-row
  precedent). Search still finds it. The managed note replaces any section-passed `disabledNote` on the same row. Each
  section that contains a locked row shows the section line (§ Draft copy) at its top, derived from `lockedSettings` (no
  per-section list to maintain).
- **Landmines**: sparse persistence: the overlay must never write (rule 5), and `isModified` must not report a locked
  row as modified because of the overlay. The settings window, main window, and onboarding all run
  `initWindowSettings()`; the policy fetch goes there, not per section. `onSpecificSettingChange` subscribers must see
  the overlay change exactly once.
- **Test plan**: settings-store tests (locked get/set/reset, disallowed stored value, overlay change emits once, nothing
  persisted); `SettingRow` / primitive a11y tests for the locked state (the note is reachable through
  `aria-describedby`; a managed lock beats a section-passed note); a section-line test (present iff the section holds a
  locked id); `mcp-main-bridge.test.ts` for the marker; Rust test for the `set_setting` refusal.
- **DONE**: every setting `locked_settings` names renders locked with the note in Settings and can't be changed from the
  UI or MCP.
- **Implementation notes** (M5 as built):
  - The policy fetch lives in `initializeSettings` (full windows), not `initWindowSettings`: the main window's layout
    calls `initReactiveSettings()` directly, and loading the policy before the store marks itself initialized is what
    keeps the updater's first `updates.autoCheck` read and the first `pushConfigToBackend()` from ever seeing an
    unlocked value. Every full window still reaches it through `initWindowSettings()`.
  - Pinned vs narrowed: only a `Fixed` lock disables a row (note, no pip). A `DisallowedValues` lock (`ai.provider`
    under `DisableCloudAI`) leaves the row usable; `setSetting` refuses just the ruled-out values, and M7's provider
    control disables the `cloud` option. The section line counts both. `resetSetting` refuses only a `Fixed` setting.
  - Under `DisableAI`, the `ai.provider` row renders locked with the note, but `AiSection`'s bespoke radios don't read
    the lock yet (they're not a row primitive): a click is refused by `setSetting` and changes nothing. M7 owns that
    control.
  - The section line comes from rows registering with their `SettingsSection` through context, so it needs no section
    path and no per-section list.
  - MCP: Rust `managed_policy::refuses_write` (one decision over `locked_settings`) answers `set_setting` before the
    round trip; the frontend bridge refuses too (`refusal: 'managedByOrganization'`), as the backstop for a policy
    change racing the round trip. `cmdr://settings` marks managed ids `managed: true` beside the effective `value`.
  - `managed-policy.svelte.ts` imports the `$lib/tauri-commands` barrel lazily (`await import`): the
    `desktop-ipc-unused` check only counts barrel imports, and a static one would pull the whole IPC surface into the
    settings store.

### M6. Feature surfaces: updates, privacy, reports, onboarding

- **Scope**: Updates & privacy section (the check button and status line render each M3 outcome, including the ceiling
  sentence); the menu "Check for updates" toast for managed outcomes; the error-report dialog under
  `DisableCrashAndErrorReports` (send hidden, "Save to disk" kept, the managed line); the crash flow shows nothing (the
  backend already answers `None`); `StepBeta.svelte` shows the managed line in place of the usage-stats switch. Copy in
  `messages/en/*.json` with `@key` descriptions.
- **Intentions**: one sentence of reason, in the place the user looks; no dead buttons without a reason.
- **Landmines**: nothing may show during onboarding from the updater (`shouldShowUpdateToast()`); keep it. The crash
  dialog copy rules (`crash-copy.ts`) don't apply because nothing renders.
- **Test plan**: component tests per surface and outcome; a11y tests for new states; one Playwright spec using
  `CMDR_MANAGED_PREFS_FILE` that opens Settings › Updates & privacy and asserts the locked rows and notes (read
  `test/e2e-playwright/CLAUDE.md` first).
- **DONE**: each telemetry and update key is visible and explained everywhere a user would look.
- **Implementation notes** (M6 as built):
  - Decision 5 is in: a read-only "Managed by your organization" card (`managed-policy/ManagedPolicySummary.svelte`,
    worded by the pure `policy-summary.ts` from `ManagedPolicyView`) at the TOP of Settings › Updates & privacy, shown
    only on a managed Mac and hidden while a search is active (a static search entry would hit on every unmanaged Mac).
    Updates & privacy over About: it's the page that already holds most locked rows and the section line, so a help-desk
    person lands on the summary and the locked rows together; About is a compact modal about the build and license.
  - Under `DisableUpdates` the Check for updates button is disabled up front with the managed sentence as its status
    line (`aria-describedby`), rather than inviting a press that can only say so. The ceiling sentence still comes from
    a check (`updateState.managed`), as M3 built it; the menu toast already rendered both outcomes, now covered by
    tests.
  - Error-report dialog under `reportsDisabled`: the managed line replaces the explanation, Send and attach-email go,
    and "Save to disk" is the primary button; amend mode shows the line and a lone Close. ❗ `save_error_report_to_disk`
    was `cfg(debug_assertions)`, which would have made "You can still save one to disk" false in release, so it's now a
    command in every build (local only; the file is `error-report-<timestamp>.zip` in the app data dir).
  - `StepBeta` keys off `isSettingLocked('analytics.enabled')` / `('updates.crashReports')`, never the view's bools.
    With usage stats pinned off, the row shows the managed line and keeps disclosing on-by-default crash reports
    (`crashReportsNoteAlone`) unless those are pinned off too; with only crash reports pinned off, the tip drops its
    "crash reports are on too" sentence.
  - E2E: the macOS lane gives every shard `CMDR_MANAGED_PREFS_FILE=<data dir>/managed-prefs.plist` (absent = no policy),
    and `managed_policy/override_watch.rs` (debug and E2E builds) re-reads it on change, so specs push and remove a
    policy live. `managed-policy.spec.ts` (verified passing on a hand-launched E2E build) skips when the variable is
    unset, which includes the Linux Docker lane for now.

### M7. Feature surfaces: AI

- **Scope**: `AiSection` / `AiCloudSection` (provider control with `cloud` disabled under `LocalOnly`, everything locked
  under `Off`), the provider picker in `src/lib/ai-provider-setup/` (presets whose host `cloud_ai_hosts_allowed` rejects
  render disabled with the reason; a custom or Azure URL is checked when entered), `AiCloudConsentToggle` (locked from
  `CloudAiConsentStatus.managed`), `AskCmdrSection`, `StepAi.svelte` (onboarding skips the AI step under `Off`, and
  under `LocalOnly` on a Mac that can't run local AI, and offers only local or off under `LocalOnly` otherwise), and the
  translate-error toast for `Managed`. The disabled `cloud` option reuses the provider row's existing per-option
  disabled path, plus a visible line under the row (§ Draft copy).
- **Intentions**: the shared provider-setup steps change once and both onboarding and Settings get it
  (`sections/CLAUDE.md`: those controls aren't the section's).
- **Landmines**: a disallowed-host preset still needs its row visible with the reason, not removed. The Azure preset's
  placeholder URL has no real host until the user types one; check after entry. ❌ No API key crosses IPC for any of
  this (the host check takes a URL, never a key).
- **Test plan**: picker tests with a mocked `cloud_ai_hosts_allowed`; consent-toggle locked test; `StepAi` tests per
  policy (including Intel + `LocalOnly`); translate-toast tests for each `ManagedAiRefusal`; one Playwright spec with an
  `AllowedCloudAIHosts` file override.
- **DONE**: each AI key is visible and explained in Settings, onboarding, and the error toasts.
- **Implementation notes** (M7 as built):
  - One copy map, `managedAiRefusalMessage` (`src/lib/managed-policy/ai-refusal.ts`), words every AI refusal; the
    translate toast keeps its generic title and names the rule from `err.managed` in the body.
  - `cloud_ai_hosts_allowed` stays `Vec<bool>`: the picker flow only renders while `ai.provider` can read `cloud`, so a
    refused URL there is always `hostNotAllowed`. `PresetHostVerdicts` asks about every fixed-endpoint preset and
    re-asks on policy change; custom and Azure are judged only once the person's URL is entered.
  - `ProviderSetupController` asks the policy before every connection check (on open too), so a refused preset says so
    with no key and a refused typed endpoint is never probed. A refusal is the new `managed` status, never "can't
    connect".
  - `Select` items gained `disabled` (Ark already honoured it; the type, style, and catalog example were missing).
  - The provider radios and the onboarding step read the `ai.provider` lock through `lockAllowsWrite`, not `ai.mode`.
    Onboarding skips step 2 via `aiStepSkippedFor` (lock + local AI support), never without a lock. Under on-device
    only, onboarding keeps Cloud listed but disabled with the reason, rather than removing it.
  - Ask Cmdr's section shows "Your organization turned off AI in Cmdr." from `ai.mode === 'off'` in place of the "turn
    on a provider" hint.
  - The Playwright spec (`managed-policy-ai.spec.ts`, helpers in `managed-policy-helpers.ts`) is written and typechecked
    but not yet run against a live app: it skips until the per-shard `CMDR_MANAGED_PREFS_FILE` wiring lands. Close-out
    runs it.

### M8. Sample profile, `/trust`, and the drift guard

- **Scope**: publish `apps/website/public/mdm/cmdr-managed-preferences.mobileconfig` (a complete, unsigned profile:
  top-level `PayloadType` `Configuration`, `PayloadScope` `System`, one payload whose `PayloadType` is
  `com.veszelovszki.cmdr` carrying EVERY key with a sensible example value, stable UUIDs, an XML comment per key) and
  `apps/website/public/mdm/com.veszelovszki.cmdr.plist` (the bare key dictionary, for Jamf "Application & Custom
  Settings" / Kandji "Custom Profile" / Intune "Preference file" uploads). `/trust`: a "Central management (MDM)"
  section listing every key (type, effect, precedence, the macOS-only note), linking both files, and replacing the "No
  central administration" gap line in `trust.ts` (keep the `.pkg`/PPPC line) and the :70 sentence. The key list on
  `/trust` renders from a typed `managedPreferenceKeys` array in `trust.ts` (`{ key, type, effect }`), so there's one
  list to guard. A Rust test that `include_str!`s both files and `trust.ts` and asserts all three key sets equal the
  constants in `keys.rs`, and that each example value parses without warnings. ❗ The checker caches each lane by its
  declared `Inputs` (`scripts/check/DETAILS.md` § Input fingerprint cache): add `apps/website/public/mdm/**` and
  `apps/website/src/lib/trust.ts` to the Rust test lane's inputs, or editing only the profile is a cached "pass"
  locally. Wire the files into whatever the website's link checks need. `/trust` also carries (all decided):
  - **The traffic no key turns off, each with why**: license validation (a paid license must be checkable; it sends the
    key and nothing about files), the S3 price list (public prices for the cost estimate; no user data), the CLIP model
    download from Hugging Face (the on-device image search model; a download, nothing uploaded), and feedback / beta
    signup (only when the person sends them). Source of truth: the always-allowed `Egress` arms plus the non-api-server
    list in § Fresh grep.
  - **The local-Ollama recipe**: `DisableCloudAI` blocks `localhost` too, so to allow only a local Ollama or LM Studio,
    leave it off and set `AllowedCloudAIHosts` to `localhost` and `127.0.0.1`.
  - **What the ceiling can't do** (§ Updates): no patch releases arrive once the newest release passes it.
  - **What the heartbeat says on a managed Mac**: effective settings plus one "managed" flag, never which keys are set.
- **Intentions**: an admin can download one file, edit values, and upload it. The guard makes a new key impossible to
  forget in the public docs.
- **Landmines**: coordinate with the `website-copy` agent (it edits `/trust` on this branch): stage only your files, and
  leave copy polish to David. A `.mobileconfig` served as `text/plain` opens in the browser; check whether the website
  host needs a `_headers` entry (`application/x-apple-aspen-config`) and add one only if the mechanism exists. Validate
  both files with `plutil -lint`. Don't sign the profile (MDMs re-sign on upload).
- **Test plan**: the drift-guard test (red first: add a key constant with no profile entry and see it fail);
  `plutil -lint`; website build and its checks.
- **DONE**: the files are published under `/mdm/`, `/trust` documents every key, the gap line is gone, the guard is
  green.
- **Implementation notes** (M8 as built):
  - The guard is `managed_policy/public_docs_test.rs`: the profile's `com.veszelovszki.cmdr` payload (minus its
    `Payload*` keys), the bare plist, and `managedPreferenceKeys` in `trust.ts` each equal `ALL_KEYS`; both files'
    values parse with no warning and match each other. Its three files joined `rustEmbeddedInputs` AND ci.yml's `rust`
    filter (a website-only push would otherwise skip the Rust job in CI too).
  - Example values: `DisableUsageStats`, `DisableCrashAndErrorReports`, and `DisableAutomaticUpdateChecks` on; the rest
    permissive (`false`, `MaxUpdateVersion` `"1"`, a four-entry host list including `localhost`/`127.0.0.1`), so an
    unedited upload means "telemetry off", not "Cmdr frozen".
  - The website host is nginx, not one with `_headers`, so a `location ^~ /mdm/` with a `types` block serves
    `.mobileconfig` as `application/x-apple-aspen-config` and `.plist` as `application/x-plist` (`nginx -t` passes).
  - `/trust` changes beyond the new section: the summary line, the "Turning updates off" and AI paragraphs, and
    `/trust/development` lost their "IT can't yet" sentences; the network rows name the key that turns each one off, and
    the license and S3 rows say why none does. Gaps: "No central administration" and "No central control over AI" are
    gone, "No update control for IT" became "No update channels and no staged rollout", and a new gap names the traffic
    no key covers. The `.pkg`/PPPC gap stays.
  - ❗ Release timing: `/trust` describes the RELEASED app, and the website deploys on every push to `main`. A `DevTodo`
    in the section says to publish it with the first release that ships managed preferences.

### M9. Close-out

- An adversarial conformance review against the invariants below (fresh agent), then a docs audit of every touched
  `CLAUDE.md` / `DETAILS.md`.
- David runs recipe 3 of "Testing without an MDM" once (records in `managed_policy/DETAILS.md` which of the `root:wheel`
  / `cfprefsd` steps were needed, dated), then installs the sample profile (recipe 4) and walks the three groups. Then
  tick the three #118 checkboxes and the `/trust` one.
- **Implementation notes** (M9 conformance fixes, 2026-10-05; these supersede the M4/M7 notes they name):
  - The connection probe follows redirects through the same guard as the LLM client (`client::policy_guarded_redirects`,
    one function).
  - Onboarding's AI step writes nothing when its preselect came from the policy (a stored `cloud` read as `off`) and the
    person didn't pick; `isOverriddenByPolicy(id)` in the settings store says when that's the case.
  - Ask Cmdr's switch reads through `overlay`, as § Architecture said, as a typed
    `AskCmdrSwitch { On, Off, ManagedOff }` (`settings::load_ask_cmdr_switch`). `ManagedOff` (stored on, pinned off by
    `DisableAI`) reads as `WakeReadiness::Off`, so the stored backlog stays (rule 5); only the person's own `Off` purges
    it. The send gate passes a `ManagedOff` on to the slot, which refuses with the organization's reason, never
    `askCmdrOff`.
  - The model download reads the policy with `for_egress()` at start and again before its post-download server start,
    and `spawn_and_track_server` (every start's lowest function) refuses under `DisableAI`.
  - The client backstop's mid-turn refusal stays typed: `AgentLlmError::Managed` →
    `AgentErrorKind::ManagedByOrganization` (supersedes M4's "ends the turn as a provider failure").
  - `start_ai_server` / `start_ai_download` reject with the typed `LocalAiError`; the frontend logs `managed` and
    `cancelled` at info (supersedes M4's "keep `Result<(), String>`").
  - `cloud_ai_hosts_allowed` became `cloud_ai_host_verdicts`, answering `Vec<Option<ManagedAiRefusal>>` (supersedes M7's
    `Vec<bool>` decision).
  - The frontend-supplied update trigger stays trusted, as an accepted residual (`managed_policy/DETAILS.md` § Accepted
    residuals).

## Testing without an MDM

1. **Unit and integration tests**: `FakeSource` for logic, `PlistFileSource` for whole-app tests, the scratch-domain CF
   test for the "user layer doesn't count" rule. No test needs root or a profile.
2. **Dev and E2E runs**: `CMDR_MANAGED_PREFS_FILE=/path/to/policy.plist pnpm dev` (debug builds and the `playwright-e2e`
   feature only).
3. **Real managed preferences, fast** (needs `sudo`, so David runs it in M9; unverified until then):
   `sudo defaults write "/Library/Managed Preferences/com.veszelovszki.cmdr" DisableUsageStats -bool true`, then
   reactivate Cmdr (or rely on the folder watch). If `IsForced` doesn't see it, check the file is `root:wheel` `0644`
   like a profile-written one, then `sudo killall cfprefsd` and retry. Record which of these were needed. Undo with
   `sudo defaults delete "/Library/Managed Preferences/com.veszelovszki.cmdr"`. On an MDM-enrolled Mac, `mdmclient` may
   rewrite that folder; use a spare or unenrolled Mac.
4. **The real thing**: open the published `.mobileconfig`, approve it in System Settings › General › Device Management,
   confirm with `sudo profiles show -type configuration` and
   `defaults read "/Library/Managed Preferences/com.veszelovszki.cmdr"`, then remove it in the same pane. This is what
   an MDM delivers, minus the MDM.

## Decisions for David

1. **Feedback.** Should `DisableCrashAndErrorReports` also block in-app feedback (`send_feedback`)? The plan says no:
   feedback is text the person typed and sent on purpose. A separate `DisableFeedback` key is cheap if an admin asks.
2. **Local AI under `DisableAI`.** The plan blocks the local LLM too ("no AI at all"). The media index's on-device CLIP
   model isn't covered (it's search indexing, not a generative feature). Agree, or should `DisableAI` cover CLIP too?
3. **Optional extras, not in this plan**: a `DisableMCPServer` key, a Jamf JSON schema for a form-based editor in Jamf
   Pro, and "force on" keys for pre-configuring a corporate AI gateway (`CloudAIBaseURL` plus pre-approved consent).
   Each is a clean add-on later.
4. **Copy**: the draft lines in § Draft copy.
5. **A "what your organization manages" summary for IT to verify.** #118 asks that IT can "see that the app honors it".
   Today that's per-row notes plus `defaults read`. A short read-only list in Settings › Updates & privacy (or About)
   rendered from `ManagedPolicyView` ("Usage stats: off. Updates: up to 0.52.") is a few lines on top of M5 and gives a
   help-desk person one place to look. Recommendation: add it to M6; skip if you'd rather keep Settings quieter.

Decided by the lead (2026-10-05), now baked into the plan: loopback stays blocked under `DisableCloudAI`, with the
`AllowedCloudAIHosts` recipe on `/trust` (§ AI, M8); the heartbeat reports effective values plus one coarse
`managedByOrganization` bool, never which keys are set (M2); `/trust` lists the traffic no key turns off, with why (M8).
A `DisableNonEssentialNetwork`-style key for that remaining traffic stays a later add-on.

## Invariants (the close-out review checks each)

1. No path sends usage stats, crash reports, or error reports while the matching key is forced on.
2. No update request (check, download, install) happens under `DisableUpdates`; nothing above the ceiling installs.
3. No LLM request reaches a cloud host under `DisableAI` / `DisableCloudAI`, or a host outside `AllowedCloudAIHosts`
   (redirect hops included); no local model download or server start under `DisableAI`.
4. Only forced (managed) values count; the user layer of the domain never does.
5. No key can enable anything; a malformed key restricts as much as it can.
6. Nothing writes a forced value into `settings.json` or the consent record.
7. Every egress gate reads the policy fresh; a profile change while running applies at the next send.
8. Key names are spelled once in Rust (`keys.rs`); the sample profile, plist, and `/trust` match it (drift-guard test).
9. Every refusal is typed; no string matching on messages.
10. The frontend never decides policy: it renders `get_managed_policy`, `locked_settings`, and typed outcomes.
11. No caller can observe the "not loaded yet" policy; `current()` is never fail-open.
12. Every egress gate lives in the lowest send function (`server_request::send` with its required `Egress`, the
    `ai/client.rs` request functions against `AiBackend::destination`), not only in the commands above it.
13. Each policy decision is made in one function (`ManagedPolicy::allows(Egress)`, `ManagedPolicy::ai_destination`,
    `locked_settings`); every other site calls it and never re-derives the rule.

## Review round 1

Fresh-eyes review against the code at `f6ef6d051` (2026-10-05). Changes, ranked by what would have bitten:

1. **Ask Cmdr mid-turn hole.** The backend is resolved once per turn and reused across the tool loop, so rule 6 ("the
   next send") didn't hold for it, nor for `resolve_backend_with_model`'s second config read. Added the per-request
   backstop in the three `ai/client.rs` request functions, and the general rule that gates live in the lowest send
   function (`post_crash_report`, `upload`, `send_amend`, heartbeat, updater fetches), with a shared
   `ServerRequestError::BlockedByPolicy`.
2. **Redirects broke invariant 3.** The plan accepted that an allowed host can redirect anywhere while invariant 3
   promised no request reaches a disallowed host. Now enforced per hop with a custom redirect policy (genai 0.6.5
   `with_reqwest`).
3. **Ceiling let prereleases through.** `0.53.0-rc.1 < 0.53.0` in semver, so a `"0.52"` ceiling would have allowed 0.53
   prereleases. Compare on the release core. Also: integer accepted, real rejected, grammar spelled out.
4. **The staged-version check couldn't be built.** `download_update` takes the URL from the frontend and the backend
   never learns the version. M3 now stores the offered `UpdateInfo` and stops trusting a frontend URL.
5. **`UpdateRule` couldn't express ceiling + no automatic checks**, the most common IT combination. Reshaped to
   `UpdatePolicy`.
6. **Fail-open startup window.** `current()` returned `Default` (no restriction) until setup loaded it, and the crash
   next-launch path runs before `load_settings`. Now lazily initialized. Also resolved the "CF off the main thread" vs
   "read in setup" contradiction.
7. **Activation-only refresh** missed an MDM push while Cmdr stays frontmost (long agent turn, model download). Added a
   `/Library/Managed Preferences` watch, Chromium's approach, with citations.
8. **Value parsing**: string bools (`defaults write` without `-bool` writes strings), host normalization (IPv6 brackets,
   trailing dot, IDNA, pasted URLs, ports), unknown keys ignored.
9. **Fresh grep gaps**: `crash_reporter/pending_delivery.rs`, `error_reporter::upload`, `auto_sent::send_amend`, the
   `ai/client.rs` request functions, the `resolve_backend*` callers, the real `cloud-providers.ts` path, and the egress
   no key covers (now also listed on `/trust` in M8).
10. Smaller: explicit domain constant (not `kCFPreferencesCurrentApplication`), the override works on Linux E2E,
    `apply_change` cancels model downloads and newly-disallowed-host work, test-recipe permission hint, decisions 5–7.

## Review round 2

Implementer and user angle, against the code at `6b4bd4a2c` (2026-10-05). Changes, ranked:

1. **The api-server gate had five copies; now it has one.** `server_request::send` already carries six of the senders,
   so it takes a required `Egress` and makes the call. A new sender can't compile without naming its pipeline, and the
   always-allowed arms are the code form of the `/trust` "no key turns this off" list. The heartbeat moves onto it.
2. **The LLM backstop couldn't be built.** `AiBackend` doesn't know its URL, or whether it's the local server, so the
   client check had nothing to compare. It gains `destination`, and one `ai_destination` decision serves both
   `resolve_backend` and the backstop. Without that, `DisableAI` against a running local server had no backstop.
3. **The ceiling promised patch releases it can't deliver.** `latest.json` only names the newest release, so a `"0.52"`
   ceiling stops all updates once 0.53.0 ships. Text corrected, and `/trust` now says it.
4. **Backend readers saw stored values.** `analytics.enabled`, `askCmdr.enabled`, and the heartbeat config shape read
   `settings.json` directly. A Rust `locked::overlay` (the twin of the frontend one) feeds all three. That's how the
   heartbeat reports effective values, and `ManagedOff` in `send_permission` isn't needed.
5. **Four AI refusal spellings became one** `ManagedAiRefusal`, carried by every surface and given one frontend copy
   map.
6. **M1's DONE needed `sudo`**, so no agent could tick it. The real-plist recipe moves to M9 (David).
7. **The drift guard would cache-pass locally.** The checker fingerprints declared inputs, so the website files join the
   Rust lane's inputs. `trust.ts` gets a typed key list, which the guard also covers.
8. **UX**: a section line so keyboard and VoiceOver users meet the reason before the disabled controls, a visible line
   for the disabled `cloud` option (a tooltip alone isn't announced reliably), no toast for a held update, Intel plus
   `LocalOnly` handled, managed note wins over a section's own note, `install_update` gets a typed error, and Ask Cmdr's
   open question is closed (it runs on local AI).
9. Lead decisions baked in (loopback, heartbeat, remaining traffic); decisions 5–7 are gone, and a new decision 5 asks
   about a "what your organization manages" summary.
