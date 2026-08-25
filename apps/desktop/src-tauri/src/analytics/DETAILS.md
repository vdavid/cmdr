# Analytics: details

Depth behind the must-knows in `CLAUDE.md`.

## Wiring

`analytics::init(app.handle())` + `analytics::start()` run from `lib.rs` setup (mirroring `space_poller`): `init`
stores the app handle, `start` spawns the loop (one beat on launch, then hourly). `install_id::init()` runs earlier in
setup (before the crash reporter) so it can snapshot the diag id for the panic hook.

## The suppression gate: what counts as a real install

`suppression_reason()` in `mod.rs` is the ONE gate; the heartbeat loop and `posthog::capture` both call it, so the two
pipelines can never disagree about whether an install is real. It returns `Some(reason)` (a named condition, logged at
debug) when this process must not send, `None` when it may. `CMDR_ANALYTICS_FORCE=1` overrides every condition, which
is what lets an integration test drive the loop against a localhost Worker.

Suppressed when the build is a debug build, OR when any of `NON_PROD_ENV_VARS` is present in the environment: `CI`,
`CMDR_INSTANCE_ID`, `CMDR_DATA_DIR`, `CMDR_E2E_MODE`, `CMDR_MOCK_FDA`.

Presence, not value: `CMDR_E2E_MODE=0` still means a harness composed this environment, and failing closed costs
nothing.

### Why an isolated instance must never send

**The constraint the code has to defend: a fresh data dir mints a fresh `anal_` install id, so any tooling-launched
instance that reaches production analytics registers as a brand-new user, every single launch.** Judging "is this
real?" by build mode alone missed that entirely: the E2E, i18n-capture, and marketing-shot lanes all drive
release-mode binaries. Between 2026-06-10 and 2026-08-24 that produced 1,786 phantom installs against 303 real ones,
inflating installs by 6x and daily actives by roughly 24 a day. `CI` was set only on the CI runners, and the macOS
Playwright lane runs locally.

So the gate asks "is this environment one a real user's launch could produce?" rather than "is this a dev build?".
A production launch (Finder, Dock, Spotlight, the updater's relaunch) sets none of the five; every launcher in
`docs/tooling/instance-isolation.md` sets at least one. `CMDR_INSTANCE_ID` and `CMDR_DATA_DIR` carry the weight, since
they're precisely the vars that redirect the data dir that mints the id.

`every_tooling_launcher_is_recognized` pins the exact env each launcher stamps (E2E checker, i18n capture, marketing
shots, dev wrapper). A launcher that stops tripping the gate fails that test rather than quietly minting installs
again.

The five live in `apps/desktop/src-tauri/src/prod_instance.rs` rather than here, because the macOS updater's
update-check gate reads the same list (`updater::skip_reason`). Keeping one definition is what stops the
`update_checks` ceiling and the `heartbeat` floor on the dashboard from counting different populations.

### Cleaning up the rows that already landed

The stored history is corrected server-side by a daily D1 sweep that deletes the beats of installs which never
persisted a setting. Canonical: `apps/api-server/DETAILS.md` § Synthetic heartbeats.

## Install-id storage and signal-safe read

The ids resolve their data dir without an `AppHandle` (mirroring `settings/loader.rs`'s `early_load_*`), so the no-arg
accessors work from the panic hook, next-launch crash assembly, and the analytics loop alike. `diagnostics_id()`
allocates and locks, which is unsafe in the async-signal-safe crash signal handler, so `install_id::init()` snapshots
the diag id into a `OnceLock` that the panic-hook path reads via `diagnostics_id_snapshot()`; the signal path attaches
the diag id later, at next-launch assembly, where the full stdlib is available.

## Why two ids (the GDPR reasoning)

If two datasets *can* be joined, treat them as joined, so we make them genuinely unjoinable. With a separate `diag_` id,
an attached email links only to the diagnostics stream; the analytics stream stays unjoinable to any identity. The
`anal_`/`diag_` prefixes make the ids self-identifying in payloads, PostHog, and the D1 tables.

## Heartbeat payload

`HeartbeatPayload` (camelCase on the wire, `Option::None` → `null`) matches the Worker's validator:

- `analId` (required): `anal_` + lowercase hyphenated v4 UUID, `^anal_[0-9a-f-]{36}$`.
- `appVersion` (required): semver from `CARGO_PKG_VERSION`.
- `osVersion` (required): from `crate::platform::os_version()`, always non-empty.
- `arch` (required): `std::env::consts::ARCH`.
- `buildMode` (optional): `"release"` / `"debug"`.
- `config` (optional): the config-shape object, verbatim.

Fire-and-forget POST mirroring the crash/error reporters (10 s timeout, errors logged at debug, next hourly tick
retries). Endpoint: `http://localhost:8787/heartbeat` (debug) / `https://api.getcmdr.com/heartbeat` (release).

## PostHog `/capture/` body and key mechanism

`posthog::capture(event, props)` builds the body and fire-and-forget POSTs to `https://eu.i.posthog.com/capture/` (EU
cloud, project `136072`). Shape:

```json
{ "api_key": "phc_...", "event": "<name>", "distinct_id": "anal_<uuid>",
  "properties": { "source": "desktop", "app_version": "0.39.0", "os_version": "macOS 26.0",
                  "arch": "aarch64", ...props },
  "$set": <config-shape> }
```

- **`$set` is the config-shape verbatim**: person properties reuse `config_shape::build_config_shape` (same allowlisted
  object the heartbeat ships), so there's one source of truth and no second PII surface.
- **An absent key is not "the default"**, on either transport. `settings.json` is sparse (only explicitly-set keys are
  written), so the shape carries deviation, never adoption. Reading it as adoption needs the per-version defaults
  manifest the dashboard resolves against: `apps/analytics-dashboard/DETAILS.md` § Settings adoption.
  `CATEGORICAL_STRING_KEYS` is an input to that manifest, so adding a key here widens what the dashboard can answer.
- **`source: "desktop"`** is injected first and can't be shadowed by a caller `source` prop, so the dashboard always
  splits desktop events from website events.
- **The `EventIdentity` trio rides every event**: `app_version` (`CARGO_PKG_VERSION`, the same string the heartbeat
  ships), `os_version` (`crate::platform::os_version()`), and `arch` (`std::env::consts::ARCH`), injected alongside
  `source` and equally unshadowable (`injected_identity_cannot_be_shadowed_by_props`).

  **Why on the event and not only in `$set`.** PostHog person properties are last-write-wins, so the config-shape and
  the heartbeat's per-install identity both answer "what is true now", never "what was true when this event fired". An
  event without its own `app_version` is therefore uninterpretable the moment a release changes what an event means: of
  406 `search_used` events over 90 days, 265 carried only `mode` and none of the richer props, almost certainly from
  builds predating the richer event, and nothing in the data could say so. `os_version` and `arch` are there for the
  same reason at the same cost (three low-cardinality strings): a platform-specific regression shows up as a version
  mix otherwise.

  All three are categorical and PII-free, so they need no exemption from the prop rule.
- **The key is `option_env!("CMDR_POSTHOG_KEY")`**, baked at build time (a GitHub secret on the `tauri-action` step in
  `release.yml`; `build.rs` has a `rerun-if-env-changed` for it). `None` locally → `capture` is a no-op (logged once at
  debug). The key is public by design (PostHog ingest keys are safe in client code).

## The `track_event` IPC

Frontend events call the `track_event` IPC (`commands/analytics.rs`), a thin pass-through to `capture`. It takes
`props_json: String` rather than a structured type because the prop set is open and `serde_json::Value` can't cross the
specta IPC boundary; the frontend's typed `trackEvent` wrapper `JSON.stringify`s the props. A malformed or non-object
`props_json` degrades to no props (the event still fires with `source: "desktop"`).

## How to add an event

Open set, no enum, no schema:

- **Backend event**: at the success chokepoint,
  `crate::analytics::posthog::capture("my_event", serde_json::json!({ "kind": some_enum }))`.
- **Frontend event**: `import { trackEvent } from '$lib/tauri-commands'`, then `void trackEvent('my_event', { kind: someEnum })`.
- Name internals after the UI; keep props categorical.

## Reading a zero (before calling it a bug)

An event with no data in PostHog is the normal way an instrumentation bug shows up, and also the normal way an unused
feature shows up. Walk these in order; each one is cheap and rules out a whole class.

1. **Did it ship?** An event only fires from a released binary. Compare the emitter's commit date against
   `git for-each-ref --sort=-creatordate refs/tags`, and confirm directly against the shipped app by looking for the
   event name in `strings -a /Applications/Cmdr.app/Contents/MacOS/Cmdr`. That reads the RUST binary only: a frontend
   event's name lives in the embedded webview bundle and won't appear there, so a hit proves presence and a miss only
   means something for a backend event. `properties.app_version` rides every event, so PostHog can also answer "has
   this ever fired from a build at or after X".
2. **Is the feature reachable at all for most installs?** Anything under `agent/` needs a configured AI provider, and
   the person-property split (`person.properties['ai.provider']`, `off` / `local` / `cloud` / unset) is a small
   denominator. A tiny population plus a short window explains a lot of zeros honestly.
3. **Is there a denominator event above it?** A feature whose funnel starts at its LAST step can't distinguish "unused"
   from "broken". That's why `ask_cmdr_turn` exists above the three `suggestion_group_*` events. If the layer you're
   looking at has no such event, the missing event is the finding.
4. **Only then look for a swallowed call.** Read every direct caller of the store or sink the wrapper wraps, and check
   that nothing bypasses the instrumented function. A local reproduction can't confirm the capture:
   `suppression_reason()` suppresses debug builds and every isolated data dir, so `capture` is a no-op in dev unless
   `CMDR_ANALYTICS_FORCE=1` is set.

The one check that catches this class before it costs a day is `analytics-event-catalog`, which pins the list below
against every emitter in the tree, in both directions (`scripts/check/checks/DETAILS.md`).

## Starter event set (PII-free; grows over time)

Backend events fire at success chokepoints; frontend events ride `track_event`.

- `app_launched` (backend, `lib.rs` setup): no props.
- `pane_navigated` (frontend, `file-explorer/pane/listing-loader.ts`, at the end of a successful load): `volume_kind`,
  the pane's `VolumeKind`; never the path. In practice it's `local` / `smb` / `mtp` / `archive`. The union has two more
  members, `network` (the SMB-browser virtual volume) and `search-results` (the snapshot virtual volume), and this
  event can NEVER carry either: `FilePane.svelte` skips `loader.loadDirectory` for both, so a virtual pane never
  reaches the emit. They're live kinds elsewhere (they drive capabilities), just not reachable here. If a virtual pane
  ever gains a real listing pipeline, this line is the one to revisit.
- `search_used` (frontend, `$lib/search/search-run-tracking.ts`, once per run when the run ENDS): `mode`
  (`filename` / `regex` / `ai`, the `SearchMode` union), `trigger`, `ending`, `coverage`, `duration_bucket`,
  `abandoned_ground`, `capped`; never the query. See below.
- `search_cta_offered` / `search_cta_used` (frontend, same file): `cta` enum. See below.
- `select_files_used` (frontend, `SelectionDialog.svelte` `commitMatches`): `mode` (the same `SearchMode` union:
  `filename` / `regex` / `ai`) + `action` (add/remove); never the pattern.
- `file_transfer_completed` (backend, `write_operations/analytics.rs` `emit_completion_analytics`): `op` (copy/move),
  `item_count` bucket, `had_conflicts` bool (proxied from `files_skipped > 0`); never names/paths.
- `delete_used` (backend, same function): `trashed` bool, `item_count` bucket.
- `archive_edit_completed` (backend, same function): `item_count` bucket.
- `rename_used` / `folder_created` / `file_created` (backend, `write_operations/analytics.rs`, emitted from the
  `rename_managed` / `create_directory_managed` / `create_file_managed` wrappers): `initiator` (the `Initiator` token:
  `user` / `ai_client` / `agent` / `agent_edited`), `target` (`volume` / `archive`), `outcome` (`done` / `failed`);
  never a name or a path. These three are INSTANT metadata ops (`manager::run_instant`) that return their `Result`
  inline and produce no `WriteCompleteEvent` at all, so they can't ride `emit_completion_analytics` — its arms for them
  are unreachable, and stay explicit only so a future PROGRESSED op type fails to compile rather than skipping
  analytics. Each driver is WRAPPED rather than emitted inside, because the in-archive route is an early return: a
  per-branch emit would count the filesystem renames and miss every in-zip one. On the `archive` target, `done` means
  the managed edit STARTED (its completion rides `archive_edit_completed`).
- `smb_connected` (backend, `crates/cmdr-smb/src/volume/mod.rs` `connect_smb_volume`): no host/share/credential props.
- `sftp_connected` (backend, `crates/cmdr-sftp/src/volume/mod.rs`): no host/account/port/path props.
  Both connection events go through the `AnalyticsSink` seam rather than `capture` directly, since the backend crates
  can't see `tauri` (`volume_sink.rs`).
- `mtp_connected` (backend, `mtp/connection/mod.rs` `connect`): no device/product props.
- `tag_toggled` (backend, `file_system/tags.rs` `toggle_color`, at its single exit): `action` (`applied` / `removed`),
  `color` (the Finder palette's canonical name, lowercased: a closed set of seven), `item_count` bucket, `succeeded`
  bool; NEVER a tag's own text, which is user-authored content. `toggle_color` is the one op behind all three triggers
  (the seven keyboard commands, the context-menu circles, and the MCP `tag` tool), so instrumenting it covers them
  without any caller having to remember; the cost, accepted deliberately, is that the event can't say which trigger
  fired. macOS-only, like Finder tags themselves. `succeeded` covers the partial-write case, where an earlier file kept
  its new tags and a later one didn't.
- `favorite_changed` (backend, `favorites/store.rs` `mutate_and_persist`): `action` (`added` / `removed` / `renamed` /
  `reordered`) + `favorites`, the list's size AFTER the change as an `item_count_bucket`; never a path or a label. It
  sits past the no-op guard, so removing an id that isn't there doesn't inflate the count. The `action` is a required
  parameter rather than an inferred one, so a fifth favorites mutation can't be added without deciding what it reports.
- `viewer_opened` (backend, `file_viewer/analytics.rs`, from `session::open_session_inner`): `content` (`text` /
  `image` / `pdf`, or `unknown` on a failure), `size_bucket` (`<1MB` / `1-10MB` / `10-100MB` / `100MB-1GB` / `1GB+`,
  stepping where the viewer's own backends step), `outcome` (`opened` / `failed`), `failure` (the `ViewerError`
  variant's token, or `none`), `from_archive` and `forced_text` bools. ❌ NEVER a file name, an extension, or a byte
  count: an extension list fingerprints a person's work in a population of a few hundred installs. Every open passes
  `open_session_inner` (F3, the "View as text" override, preview-inside-a-zip), and it's wrapped rather than emitted
  inline because the media path is an early return and the text path has a dozen `?`s.
- `update_check` (frontend, `$lib/updates/update-analytics.ts`, once per finished check, from every exit of
  `checkForUpdates()`): `trigger` (what set it going: `startup` / `poll` / `auto_check_on` / `command` / `settings`),
  `outcome` (`up_to_date` / `staged` / `already_staged` / `blocked` / `failed`), `failure` (the
  typed kind, or `none`: `check` / `download` / `install` for a phase that didn't get there, `translocated` /
  `read_only_volume` for a bundle that can't be written), and `staged_version`, the release sitting in the bundle
  waiting for a restart (one of our own release numbers, or `none`). ❌ Never a URL, a bundle path, or the text of a
  failure. `trigger` is `checkForUpdates()`'s required first parameter, with no default, so a sixth entry point has to
  decide what it reports; without it a run of manual checks (someone hunting for a fix) and the background loop ticking
  are one number. Without this the update path is invisible between "the install asked" (the `update_checks` row the manifest
  proxy writes) and "the install reports a version", so "everyone is current", "everyone has a build staged they never
  restart into", and "the install fails on every one of them" all read the same. `already_staged` is the one that
  answers the middle case directly: a rising count of it against a flat `staged` IS the stuck population.
  `failure: install` never separates from `download` on the fused non-macOS plugin path, which is fine (macOS is what
  ships). `$lib/updates/DETAILS.md` § What a check reports.
- `session_reached` (backend, `analytics/session.rs`): `milestone` (`1m` / `5m` / `15m` / `1h` / `4h` / `12h` / `24h`).
  The session-length signal, and the reason there is no `app_quit`. See below.
- `ask_cmdr_turn` (backend, `agent/chat/runtime/analytics.rs`, at `run_turn`'s single exit AND on the pre-turn
  refusal path): `origin` (`text` / `wake` / `outcomes` / `resume`), `outcome` (`answered` / `cancelled` / `failed` /
  `refused`), `failure` (the `AgentErrorKind` / `AgentErrorKindView` token, or `none`), `provider` (the `ProviderTag`
  token, or `unresolved` on a refusal), `tool_turns` + `proposals` buckets; never a prompt, a reply, or anything a tool
  read. `refused` covers the four gates that answer before a turn exists (no store, no consent, no resolvable provider,
  a local window under the floor), so the funnel has a top as well as a middle. This is the agent funnel's DENOMINATOR: the three `suggestion_group_*`
  events below only become readable against it, because a zero on them otherwise can't be told apart from a feature
  nobody uses. `agent/chat/DETAILS.md` § The turn event.
- `agent_wake` (backend, `agent/wake/runner.rs` `record_outcome`): `outcome` + `tier` tokens, `folders` + `proposals`
  buckets. Counts every wake OUTCOME, including wakes that never opened a turn, so it is a WIDER population than
  `ask_cmdr_turn`'s `origin: "wake"`; the gap between them is the point.
- `suggestion_group_proposed` / `suggestion_group_approved` / `suggestion_group_rejected` (backend,
  `agent/suggested_ops/analytics.rs`): `verb` (the `ProposalVerb` token) + `op_count` bucket. Acceptance rate is the
  agent's north-star metric, which is why the proposal and both outcomes are all counted; never a path, file name,
  rationale, or selector pattern.
- `tab_opened` / `tab_closed` / `tab_switched` / `tab_pin_toggled` (frontend, `file-explorer/tabs/tab-analytics.ts`,
  called from `file-explorer/pane/tab-operations.ts`): `source` (`new` / `reopened`, or `single` / `others` on a
  close), `outcome` (`opened` / `atCap` / `nothingToReopen`; `closed` / `cancelled` / `lastTab`), `open_tabs`, a
  `pinned` bool on the close and the pin toggle, and `method` (`cycle` / `pick`) on the switch. Never a path, which is
  a tab's whole identity.
  **`open_tabs` is a RAW count, the one documented exception to `item_count_bucket`**: a pane caps at ten tabs, and
  that ladder has two values (`1`, `2-10`) across the entire range, so bucketing would throw the answer away for no
  privacy gain. Ten possible integers identifies nobody.
  `tab-operations.ts` is the emit layer because every trigger funnels through its exports (the tab bar, the File menu,
  the keyboard, the palette, and the MCP `tab` tool); the pure `tab-state-manager.svelte.ts` beneath it is deliberately
  left alone, since it's exercised directly by unit tests. The refusals are counted for the same reason
  `search_cta_offered` is: a success-only count can't tell "nobody reopens tabs" from "everybody hits the cap trying".
- `quick_look_used` (frontend, `command-handlers/file-handlers.ts` `file.quickLook`): `outcome` (`opened` / `closed` /
  `noTarget` / `insideArchive`). One event for the whole toggle, with its gate counted: a preview inside a `.zip` is
  refused (the inner path isn't a real file), and without that arm a low `opened` number can't be told from people
  reaching for it where it can't work. The duplicate fire of one Shift+Space — AppKit's menu accelerator plus the
  webview keydown — is swallowed by the dispatch guard BEFORE the emit, so it can't double every number here.
- `editor_opened` (frontend, same file, `file.edit`): no props. F4 hands the file to the OS's text editor (`open -t`),
  so nothing downstream can count it, and the file's name and extension are exactly what must never cross.
- `drop_received` (frontend, `file-explorer/drag/drag-analytics.ts`, from `pane/drag-drop-controller.svelte.ts`
  `handleDrop`): `origin` (`self` / `external`), `outcome` (`transfer` / `noTarget` / `samePane` / `selfDescendant`),
  `op` (`move` / `copy`), `item_count` bucket. **This is what makes drag readable as an INPUT PATH**:
  `file_transfer_completed` can't say how an operation was started, because by the time it settles nothing remembers,
  so `drop_received{outcome: transfer}` against the transfer total is the split between dragging and the keyboard.
  All three refusals report, because a drop that lands nowhere feels identical to one that works.
- `drag_out_completed` (frontend, same module, from `drag/drag-out-event-bridge.ts`): `item_count` bucket + `outcome`
  (`done` / `partial`). Per drag SESSION, matching the toast — one gesture is one drag however many files it carried.
  Counted at the drain rather than at the start, because a promise-backed drag (MTP, a NAS) can be abandoned before
  anything fulfills. ❌ The payload's `failures` holds leaf NAMES; only its length crosses.
- `favorite_opened` (frontend, `file-explorer/navigation/favorites-analytics.ts`): `surface` (`breadcrumb` /
  `command`). The payoff half of favorites — `favorite_changed` counts the list being edited and can't say whether
  anybody ever goes anywhere with it. Two call sites rather than one, and there's no lower chokepoint: both fold onto
  `navigate({ to: { selectVolume } })`, which by then holds the CONTAINING volume's id and can't tell a favorite from
  a drive.
- `settings_opened` (frontend, `$lib/settings/settings-window.ts` `openSettingsWindow`): `surface` enum (`command` /
  `ipc` / `crash-toast` / `error-toast` / `wake-indicator` / `paste-toast` / `enter-menu` / `volume-breadcrumb` /
  `downloads-toast` / `low-disk-toast` / `shortcut-chip` / `quick-look-toast`); never the section. It sits in the
  window helper every entry point funnels through, so it counts all dozen of them and covers a new one for free. Why
  `surface` is a required first param and why `section` stays out: `apps/desktop/src/lib/settings/DETAILS.md` § "Every
  open funnels through `openSettingsWindow`".
- `error_encountered` (backend, `listing/streaming.rs` `TauriListingEventSink::emit_error`): `category` enum (from the
  ListingError); never the path/message/provider.
- `first_index_started` / `first_index_home_covered` / `first_index_completed` (backend, `first_index.rs`): no props on
  the first, a `duration_bucket` on the other two. See below.
- `first_folder_size_shown` (frontend, `$lib/indexing/first-size-timing.ts`, once per launch): `seconds_bucket` +
  `covering` bool. See below for its population.
- `language_resolved` / `language_changed` (frontend, `$lib/intl/language-analytics.ts`): base language subtags only.
  See below.

## Session length, in detail

**There is no `app_quit` event, on purpose.** A quit event is unreliable exactly when it matters: a crash, a
force-quit, a power cut, and a `SIGKILL` all end a session with no moment left to report in. A length counted at the
end therefore drops its most interesting cases and reads back longer than the truth. It's the same trap the
first-index interruption rate documents below, and it gets the same answer: count what a session REACHES, and let the
absence of the next rung be the ending.

`session_reached` is that ladder. One task per launch sleeps to each rung in turn (1m, 5m, 15m, 1h, 4h, 12h, 24h) and
fires, then exits at the top; the task dies with the process, so an ending needs no code and can't be missed. Each rung
is monotone — once sent, nothing retracts it — so the distribution of the TOP rung per launch is a survival curve, with
`app_launched` as its zeroth rung and denominator. Seven events per launch is the whole cost, and a session parked for
a week costs nothing after the first day.

**It measures the app being OPEN, not the person being at the keyboard.** A Cmdr left running overnight climbs the
whole ladder. Telling "using" from "open" needs input or focus tracking, and watching when someone touches their
keyboard is a bigger intrusion than the question is worth, so the top rungs read as "leaves it running", never as
"worked for twelve hours".

❌ Adding a rung in the middle later is a schema change, not a free one: it shows up in the data as a behavior change
in the survival curve rather than a schema one. Say so here if it happens.

## The language events, in detail

Two questions: which language does an install run in, and does auto-selection land somewhere the user wants to stay?
Both events live in `$lib/intl/language-analytics.ts`.

- `language_resolved` fires once per launch, from `settings-applier.ts::initSettingsApplier` right after the settings
  apply, so `active` is what the user is actually looking at rather than the webview's guess while the OS answers were
  in flight. Props: `detected` (what the Rust resolver matched in the OS preference list against the shipped catalogs,
  or `none`), `active` (what the app runs in), and `source` (`explicit` when `appearance.language` names a tag, `auto`
  when it's `system` and a catalog matched, `fallback` when it's `system` and nothing did).
- `language_changed` fires per deliberate pick, from the two pickers (`OnboardingLanguagePicker` and
  `AppearanceSection`'s row) through `SettingSelect`'s `onPicked`. Props: `from` (the language they left) and `surface`
  (`onboarding` / `settings`).

**Both send the BASE subtag only** (`hu`, never `hu-HU`; `pt-BR` → `pt`). A rare language plus a region narrows a
population further than the question needs, and the base subtag answers it completely.

**`language_changed` is the only quality signal we get.** Nothing in the UI asks how a translation reads, and nothing
will (David: no machine-translation notice anywhere), so a user walking away from their own language is the strongest
evidence that a locale is bad. That's what makes `from` load-bearing, and it's why two things are the way they are:

- ❌ **The hook is the pick, never a settings subscription.** `SettingSelect` writes the setting on every HIGHLIGHTED
  row (the live preview, keyboard and hover alike) and the store mirrors it into every open window, so a subscription
  would report each row the user skimmed past, from each window at once.
- **A pick that lands on the language already running sends nothing.** Pinning "System default (Magyar)" to an explicit
  `hu` is not a walk-away, and counting it as one would put phantom evidence against Hungarian.

The `from` of the first pick comes from a per-window seed: the main window's is set by `language_resolved`, and
secondary windows (the Settings window hosts the picker and never sends that event) seed through `noteStartupLanguage()`
in `initWindowLanguageSync`. First seed wins, so a late startup seed can't rewrite a pick that already happened.

The setting itself also rides the heartbeat: `appearance.language` is on `config_shape.rs`'s `CATEGORICAL_STRING_KEYS`,
since its vocabulary is a fixed set of shipped tags plus the `system` sentinel.

## The search events, in detail

Search is the one feature whose interesting question is not "did somebody use it?" but "did it have to walk, how long
did that take, and did they stay for it?" — since a search now covers unindexed ground by walking it
(`docs/specs/unindexed-search-plan.md`). The vocabulary is minted in `apps/desktop/src/lib/search/search-analytics.ts`, a pure module
that cannot see a query, a pattern, or a path.

**`search_used` fires ONCE per run, when the run ends.** Firing at the start would leave every one of those questions
unanswerable. The props:

- `mode` — the dialog's own mode enum (`filename` / `regex` / `ai`). Typed as `SearchMode` on `SearchRunFacts`, not
  `string`, so adding a mode is a compile error rather than a silent new value in the data.
- `trigger` — `run` (Enter or the run button, the path that walks) or `autoApply` (the debounce, which answers from the
  index alone, Decision 7). ❌ Don't fold them together: auto-apply fires on every typing pause, so it would drown the
  deliberate searches in the denominator.
- `ending` — `completed` / `interrupted` (the drive went away) / `cancelled` (Escape or the dialog closing) /
  `superseded`. The cancel rate is `cancelled` over the `trigger: run` total.
- `coverage` — `covered` (the index answered it all) / `live` (nothing was covered, every row came off the walk) /
  `mixed`, from the backend's own `SearchRunCoverage::kind`, or `unknown` for a run that ended before saying.
- `duration_bucket` — `<1s` / `1-5s` / `5-30s` / `30s-2m` / `2m+`. Absent for an index-only run, which answers inside
  one promise: timing it would measure the IPC round trip, not a wait anybody felt.
- `abandoned_ground` and `capped` — the two ways a run comes back short WITHOUT its ending saying so.

**`superseded` is the frontend's own word.** The backend never reports it: a run the user typed past keeps walking
(Decision 11) and no terminal event for it is coming, so the arrival of its successor is the only moment it can be
counted. That's why the run clock starts on the coverage callback's `null` (a run starting) rather than on
`searchFilesStreaming` resolving — a small folder's whole run can arrive before that promise does.

**CTA conversion is two events, not a prop.** `search_cta_offered` fires when the coverage note starts offering one and
`search_cta_used` when it's pressed, both carrying the same `cta` enum (`indexDrive` / `fullDiskAccess`), so conversion
is a ratio per CTA. It can't be one prop on `search_used`: the Full Disk Access offer depends on a TCC probe that
answers AFTER the run does, so an offer counted at settle time would miss every late one and put the rate over 100%.

## The first-index events, in detail

Covering a drive in phases is justified by a user-experience claim, so the four numbers that can falsify it ride the
same pipeline as everything else. Three are backend (`first_index.rs`, off the index's own `IndexEvent`
stream); the fourth is the frontend's, because only it knows what is on screen.

- **`first_index_started`** — a run announced itself with `covered_in_phases`. It is the DENOMINATOR, and that is its
  whole job.
- **`first_index_home_covered`** — `duration_bucket` (`<10s` / `10-30s` / `30s-2m` / `2-5m` / `5m+`) since that start,
  off the `IndexEvent::HomeCovered` report. The claim is that a user's own files answer in seconds, so the first two
  buckets are where it lives or dies.
- **`first_index_completed`** — `duration_bucket` (`<1m` / `1-3m` / `3-10m` / `10-30m` / `30m+`) since that start.
- **`first_folder_size_shown`** — `seconds_bucket` since the frontend booted, plus `covering` (was a phased first index
  running on that drive?). Fires on the first window of rows carrying a real `recursiveSize`, at most once per launch.
  This is the wow moment itself: not "the index finished", which nobody watches, but "I opened a folder and it told me
  how big it is". `covering` is what keeps a machine indexed weeks ago from drowning the measurement in zeroes.

  **Its population is launches that opened a folder**, in either list mode: both `views/full-list-cache.svelte.ts` and
  `views/BriefList.svelte` call the hook, at the two points in each where rows the user is looking at gain sizes. A
  launch that never opens a folder can't fire it, which is the one exclusion and an honest one — there was no moment to
  measure. ❌ If a call site is ever dropped, the population narrows to "launches in the OTHER mode" and nothing in the
  numbers says so, so say it here.

  The comparison the claim rests on is WITHIN the event: `covering: true` (a first index was running on that drive)
  against `covering: false` (the index was already there). That is what makes the wow moment falsifiable, and it needs
  no cross-event denominator at all.

**The interruption rate is a RATIO, not an event.** How often a first index never finishes is
`1 - first_index_completed / first_index_started`. ❌ Don't add a terminal "interrupted" event: a run that ends with the
process (a quit, a crash, a power cut) has no moment left to report in, so anything counted at the end under-counts
exactly the case being measured — and that case is the one the truncate-and-rebuild design lost entirely.

**A phased run is told from every other run by the clock's own presence.** `first_index.rs` keeps a per-volume start
`Instant` only for a run that announced `covered_in_phases`, so the `ScanComplete` of a change check on an
already-indexed drive finds no clock and counts as nothing.
