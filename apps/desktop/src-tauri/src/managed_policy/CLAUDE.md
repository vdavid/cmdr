# Managed policy (MDM)

What an organization's configuration profile restricts: telemetry, updates, and AI. Read from the FORCED layer of the
`com.veszelovszki.cmdr` preferences domain; macOS only (other platforms read "no restriction").

## Module map

- `mod.rs`: `ManagedPolicy` and its typed answers (`updates()`, `ai()`, `ai_destination()`, …). `cache.rs`: `current()`,
  `refresh()`, `for_egress()`, `init()`, and `apply_change`. `watch.rs`: activation and folder-watch triggers (macOS).
  `override_watch.rs`: the live re-read of the `CMDR_MANAGED_PREFS_FILE` plist (debug and E2E builds).
- `keys.rs`: key names and the one parse. `source.rs`: CFPreferences, the test-build plist file, the fake.
- `ceiling.rs`, `hosts.rs`: `MaxUpdateVersion` and `AllowedCloudAIHosts`. `egress.rs`: `allows(Egress)`. `refusal.rs`:
  `ManagedAiRefusal`. `locked.rs`: `locked_settings`, `refuses_write`, and `overlay`. `view.rs`: `ManagedPolicyView`,
  the command, the event. Frontend half: `apps/desktop/src/lib/managed-policy/CLAUDE.md`.

## Must-knows

- **The canonical key catalog is `DETAILS.md`.** `/trust` and the sample profile mirror it, and `public_docs_test.rs`
  fails until a new key reaches both. ❌ Spell a key name only in `keys.rs`.
- **Policy only restricts.** No key turns anything on; a key forced to its permissive value is "not managed".
- **Only forced values count.** `CfPrefsSource` asks `CFPreferencesAppValueIsForced` before `CopyAppValue`, which
  merges the user's own `defaults write` too. ❌ Never read the domain any other way.
- **A value we can't read restricts as much as that key can** (rule 4), with one `warn!` per change.
- **Never fail open.** `current()` reads synchronously on first use, so no caller sees an unloaded `Default` (= no
  restriction). ❌ Don't add a "loaded yet?" state.
- **Egress reads fresh.** A path about to send bytes calls `for_egress().await` (coalesced to one `cfprefsd` trip per
  second), never `current()`.
- **One decision per rule.** `allows(Egress)`, `ai_destination`, and `locked_settings` decide; callers ask, ❌ never
  re-derive a rule from the fields.
- **The overlay never writes.** `overlay()` changes an in-memory `settings.json` map; ❌ never persist its result.
- **`CMDR_MANAGED_PREFS_FILE` works only in debug, `playwright-e2e`, and test builds.** ❌ Never in a release build.

Key catalog, parse rules, precedence, refresh triggers, gate locations, and manual testing: `DETAILS.md`. Read it
before any non-trivial work here: editing, planning, reorganizing, or advising.
