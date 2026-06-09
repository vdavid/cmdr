# Beta usage analytics: implementation plan

Status: draft for execution. Branch: `beta-analytics`.

This plan is the product of a long design conversation. Read the **Design rationale** section first; it captures the
"why" behind every non-obvious choice so the implementing agent can adapt without re-deriving the tradeoffs. All
user-facing copy is drafted in [Appendix A](#appendix-a-draft-copy-david-reviews); David reviews and modifies it before
merge.

## Goals

1. **Fix the inflated "active devices" number** on `analdash.getcmdr.com`. Today it shows ~217 when the real figure is
   ~10. The cause is not the once-per-hour update check (a secondary factor); the dominant bug is that the dashboard
   **sums per-day active counts across the whole time window** (a 7-day window multiplies a ~10/day figure by ~7, plus
   IP-churn and version/arch fragmentation). See M1.
2. **Add reliable, privacy-clean beta usage tracking**: a true daily-active count, lightweight engagement and preference
   data, curated feature events, and an optional contact channel to early testers, without ever being able to tie a
   person's identity to their usage data.

## Design rationale (the load-bearing "why"s)

### Two identities that never meet, by construction

We mint **two** random per-install identifiers, both generated and owned by Rust, stored in `CMDR_DATA_DIR`:

- **`anal_<uuid>`** (analytics identity): the heartbeat key and the PostHog `distinct_id`. **Never** attached to a crash
  or error report.
- **`diag_<uuid>`** (diagnostics identity): attached **only** to crash and error reports, so sequential reports from one
  install can be grouped ("5 reports, same install" vs "5 different people"). **Never** sent through the analytics
  pipeline.

Why two and not one: a tester can voluntarily attach their email to a report (so we can reply). If reports carried the
_analytics_ id, then email → analytics-id → the install's entire usage history would be joinable on our servers, which
is exactly the linkage we promise not to have. With a separate `diag_` id, an attached email links only to the
diagnostics stream; the analytics stream stays unjoinable to any identity. This satisfies the GDPR principle David
insisted on: **if two datasets _can_ be joined, treat them as joined** — so we make them genuinely unjoinable.

The `anal_`/`diag_` prefixes are intentional and make the ids self-identifying in payloads, PostHog, and D1. (Yes,
`anal_` is deliberate and David finds it funny. It will be visible in the PostHog `distinct_id` column and the D1
tables.)

### Email is decoupled, contact-only

The optional email lives **client-side** in `settings.json`, shown in Settings with a prominent "never sent with your
anonymous data" note. On entry it is sent to a **separate** `/beta-signup` endpoint that carries **no install id** and
subscribes the address to a dedicated Listmonk list. The analytics pipeline never sees the email. The email's only jobs
are: (a) a broadcast channel to testers (announcements, surveys, interview invites) via Listmonk, and (b) being
_voluntarily_ attachable to a specific report so we can reply about that bug. Targeted outreach based on observed usage
is intentionally **not** possible; that is the price of the unjoinability, and it is the right price.

### Consent model: opt-out for the beta

Anonymous analytics (heartbeat + feature events) is **on by default** during the open beta, disclosed plainly on a new
onboarding page and toggleable in Settings. Opt-out is **fully silent**: an opted-out install sends _nothing_, not even
a "I opted out" bit (that would be collecting from someone who declined). We therefore cannot measure the opt-out rate
from the analytics channel; estimate it indirectly from the update-check denominator (update checks are sent by everyone
regardless of the analytics toggle).

The "during the open beta" framing matters: it scopes the opt-out default and lets the privacy-policy copy reassure
users. Post-beta, the plan is to revisit the default (likely flip to opt-in or trim events); the plumbing stays.

### Smart backend, thin frontend; one event path

All analytics network calls happen in **Rust**. Frontend-originated feature signals (a search opened, a pane navigated)
go through a single thin `track_event` IPC command into the Rust analytics module, which attaches the `anal_` id and the
person properties and posts to PostHog. This keeps the PostHog key and the analytics id fully backend, gives a single
consent gate, and matches the "smart backend, thin frontend" and "thin IPC" principles. Backend-originated events call
the same Rust function directly.

### PII-free by allowlist, never by redaction

The heartbeat's config snapshot and every PostHog event carry only an **explicit allowlist** of categorical fields
(enums, counts, booleans). We never try to redact PII out of a free-form blob: settings hold SMB hostnames, paths, AI
keys, and the email, so a denylist would eventually leak one. The allowlist is PII-free by construction. Hard nevers:
file names, contents, paths, search queries, AI prompts, keystrokes, screenshots.

## Architecture summary

```
Desktop (Rust)                       api-server (CF Worker + D1)         External
──────────────                       ───────────────────────────         ────────
install_id.rs                        POST /heartbeat ─────► TELEMETRY_DB.heartbeat (raw, forever)
  anal_<uuid>, diag_<uuid>           GET  /admin/heartbeat-dau (bearer)
  (data-dir json, Rust-owned)        POST /beta-signup ───────────────────► Listmonk "Cmdr beta testers"
                                                                              (double opt-in, NO install id)
analytics/ (new module)
  heartbeat loop (launch + hourly) ──► POST /heartbeat        {anal_id, version, os, arch, build_mode, config-shape}
  track_event() ────────────────────► PostHog /capture/      {distinct_id: anal_id, source: desktop, $set: config-shape}
  reads analytics.enabled (consent), dev/CI suppressed

crash_reporter / error_reporter ──► diag_<uuid> + optional email (NEVER anal_id)
  crash → POST /crash-report → TELEMETRY_DB.crash_reports (+ diag_id, email columns)
  error → R2 bundle manifest (+ diagId, email) + Discord embed

analytics-dashboard
  per-day DAU chart  ◄── GET /admin/heartbeat-dau   (after M2/M8; interim uses daily_active_users in M1)
```

## Settings keys (new)

- `analytics.enabled` — boolean, **default `true`**. The opt-out. Section: **Updates & privacy**.
- `analytics.email` — string, default `""`. The beta contact email. Section: **Updates & privacy**, with the never-sent
  note.
- `updates.attachEmailToReports` — boolean, **default `false`**. Sticky default for the report email checkbox. Section:
  **Advanced**. (Also written by the report dialogs when the user toggles the checkbox, so it remembers the last
  choice.) Defined in **M7** (its only consumer), not earlier, to avoid a registry entry with no consumer. Uses the
  existing `updates.*` namespace (matching `updates.crashReports` / `updates.errorReports`), **not** a new
  `errorReports.*` namespace.

Per `name-internals-after-the-UI`: the section UI label is "Updates & privacy"; settings ids stay in the existing
`analytics.*` / `updates.*` namespaces. `analytics.enabled` + `analytics.email` are frontend-owned in `settings.json`;
the `anal_`/`diag_` ids are Rust-owned files (not settings) to avoid a first-launch write-ownership race (Rust only
reads settings).

## Milestones

Sequential execution is fine and expected. M1 is folded into M8 (single dashboard pass, heartbeat-sourced); the rest run
in order, with M8 (dashboard) naturally last since it depends on M2's heartbeat data. Each milestone lists its docs
updates, its tests (and which are written test-first), and the checks to run.

---

### M1 — (folded into M8)

**Decided (David): fix the dashboard once, not twice.** M1-M8 ship together this morning, so there's no value in an
interim update-check-based dashboard fix. All dashboard work (remove the inflated "217" display, add the per-day DAU
chart sourced from the heartbeat) lives in **M8**. The inflated number simply stops being displayed there; the new chart
sources from the heartbeat. (Note: the heartbeat table is empty at release and fills as testers update and run the new
build, so the chart starts empty and grows. That's honest and fine, better than a wrong number.)

---

### M2 — api-server: heartbeat ingestion + admin DAU endpoint

**Intent:** the backend foundation for true DAU. Mirrors the existing update-check telemetry patterns exactly.

**Changes (`apps/api-server/`):**

- **Migration** `migrations/0005_heartbeat.sql` (latest is `0004_crash_short_id.sql`). New table, raw rows, kept forever
  (no prune for now):
  ```sql
  CREATE TABLE heartbeat (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      anal_id TEXT NOT NULL,
      created_at TEXT NOT NULL DEFAULT (datetime('now')),
      app_version TEXT NOT NULL,
      os_version TEXT NOT NULL,
      arch TEXT NOT NULL,
      build_mode TEXT,                 -- 'release' | 'debug', nullable
      config_json TEXT                 -- the allowlisted config-shape snapshot (whole config minus string fields)
  );
  CREATE INDEX idx_heartbeat_created ON heartbeat(created_at);
  CREATE INDEX idx_heartbeat_anal ON heartbeat(anal_id);
  ```
  **`config_json` is a single JSON blob, not per-field columns** (David: "include the whole config except string
  fields"). Per-column would force a migration per new setting; a blob auto-absorbs new fields. DAU/engagement queries
  don't touch it; config-shape breakdowns come from PostHog person properties (richer at filtering). No UNIQUE/dedup
  constraint: we keep every beat (engagement = beats/day); DAU aggregation happens at query time.
- **`POST /heartbeat`** in `src/telemetry.ts`, mirroring `/crash-report`: size guard, JSON parse, shape validation
  (required `analId` matching `^anal_[0-9a-f-]{36}$`, `appVersion` semver, `osVersion`, `arch`; optional `buildMode`;
  `config` is an arbitrary object stored verbatim as `config_json`, capped in size), fire-and-forget D1 insert via
  `waitUntil`, return 204. **No IP hashing / no IP stored** (the `anal_` id is the dedup key now; storing a hashed IP
  alongside a stable id adds nothing and is more data) — but see rate limiting below, which needs the IP transiently.
- **Rate limiting (David: add now).** Gate `POST /heartbeat` with Cloudflare's Workers rate-limiting binding
  (`[[ratelimit]]` in `wrangler.toml`, `env.<LIMITER>.limit({ key })`), keyed by `cf-connecting-ip` (the IP is used only
  for the limiter's sliding window, never stored). Legit traffic is ~1/hour/install, so a generous cap (for example 12
  requests/minute/IP) stops a bloat-spam loop without touching real users. On limit, return 429. Verify the binding's
  current API at build time (don't trust stale docs). Apply the same limiter to `/beta-signup` (M5).
- **`GET /admin/heartbeat-dau`** in `src/admin.ts`, bearer-auth via `verifyAdminAuth`, `range` in `{7d,30d,90d,all}`
  (reuse `rangeToSqliteInterval`). Returns per-day `{ date, dau, beats }` where `dau = COUNT(DISTINCT anal_id)` and
  `beats = COUNT(*)` grouped by `date(created_at)`. This endpoint feeds the M8 dashboard chart.
- **Bindings/types:** the rate-limit binding in `wrangler.toml` + `Bindings`; otherwise reuses `TELEMETRY_DB`.

**Docs:** `apps/api-server/CLAUDE.md` (Routes table, Data flow, a "Heartbeat tracking" key-pattern entry, migration
note).

**Tests (test-first for validation — bug-prone boundary):**

- `src/heartbeat.test.ts` (new), mirroring `download-and-update-check.test.ts`: valid beat → 204 + row written
  (`config_json` round-trips); missing `analId` → 400; malformed `analId` → 400; oversized body → 400; optional fields
  omitted → 204; over the rate limit → 429.
- `src/admin-endpoints.test.ts`: add `/admin/heartbeat-dau` cases (auth required; per-day distinct count; range filter).

**Checks:** `pnpm check --check api-server` equivalents (`pnpm --filter @cmdr/api-server test`, `typecheck`), then
`--check oxfmt`.

---

### M3 — Desktop: install ids + heartbeat sender + consent + settings

**Intent:** real DAU data starts flowing; the consent + settings surface lands. No PostHog events yet (M4).

**Changes (`apps/desktop/src-tauri/`):**

- **`src/install_id.rs`** (new, neutral module so crash/error reporters needn't depend on `analytics`): owns both ids.
  `analytics_id()` and `diagnostics_id()` lazily read-or-generate, persisting to a single Rust-owned JSON file (for
  example `install-ids.json` with keys `analId`/`diagId`), written via `config::durable_write_json`. Format
  `anal_`/`diag_` + `Uuid::new_v4()`. Cached in a `OnceLock`/`Mutex`.
  - **Resolve the data dir WITHOUT an AppHandle** so the accessors can stay no-arg and be callable from the panic hook,
    the next-launch crash assembly, and the analytics loop alike. `config::resolved_app_data_dir` needs an `&AppHandle`,
    which isn't always available at those call sites. Use the same approach as `settings/loader.rs`'s `early_load_*`
    helpers: `CMDR_DATA_DIR` env if set, else `dirs::data_dir().join("com.veszelovszki.cmdr")`. (If you instead thread
    an `AppHandle` and resolve once at `init`, then the no-arg `analytics_id()`/`diagnostics_id()` signatures referenced
    in M3/M4/M7 must change to read from the `init`-populated statics.) **`uuid` is already a dependency**
    (`uuid = "1.x"`, `v4` feature, in `src-tauri/Cargo.toml`): just `use uuid::Uuid`. No `cargo add`, no version check,
    no `cargo deny` step.
  - **Signal-safety (load-bearing):** the crash signal handler is async-signal-safe (no alloc, no locks, only writes raw
    bytes to a pre-opened fd). It must NOT call `diagnostics_id()` (which allocates + locks). Resolve `diagnostics_id()`
    once at `init()` time into a static (`OnceLock<String>`), so the panic-hook path can read it cheaply, and the signal
    path attaches the diag id at **next-launch report assembly** (in the crash reporter's symbolicate/assembly step,
    full stdlib), not inside the handler. See M7.
- **`src/analytics/` (new module):**
  - `mod.rs`: `init(app)` + `start()` (mirror `space_poller`'s `tauri::async_runtime::spawn` loop). Loop: send a beat on
    launch, then every hour. Each tick: bail if `cfg!(debug_assertions)` or `CI` env set; read `analytics.enabled` via
    `crate::settings::load_settings` and bail if false; build the payload; `reqwest` fire-and-forget POST to
    `HEARTBEAT_URL` (`http://localhost:8787/heartbeat` debug / `https://api.getcmdr.com/heartbeat` release), mirroring
    `commands/crash_reporter.rs` client + timeouts + error logging.
  - `config_shape.rs`: pure function building the config-shape JSON from the **raw `settings.json`** + FDA state
    (`crate::fda_gate`). **Pure and unit-tested.** This is the only place the rule is defined.
    - **Rule (David: "whole config except string fields"):** read the raw settings JSON and include **every key whose
      value is a JSON boolean or number** (auto-extends as David adds bool/number settings, zero maintenance). Plus an
      explicit small `CATEGORICAL_STRING_KEYS` allowlist for the categorical enum-strings worth keeping (theme, view
      mode, AI mode/provider, sort mode, select-window mode, etc.) — these are stored as strings in JSON but are non-PII
      enums. **Exclude every other string, and all objects/arrays.** Why not "include all strings": free-text and
      identifier strings (SMB hostnames, paths, recent-paths lists, the email, AI key refs, custom names) are strings
      too, and blindly shipping them would leak PII. Bools/numbers are PII-free by nature, so they're safe to
      auto-include; only the categorical _string_ enums need the one-line allowlist add. Add FDA-granted (runtime, not a
      setting) explicitly.
    - **Guard test (privacy invariant):** given a settings JSON seeded with known PII-shaped string keys (email, an SMB
      host, a recents list), the output contains none of them, and contains no key whose value is a string unless it's
      in `CATEGORICAL_STRING_KEYS`.
  - `posthog.rs`: added in M4 (stub/empty in M3 or omit).
- **Heartbeat payload** built from `env!("CARGO_PKG_VERSION")`, `std::env::consts::ARCH`, build mode, `analytics_id()`,
  and the config-shape. For OS version, `crash_reporter::get_os_version()` is **private**; `error_reporter`'s copy is
  `pub(crate)`. The clean move (it's already duplicated across the two reporters) is to promote one `get_os_version()`
  to a shared spot and call it from all three; failing that, call the `error_reporter` `pub(crate)` one.
- **Settings loader** (`src/settings/loader.rs`): add `analytics_enabled: Option<bool>` and
  `analytics_email: Option<String>`. ⚠️ `parse_settings()` does **manual** `json.get("dot.key")` extraction (a
  deliberate decision, see `settings/CLAUDE.md`, because tauri-plugin-store writes dot-notation string keys); adding
  `#[serde(alias=...)]` alone does NOT read the value. Add the explicit lines:
  `json.get("analytics.enabled").and_then(|v| v.as_bool())` and
  `json.get("analytics.email").and_then(|v| v.as_str()).map(String::from)`, and wire both into the `Ok(Settings {...})`
  literal + the `Default` impl.
- **Consent is tri-state, default-on (the single most privacy-load-bearing line):** the frontend store persists only
  non-default values, so an opted-in install has **no** `analytics.enabled` key → `analytics_enabled: None`. The gate
  must treat `None` as **enabled** and only `Some(false)` as opted-out. `Some(true) → send`, `None → send`,
  `Some(false) → silent`. Both the heartbeat loop and `track_event` (M4) read through one shared
  `analytics_consent_granted()` helper.
- **Wire** `analytics::init(app.handle())` + `analytics::start()` from `src/lib.rs` setup, alongside `space_poller`.

**Changes (`apps/desktop/src/`):**

- **Settings registry** (`lib/settings/settings-registry.ts` + `lib/settings/types.ts` `SettingsValues`): add
  `analytics.enabled` (boolean, default true, switch), `analytics.email` (string, default "", text-input).
  (`updates.attachEmailToReports` is defined in M7, its only consumer.)
- **Section rename:** the existing **Updates** settings section becomes **"Updates & privacy"** (find the section label
  source — `SettingsSidebar.svelte` and the section component, for example `UpdatesSection.svelte`). Add the analytics
  opt-out switch and the email field (with the never-sent helper note) to it.
- The email field: on change, persist to settings (M3) and trigger the beta-signup call (M5 wires the call; in M3 just
  the field + persistence).

**Docs:** new `apps/desktop/src-tauri/src/analytics/CLAUDE.md` (the two-id model, consent gating, dev/CI suppression,
the allowlist-not-redaction rule, why ids are Rust-owned files). Update `lib/settings/CLAUDE.md` / `sections/CLAUDE.md`
for the renamed section. Note the new module in `docs/architecture.md` backend list.

**Tests (test-first for pure logic):**

- Rust: `install_id` generation/persistence (correct prefix, stable across calls, survives a reload from disk, regen if
  file missing) — **test-first**. `config_shape` builder maps settings + FDA state to the exact allowlist and emits no
  unexpected keys — **test-first**. The **consent tri-state** (`analytics_consent_granted()`: `None`/`Some(true)` →
  true, `Some(false)` → false) — **test-first** (this is the privacy invariant). Heartbeat payload serialization shape
  (no network) — written alongside.
- Frontend: a vitest that the registry exposes the new settings with correct defaults/types.

**Checks:** `pnpm check --rust` and `--svelte`, then `--check oxfmt`. (New filesystem-touching command? The heartbeat
runs in a background task, not a Tauri command, so no `blocking_with_timeout` needed.)

---

### M4 — Desktop: PostHog feature events

**Intent:** curated, PII-free product events for the consented-by-default beta, through one backend path.

**Changes (`apps/desktop/src-tauri/`):**

- **`src/analytics/posthog.rs`:** `capture(event: &str, props)` — builds
  `{ api_key: <phc_>, event, distinct_id: analytics_id(), properties: { source: "desktop", ...props }, $set: <config-shape> }`,
  fire-and-forget POST to `https://eu.i.posthog.com/capture/`. Key via `option_env!("CMDR_POSTHOG_KEY")` (the public
  `phc_` value for project `136072`, same as the website's `PUBLIC_POSTHOG_KEY`); if `None` (local dev), no-op. Same
  dev/CI + consent gating as the heartbeat.
- **`track_event` Tauri command** (thin pass-through to `analytics::capture`, async, consent-gated, fire-and-forget) so
  frontend-originated events use one backend path. Add a typed wrapper in `lib/tauri-commands/`, regen bindings
  (`pnpm bindings:regen`). **No capability entry needed:** custom app commands aren't ACL-gated (only core/plugin APIs
  go in `capabilities/*.json`); `default.json` lists zero custom commands. Still `await` it in try/catch. This is
  **not** a palette command, so no `COMMAND_IDS`/registry/handler needed (those are for user-facing actions); it's an
  internal IPC, so the `no-raw-tauri-invoke` rule is satisfied by the typed wrapper.
- **Build env:** add `CMDR_POSTHOG_KEY: ${{ secrets.CMDR_POSTHOG_KEY }}` to the `env:` block of the
  `tauri-apps/tauri-action` step in `.github/workflows/release.yml` (~line 105). Create the GitHub secret with the
  `phc_` value. Document in the api-server/website tooling docs.

**Open, extensible event API (David: "keep it open, I'll gradually add ~20 event types and per-event fields").** The
core is a generic `capture(event: &str, props: serde_json::Value)` (Rust) and a `track_event(name, props)` IPC for
frontend-originated events. Event names and props are **not** a fixed enum: adding an event later is a one-line call
with whatever PII-free props that event needs (for example `search_used` with a future `search_type` prop). Document the
"how to add an event" recipe + the naming convention (`name_internals_after_the_UI`) in `analytics/CLAUDE.md`.

**Starter event set (PII-free: enums, counts, bools only; never paths, names, queries, prompts):**

- `app_launched` (backend, at startup) — props: cold/warm if cheaply known.
- `pane_navigated` (frontend) — props: `volume_kind` enum (local/smb/mtp/git/search), never the path.
- `search_used` (frontend) — props: `mode` enum (extend with `search_type` etc. later); never the query.
- `select_files_used` (frontend, the "Select files…"/"Deselect files…" dialog) — props: `mode` enum; never the pattern.
- `file_transfer_completed` (backend, write_operations) — props: `op` (copy/move), `item_count` bucket, `had_conflicts`
  bool; never names/paths.
- `delete_used` (backend) — props: `trashed` bool, `item_count` bucket.
- `smb_connected` / `mtp_connected` (backend) — no host/device identifiers.
- `settings_opened` (frontend).
- `error_encountered` (backend, FriendlyError surface) — props: `category` enum only.

(Dropped per David: any AI folder-name-suggestion event.) This is a starting set, expected to grow.

**PII guard for open props (since props aren't a closed enum):** a single `sanitize_props` chokepoint in
`analytics::capture` that, in debug builds, asserts/logs if any string prop value looks PII-shaped (contains `/`, `\`,
`@`, or a `~/` home prefix), so an accidental path/email/query is caught during dev before it ever ships. It's a
heuristic safety net, not a substitute for the convention; pair it with the docs rule and a per-event review when adding
events. (Numbers/bools/short enums pass freely.)

**Docs:** `analytics/CLAUDE.md` (the open event API + how to add one, the PII-free convention + the `sanitize_props`
net, the single `track_event` path, the `option_env!` key mechanism, config-shape mirrored as person properties).
`docs/tooling/posthog.md` (desktop now captures; project, host, key mechanism).

**Tests:** Rust unit test for the capture payload builder (event name, `distinct_id` is the `anal_` id,
`source: desktop`, person-props `$set` = config-shape) with the HTTP boundary stubbed. A `sanitize_props` test: a
path/email/`~` string prop trips the guard, plain enums/numbers don't.

**Checks:** `--rust`, `--svelte` (bindings + wrapper), `--check oxfmt`. Run `pnpm bindings:regen` and verify
`bindings-fresh`.

---

### M5 — Email: beta-signup endpoint + Listmonk

**Intent:** the decoupled contact channel. The signup request carries no install id; double opt-in prevents prank
signups.

**Changes (`apps/api-server/`):**

- **`POST /beta-signup`** (new route module or in `licensing`/`telemetry`-adjacent file): accepts `{ email }` only,
  validates email shape, calls Listmonk `POST /api/subscribers` with the new list id, `status: "unconfirmed"`, **no**
  `preconfirm_subscriptions` (Listmonk then sends its double opt-in confirmation). Auth header
  `Authorization: token <user>:<token>` from secrets. Returns 204 on success, never reveals whether the address already
  existed (avoid enumeration). Fire-and-forget is **not** appropriate here (we want to surface failure to the client as
  a soft toast), but keep it resilient: on Listmonk error return a 502-style soft failure the app can show gently. Apply
  the M2 rate-limiter here too (keyed by IP), so the endpoint can't be used to spam signups/confirmation emails.
- **Bindings/types** (`src/types.ts`): `LISTMONK_API_URL?`, `LISTMONK_API_USER?`, `LISTMONK_API_TOKEN?`,
  `LISTMONK_BETA_LIST_ID?` (number). Set as wrangler secrets.
- **Listmonk infra (implementation-time, authorized by David):** create a new **double-opt-in** list "Cmdr beta testers"
  (NOT newsletter list `3`), and a new dedicated API user/token for the Worker (least privilege). Store the token + list
  id as wrangler secrets. Document the list id in `docs/tooling/` (mirror the obsidian listmonk doc note). Verify the CF
  Worker can reach `https://mail.getcmdr.com` over HTTPS.

**Changes (`apps/desktop/`):**

- On `analytics.email` change (Settings field from M3, and the onboarding field from M6): call a typed wrapper →
  `beta_signup` Tauri command → `POST /beta-signup` (Rust does the network, consistent with the backend-does-network
  rule; the email never travels with any analytics call). Surface success/failure as a gentle inline message ("Check
  your inbox to confirm" on success). Deleting/clearing the email locally only clears the local copy + stops
  report-attach; unsubscribe is via Listmonk's own link (state this in the Settings note).

**Docs:** `apps/api-server/CLAUDE.md` (route + Listmonk integration + secrets). A `docs/tooling/listmonk.md` note (Cmdr
beta list id, Worker token).

**Tests:** `src/beta-signup.test.ts` — valid email → Listmonk called with correct list + unconfirmed status, 204;
invalid email → 400; Listmonk failure → soft 502; **no install id present in the outbound Listmonk call** (assert the
request body). Mock the Listmonk `fetch`.

**Checks:** api-server tests + typecheck, desktop `--rust`/`--svelte`, `--check oxfmt`, `bindings-fresh`.

---

### M6 — Onboarding: "Open beta" page

**Intent:** disclose the opt-out analytics and offer the optional email, between AI and Optional.

**Changes (`apps/desktop/src/lib/onboarding/`):**

- `onboarding-state.svelte.ts`: extend `OnboardingStep` (`… | 4`) and bump `ONBOARDING_STEP_COUNT` to 4. The new "Open
  beta" page is **step 3**, pushing "Optional" to step 4. Verify `nextStep`/`isAtLastStep`/`resumeStepFor` still hold
  (they key off the count, not hardcoded numbers — confirm).
- `StepBeta.svelte` (new), built from `OnboardingStepShell`: the analytics disclosure + an opt-out control bound to
  `analytics.enabled`, and an optional email field (same field as Settings) that triggers beta-signup on submit. Model
  the input on the existing onboarding text-input pattern.
- `OnboardingWizard.svelte`: add the `{:else if currentStep === 3}` render branch (and shift the Optional branch to 4).
  The optional-dot logic keys off `ONBOARDING_STEP_COUNT - 1`; confirm the last (Optional) dot stays the optional one
  and decide whether the beta page is mandatory-advance or skippable (recommend: a normal Next, content makes clear it's
  informational + optional email).
- **⚠️ The AI step (step 2) currently has a dual-button footer where "Start using Cmdr!" fires `onComplete()` and skips
  straight past the Optional step (`onboarding/CLAUDE.md` § dual-button footer).** With Beta inserted at step 3, that
  shortcut would bypass the analytics disclosure. **Decided (David): the Beta page is non-skippable.** Rework the AI
  step's footer so its forward button advances to the Beta page (step 3) instead of completing onboarding. Replace the
  "Start using Cmdr!" skip with a single forward button (draft label "Go to open beta", see Appendix A) so the user
  always lands on Beta. Only the final _Optional_ step keeps a skip-to-finish. Net flow: FDA → AI → Beta (always seen) →
  Optional (skippable). The Beta page itself advances with a normal Next to Optional.

**Docs:** `apps/desktop/src/lib/onboarding/CLAUDE.md` (the new step, its purpose, the step-count bump).

**Tests:** vitest for `onboarding-state` (step count = 4, navigation 1→4, resume logic unaffected). A light component
test that the opt-out toggle reflects/writes `analytics.enabled` and the email field triggers signup.

**Checks:** `--svelte`, `--check oxfmt`.

---

### M7 — Crash/error reports: diag id + optional email attach

**Intent:** group reports per install; let testers voluntarily attach their email so we can reply. Preserve
unjoinability (never the `anal_` id; default-unchecked, sticky).

**Changes (`apps/desktop/src-tauri/`):**

- **Crash** (`src/crash_reporter/mod.rs`): add `diag_id: String` and `email: Option<String>` to `CrashReport`.
  - **`diag_id` is set at report time, not in the handler.** The panic-hook path may read the `OnceLock<String>` diag id
    (resolved at `init()`, see M3). The signal-handler path is async-signal-safe and must not allocate/lock, so it
    attaches the diag id at **next-launch assembly** (where the persisted crash file is read and symbolicated, full
    stdlib). Don't call `diagnostics_id()` inside the signal handler.
  - **`email` is a send-time field, not a crash-time one.** The crash is written to disk before any email is known; the
    user ticks the attach-email box at next launch in the dialog. So `email` is populated by the dialog and flows into
    `send_crash_report(report)` at send time. Do NOT try to read the email in the build/handler paths (no settings
    access in the signal context anyway).
- **Error** (`src/error_reporter/mod.rs`): add `diag_id` (from `install_id::diagnostics_id()`, safe here, full stdlib)
  - optional `email` to `BundleManifest` at assembly; include in the Discord embed. `commands/error_reporter.rs` threads
    the email from the dialog.
  * **⚠️ `BundleManifest` is shared by both error-report flows.** Flow A (user-initiated: dialog → preview → the
    attach-email checkbox) is the **only** path that may set `email`. Flow B (the opt-in auto-send `auto_dispatcher`,
    which fires on `log_error!` with no preview and no checkbox) **must always set `email: None`** — a user who enabled
    auto-send but never ticked attach-email for a specific report has not consented to shipping their address, and a
    leak there would break the decoupling promise. `diag_id` is fine on both flows (that's its purpose). Add a guard
    test: a `BundleKind::Auto` manifest never contains an email.
- **api-server** (`src/telemetry.ts`): extend `validateCrashReportShape` for optional `diagId` (regex
  `^diag_[0-9a-f-]{36}$`) and optional `email` (basic shape); write both to D1.
- **Migration** `migrations/0006_crash_diag_email.sql`:
  `ALTER TABLE crash_reports ADD COLUMN diag_id TEXT; ADD COLUMN email TEXT;` (both nullable). Surface the email in the
  crash-notification email (`scheduled.ts` `handleCrashNotifications` + `email.ts` row) so David sees who to reply to.

**Changes (`apps/desktop/src/`):**

- `lib/error-reporter/ErrorReportDialog.svelte` and `lib/crash-reporter/CrashReportDialog.svelte`: add an "Attach my
  email `<addr>` to this report" checkbox, **shown only when `analytics.email` is set**, initialized from
  `updates.attachEmailToReports` (default false), and on send/toggle it writes that bool back (sticky). When checked,
  the email is included in the send payload.
- Define `updates.attachEmailToReports` (boolean, default false) in the registry here, and add its toggle to
  \*\*Settings
  > Advanced\*\* (the manual control without opening a report).

**Docs:** `crash_reporter/CLAUDE.md` + `error_reporter/CLAUDE.md` (the `diag_` id, the optional email, why never the
`anal_` id). `apps/api-server/CLAUDE.md` (new crash columns).

**Tests (test-first for the validation + the unjoinability invariant):**

- api-server: crash-report accepts/round-trips optional `diagId`/`email`; rejects a malformed `diagId`; a test asserting
  a crash payload **never** contains an `anal_`-prefixed id (guard the invariant).
- Rust: `BundleManifest` includes `diag_id` and omits/includes email per the flag; assert no `anal_` id anywhere in the
  manifest.
- Frontend: checkbox hidden when no email on file; sticky default reflects `errorReports.attachEmail`.

**Checks:** `--rust`, `--svelte`, api-server tests, `--check oxfmt`.

---

### M8 — Dashboard heartbeat source + privacy policy + docs sweep

**Intent:** point the DAU chart at the true heartbeat data; retire the inflated metric from display (keep collecting
it); land the honest privacy-policy edits and the "no telemetry" doc corrections.

**Changes (`apps/analytics-dashboard/`):**

- New source fetch in `src/lib/server/sources/cloudflare.ts` calling `GET /admin/heartbeat-dau`; swap the M1 chart's
  data source from `daily_active_users` (update-check derived) to the heartbeat DAU series. Add an engagement view
  (beats/day or active installs) if cheap. Keep the update-check collection running (endpoint + cron untouched); just
  don't display it.
- `src/routes/api/report/+server.ts`: report the heartbeat DAU series.

**Changes (`apps/website/`):**

- `src/pages/privacy-policy.astro` — **drafted in [Appendix A](#appendix-a-draft-copy-david-reviews); David reviews
  before merge.** Punch-list: short-version box; "In the desktop app" list (anonymous-analytics bullet + optional-email
  bullet emphasizing unlinkability); rewrite the absolute "no usage telemetry" paragraph (~lines 86-90) with "during the
  open beta" framing, keeping the hard nevers (file names/contents/paths/queries/prompts/keystrokes/ screenshots);
  soften "your clicks from the app" (~line 104) to "which features are used, never their content"; legal basis
  (analytics = legitimate interest with opt-out; email = consent); who-we-share-with (PostHog now also desktop product
  analytics, EU; Listmonk already covers the beta email); retention (desktop analytics kept; "during the beta");
  Settings path "General > Updates" → "Updates & privacy"; bump `lastUpdated`.

**Docs sweep (retire the "no telemetry" claims):**

- `apps/website/CLAUDE.md` Analytics section ("The desktop app has **no telemetry**") → describe the beta analytics.
- `docs/architecture.md` desktop notes / diagnostics + the new `analytics/` module entry.
- `docs/tooling/posthog.md` ("The desktop app has no analytics") → desktop now captures.
- `apps/desktop` architecture notes referencing "no usage telemetry".

**Tests:** dashboard vitest for the heartbeat-DAU parser/aggregation (distinct per day). Manual: confirm the chart
renders the per-day series.

**Checks:** `--svelte`, website build (`html-validate` self-skips without `dist/`), `--check oxfmt`. Before declaring
done: `pnpm check --include-slow`.

---

## Cross-cutting checks and rules

- Finish every milestone with `pnpm check` (full default suite) and always `--check oxfmt` (monorepo-wide).
- `bindings-fresh` after any Rust command surface change (`pnpm bindings:regen`).
- New Tauri commands: `await` in try/catch; filesystem-touching ones async + `blocking_with_timeout`. **No capability
  entries** for custom app commands (those are only for core/plugin APIs); `track_event`/`beta_signup` do network, not
  local fs, so keep them async + resilient but they need no `capabilities/*.json` change.
- No `eprintln!`/`println!`; scoped `log::*` with `target:`. No bare lock `.unwrap()`. No error-string-matching across
  IPC (use typed enums/flags).
- No new crates needed (`uuid` is already a dependency). If any unforeseen crate comes up, verify a ≥14-day-old version
  on crates.io before `cargo add` and run `cargo deny check`.
- Style: sentence case, no em-dashes, friendly active voice, gender-neutral; `formatNumber()` for user-facing counts.

## Open implementation-time tasks (need David or a live service)

**David authorized (2026-06-09): build with the current keys we already have and swap them at the end; create the
Listmonk list with the agent token; trigger the migrations; push.** So these are greenlit, not blocked:

1. **PostHog:** use the existing public `phc_` key for project `136072` (the website's `PUBLIC_POSTHOG_KEY`; retrieve
   the value from the PostHog project settings via the `phx_` personal key in Keychain, or the website's deployed env).
   Bake it via `CMDR_POSTHOG_KEY` (GitHub secret for the release build; for local builds it's `option_env!` → `None` →
   suppressed, which is fine). To be rotated to a build-specific key later if desired.
2. **Listmonk:** create the **double-opt-in** "Cmdr beta testers" list now, using the agent superadmin token
   (`LISTMONK_API_KEY` in Keychain, user `agent`; see the obsidian listmonk doc). Record the new list id. For the
   Worker, set the wrangler secrets to the **agent token for now** (temporary), to be swapped for a dedicated
   least-privilege token at the end. Verify Worker → `mail.getcmdr.com` reachability.
3. **api-server deploy:** apply migrations `0005`/`0006` (`wrangler d1 migrations apply cmdr-telemetry`) and
   `wrangler deploy`. **Authorized.**
4. **Push:** authorized for this work.
5. **All user-facing copy** in Appendix A is David's to review/modify before merge.

## Appendix A: draft copy (David reviews)

> All strings below are drafts for David to review and modify. Sentence case, no em-dashes, friendly active voice.

### Onboarding AI step (`StepAi.svelte`) footer

- Forward button (replaces the old "Start using Cmdr!" skip): `Go to open beta`

### Onboarding "Open beta" page (`StepBeta.svelte`)

- Title: `Help shape the open beta`
- Body:
  `You're one of the first to use Cmdr. To learn what's working and what isn't, Cmdr sends anonymous usage stats during the open beta: which features get used and how often, never anything from your files. It's on now, and you can turn it off anytime.`
- Opt-out control label: `Send anonymous usage stats` (on by default)
- Email section label: `Stay in touch (optional)`
- Email helper:
  `Drop your email and I'll reach out with the occasional question or update. It's stored only on your Mac and is never connected to your usage stats.`
- Email placeholder: `you@example.com`

### Settings > Updates & privacy

- Analytics toggle label: `Send anonymous usage stats`
- Analytics toggle description:
  `Helps us see which features matter during the open beta. Never includes file names, paths, search terms, or prompts. Turn it off and Cmdr stops sending anything.`
- Email field label: `Beta contact email`
- Email field note (prominent):
  `Stored only on your Mac. We never send it together with your anonymous usage data, so your stats can't be tied back to you. Used only to reach out and to optionally attach to a report you send.`
- On signup success toast: `Check your inbox to confirm your email. Thanks for helping out!`

### Settings > Advanced

- Attach-email toggle label: `Attach my email to reports by default`
- Description:
  `When you send a crash or error report, include your beta contact email so we can reply. You can change this per report.`

### Error/crash report dialogs

- Checkbox label (when email on file): `Attach my email (<addr>) so we can reply`

### Privacy policy edits

Drafted at execution time against the live file (`apps/website/src/pages/privacy-policy.astro`), following the M8
punch-list. Key new lines to land (David reviews):

- Short version: append
  `During the open beta, the desktop app also sends anonymous usage stats (which features you use), with an easy off switch. It never includes anything from your files.`
- Desktop list, analytics bullet:
  `Anonymous usage stats (open beta): which features you use and basic preferences (like light or dark mode), tied to a random id, never to you. No file names, contents, paths, search terms, or prompts. On by default during the beta, off anytime in Settings > Updates & privacy.`
- Desktop list, email bullet:
  `Beta contact email (optional): if you share it, it's stored on your Mac and sent only to our mailing list so we can reach out. We never send it with your usage stats, so the two can't be connected.`
- Replace the "no usage telemetry" paragraph with the open-beta framing above, keeping the hard nevers verbatim.
