# Onboarding details

Pull-tier docs for `apps/desktop/src/lib/onboarding/`: architecture, flows, and decision rationale. Must-know invariants
and gotchas live in [CLAUDE.md](CLAUDE.md).

Owns first-launch consent: Full Disk Access (macOS only), AI provider, the open-beta analytics disclosure, and a small
optional-settings step. Renders the `OnboardingWizard` (a soft-sheet that covers ~90% of the viewport over the running
app) as the single first-launch path.

Flow: FDA (1) → AI (2) → Open beta (3) → Optional (4). The Beta page is **non-skippable** (see § "Step 3 (Open beta)"
and the Decision below): the AI step's forward button always lands the user there, and only the final Optional step
finishes onboarding.

## Key files

- **`OnboardingWizard.svelte`**: Soft-sheet wizard shell: backdrop, step-dot indicator, Back button, primary footer
  button, Escape-swallow. Tab containment via the shared `use:trapFocus` (no `onEscape` — dismissal requires committing
  to a step).
- **`OnboardingStepShell.svelte`**: Per-step inner frame (padding, scroll container). Steps render their body inside.
- **`StepFda.svelte`**: Step 1 (macOS only): Full Disk Access. Three variants: first-ask, revoked, already-granted.
- **`StepAi.svelte`**: Step 2: AI provider picker. FDA-outcome banner (or none), comparison table (Without AI / With
  AI), three radio choices, single "Next" forward button.
- **`CloudProviderPicker.svelte`**: Step 2 left column: scrollable listbox of all 15 cloud providers. Single tab stop
  via `aria-activedescendant` (no roving focus); Arrow / Home / End / type-to-jump move the active option.
- **`CloudProviderSetup.svelte`**: Step 2 right column: per-provider numbered tutorial with API-key persist +
  auto-check + model combobox. Providers with editable OpenAI-compatible endpoints, including Custom, still require a
  stored API key before the endpoint check runs.
- **`StepBeta.svelte`**: Step 3 (Open beta, non-skippable): personal open-beta intro (feedback channels: in-app, GitHub,
  Discord, book-a-call) + anonymous-analytics disclosure + `analytics.enabled` opt-out switch + optional
  `analytics.email` contact field. Footer = "Start using Cmdr!" (finish here) + "One more optional setup step"
  (continue). Reuses the Settings `UpdatesSection` email/`betaSignup` wiring.
- **`StepOptional.svelte`**: Step 4 (optional): networking, indexing, updates, MTP toggles bound to existing registry
  settings.
- **`onboarding-state.svelte.ts`**: Wizard state machine: step cursor, step-1 variant, step-1 footer mode, step-2 banner
  mode, `openWizard()` / `resumeStepFor()` etc.

## Status

All four steps are real. The wizard is re-openable from the macOS app menu and the command palette (both platforms), and
the legacy `FullDiskAccessPrompt.svelte` modal is gone — the wizard is the single first-launch path on macOS. Existing
users on upgrade see a one-time `info` toast pointing at the menu item. `CMDR_FORCE_ONBOARDING=1` forces the wizard
regardless of persisted state for dev / E2E iteration.

## Re-entry points

Three surfaces open the wizard after first launch:

| Surface         | macOS                                             | Linux                          | Internal command id   |
| --------------- | ------------------------------------------------- | ------------------------------ | --------------------- |
| Menu item       | `Cmdr > Onboarding…` (under "Check for updates…") | (none; palette-only by design) | `cmdr.openOnboarding` |
| Command palette | "Onboarding…" (both platforms)                    | "Onboarding…"                  | `cmdr.openOnboarding` |
| MCP             | `dialog` tool with `type: "onboarding"`           | same                           | (none; direct event)  |

All three surfaces route through the same handler (`+page.svelte::openOnboardingFromMenuOrPalette`), which opens the
wizard at the first reachable step (step 1 on macOS, step 2 on Linux) regardless of `isOnboarded`. The plan's round-3 #1
codifies "menu re-entry always opens at step 1"; `openWizard()` enforces this by checking the `source` argument.

**Why no Linux menu entry**: the wizard's design language is macOS-centric (frosted backdrop matches macOS sheets,
"Restart Cmdr" copy assumes the Quit & Reopen flow, FDA-relevance). Adding a redundant menu entry next to the palette
command would clutter Linux's GTK menu bar for marginal benefit. Palette discovery is good enough; the upgrade-nudge
toast names it on first launch after upgrade.

### Upgrade nudge

Existing users (anyone with `isOnboarded === true` and `onboarding.upgradeNudgeShown === false`) see one `info` toast on
the first launch after they update past the wizard revamp:

- macOS: "We've added new onboarding options. Open Cmdr > Onboarding… to review them."
- Linux: "We've added new onboarding options. Open the command palette and run Onboarding… to review them."

The toast fires from `resolveOnboardingMount()`'s `showApp = true` branches (so it only runs when the wizard is NOT
mounting; no need for an extra `onboardingShowing` check). It writes `onboarding.upgradeNudgeShown = true` synchronously
after firing, so it never appears again on the same machine.

The toast is suppressed under `getAppMode() === 'e2e'` so it doesn't leak into Playwright's first-spec-of-the-run state
(each E2E shard gets its own fresh data dir, so the nudge would otherwise fire once per shard launch and trip the
fixture safety net). The firing logic itself stays unit-tested in Vitest; the E2E suppression is a target-mode gate, not
a behaviour change.

### MCP

The MCP `dialog` tool's open path accepts `type: "onboarding"`. It emits the standard `execute-command` Tauri event with
`commandId: "cmdr.openOnboarding"` (the same path the menu and palette use), and acks on
`SoftDialogAppeared("onboarding")` within the standard 1500 ms budget. The wizard calls
`notifyDialogOpened('onboarding')` on mount, so `SoftDialogTracker` reflects it.

No dedicated `open_onboarding` MCP command was needed: the existing generic `dialog` tool's open switch is hard-coded
per dialog type, but adding one case is cheaper than a new tool and keeps the agent API consistent with
`dialog open about` / `dialog open settings`. Close / focus actions aren't wired for `onboarding` (the wizard has no
rivals to focus above, and closing requires committing to a step per round-3 #9; the design forbids
dismiss-without-decision).

## Step 1 (Full Disk Access)

macOS only. Linux skips the step entirely (the resume rule lands Linux users on step 2).

The step has three opening copy variants, picked by `step1VariantFor()` in `onboarding-state.svelte.ts`:

- **first-ask** (`fullDiskAccessChoice === 'notAskedYet'`): welcome + pros/cons + how-to + Allow / Deny.
- **revoked** (`'allow' && !hasFda && isOnboarded`): "Cmdr previously had FDA but you revoked it…" framing.
- **already-granted** (`hasFda === true`, menu / palette re-entry): single line + a Next footer button.

The buttons inside the step body (`Open System Settings`, `Deny`) own the Allow / Deny flow; the wizard's footer primary
button is hidden in `decide` mode and reads `Restart Cmdr` in `restart` mode (set after Allow). The `already-granted`
variant has no in-body buttons; the wizard's footer renders a single `Next`.

### Live grant detection

While the Allow / Deny variants are open and FDA isn't granted yet, a 500 ms `$effect` poller in `StepFda.svelte`
watches the OS. The moment the user toggles Cmdr on in System Settings, the body switches to a success state ("You
granted full disk access!") and the footer flips to `Restart Cmdr`, so the screen feels connected to System Settings
instead of guessing. The grant is tracked via `onboardingState.step1Granted`, set by `setStep1Granted()` (which also
sets `step1FooterMode = 'restart'`).

The poller calls `checkFullDiskAccessQuiet`, NOT `checkFullDiskAccess`. The heavy command fires a multi-trigger
registration storm (`mmap` + `NSData` + `read_dir` of the parent) plus per-call logging on every denial, by design, to
get Cmdr into the FDA list. Polling that twice a second would spam syscalls and the log, so the quiet command is a
single side-effect-free `read()` per candidate file with no steady-state logging. Both share the same `CMDR_MOCK_FDA`
override and the same `fda_probe_files()` candidate list (factored into `probe_fda_quiet()` / `mock_fda_override()` in
`permissions.rs`). Keep `checkFullDiskAccess` for the one-shot registration moments (the re-probe before
`openPrivacySettings`, the step-2 banner probe).

The restart stays required even on live detection: the FDA gate is set once at boot, so the new permission only takes
effect on relaunch (same reason as the Allow path; see § "Allow path requires a restart"). Detection only swaps the copy
and button; it never clears the gate at runtime.

Lifecycle: the interval starts on mount (only on the Allow/Deny variants, only on macOS, only when not already granted)
and is cleared on unmount and on grant, so no interval leaks. The `already-granted` variant never polls (FDA is already
on), and on Linux the whole component renders `null`, so nothing polls there.

### Allow path requires a restart

Per the "FDA gate clear-on-Allow" decision (see also § "Key decisions" below): after the user clicks Allow, the wizard
does NOT advance to step 2 in-session. The footer's primary button flips to "Restart Cmdr" (calls `relaunch()` from
`@tauri-apps/plugin-process`). Reason: the FDA gate (`fda_gate::FDA_PENDING`) is set once at boot from
`(fda_choice, os_fda_granted)`; clearing it at runtime would race the TCC popups the gate was built to suppress (we hit
5–10 stacked popups once already). The user's choice persists, and the resume rule lands them on step 2 immediately
after relaunch.

The Allow / Deny buttons stay live in restart mode so the user can change their mind to Deny without restarting (Deny
advances normally).

### Deny path

`StepFda.svelte::handleDeny`:

1. `saveSettings({ fullDiskAccessChoice: 'deny' })`.
2. `startIndexingAfterFdaDecision()`: clears the runtime FDA gate, starts the MTP watcher, kicks off the indexer. The
   scan walks `~/Downloads`, `~/Documents`, `~/Desktop`, etc., firing one TCC popup per folder. Those are the per-folder
   prompts the user opted into by denying FDA. Folders the user denies stay unindexed.
3. `setStepTwoBanner('denied')` + advance to step 2.

## Step 2 (AI provider)

Three pieces stacked top to bottom:

1. **FDA-outcome banner**: on step-2 entry, `StepAi.svelte` fires a fresh `checkFullDiskAccess()` + reads
   `fullDiskAccessChoice` + `isOnboarded` and writes one of these modes via `setStepTwoBanner()`:
   - `granted` ("Thanks for granting full disk access!") only on a FRESH first-run grant (`hasFda && !isOnboarded`).
   - `none` (no banner) when FDA is on but the user already finished onboarding (menu / palette re-entry): FDA being on
     is the steady state, not news, so we don't re-celebrate it.
   - `denied` ("You chose not to enable full disk access.")
   - `stuck` ("Cmdr doesn't seem to have full disk access yet"; surfaces a deep link to System Settings) Linux
     short-circuits with `linux` (no banner; the step opens with the Welcome line instead).
2. **Comparison table**: "without AI vs with AI" for Search, Mass-rename, Select. The "With AI" column is the rightmost
   and carries the accent flair (tint + sparkle) to draw the eye.
3. **Three radio choices**: cloud / local / no AI. Pre-selected from the persisted `ai.provider` so a crash-then-resume
   user lands on their previous pick. Picking cloud reveals `CloudProviderPicker.svelte` (left) and
   `CloudProviderSetup.svelte` (right). Picking local kicks off `startAiDownload()` in the background; switching away
   cancels (HTTP-Range resume picks up on switch-back). Intel Macs see the local radio disabled with a tooltip ("Local
   LLM requires Apple Silicon. Cloud works on Intel.") driven by `getAiRuntimeStatus().localAiSupported`.

### Forward footer (single "Next" button)

Step 2 owns its own footer via `setFooterOverride([...])` with a single primary **Next** button: it persists the AI
choice + `pushConfigToBackend()`, then `nextStep()` to the Beta page (step 3). The AI step never completes onboarding,
because the Beta page is non-skippable (see the Decision below): every path through AI lands on Beta.

The button stays enabled regardless of API-key validity per the **no-key-blocks-advance** rule: the auto-check status in
the right column is feedback enough; forcing valid key entry as a precondition would fight users who want to grab the
key later. The user can re-enter via `Cmdr > Onboarding…` or fix it in Settings; first AI use surfaces the standard
`NotConfigured` error path.

### Connection-check pipeline

`CloudProviderSetup.svelte` mirrors `lib/settings/sections/AiCloudSection.svelte`'s pipeline rather than forking it: 300
ms debounce on API-key persist, 1 s debounce on `checkAiConnection(baseUrl, apiKey)`. On `connected`, the right column
reveals the model combobox populated from `/models` and the API-key step gets a green check. The wizard never disables
advance based on connection status; the auto-check is purely informational.

### `pushConfigToBackend()` belt-and-braces

The `settings-applier.ts` listener also calls `pushConfigToBackend()` on any `ai.provider` / `ai.cloudProvider` /
`ai.cloudProviderConfigs` change, so the wizard's explicit `await` is redundant in the steady state. The reason it's
there: the listener fires per-setting-change, so if the user flips three settings in one tick we get three async
invocations racing the wizard's `onComplete()`. The explicit `await pushConfigToBackend()` in `StepAi.persist()` orders
the backend reconfigure before the user lands in the app deterministically.

## Step 3 (Open beta)

`StepBeta.svelte`: David's personal open-beta intro, the analytics disclosure, and an optional contact channel. Three
blocks:

1. **Personal intro**: first-person welcome (solo dev, rough parts marked with an inline `StatusBadge status="alpha"`,
   feedback shapes the roadmap) plus the feedback channels as a numbered list: the `Help > Send feedback…` menu item
   (with the `app.commandPalette` `ShortcutChip`), GitHub issues, Discord, a book-a-call link, and a star/watch/fork CTA
   (helps Cmdr reach Homebrew's notability bar for a tap-free `brew install`). The URLs come from the shared
   `$lib/beta-links.ts` constants (also used by `AboutWindow.svelte`); the links render as `LinkButton`s routed through
   `openExternalUrl`.
2. **Anonymous-analytics opt-out**: the registry-backed `<SettingSwitch id="analytics.enabled">` (default on). Flipping
   it writes the setting immediately, exactly like the same switch in Settings.
3. **Optional contact email**: an email field bound to `analytics.email`. It persists locally on every keystroke and, on
   commit (blur / Enter) of a valid address, calls the typed `betaSignup` wrapper (which POSTs only the email, never an
   install id) and renders a gentle inline result.

The analytics and email blocks reuse `settings/sections/UpdatesSection.svelte`'s exact wiring (the same `betaSignup`
call, the same email-pattern + `lastSubmittedEmail` resend guard, the same success/failure copy), so the onboarding page
and Settings behave identically.

The footer has two buttons: a secondary **Start using Cmdr!** that finishes onboarding right here (skipping the optional
step, via `requestWizardComplete()`) and a primary **One more optional setup step** that `nextStep()`s to the Optional
step. There is no skip-to-finish that bypasses this page: every first-launch user sees the analytics disclosure once,
because the AI step always lands here and both buttons start from this page.

## Step 4 (optional setup)

Four toggle blocks, each bound to an existing registry setting via `<SettingSwitch>`. Defaults stay ON; the step is
about letting the user turn things OFF with full context, not about asking for opt-in.

| Toggle            | Setting ID                  | Live-apply wiring                                                             |
| ----------------- | --------------------------- | ----------------------------------------------------------------------------- |
| Networking        | `network.enabled`           | `passthroughBackendHandlers` → `setNetworkEnabled` (pre-existing)             |
| Drive indexing    | `indexing.enabled`          | `passthroughBackendHandlers` → `setIndexingEnabled` (pre-existing)            |
| Automatic updates | `updates.autoCheck`         | `passthroughBackendHandlers` → `applyAutoCheckEnabled` in `updater.svelte.ts` |
| MTP               | `fileOperations.mtpEnabled` | `passthroughBackendHandlers` → `setMtpEnabled` (pre-existing)                 |

Because `<SettingSwitch>` writes via `setSetting()` on every flip, the toggles take effect the moment the user clicks
them; the wizard doesn't need its own persist queue. The footer's single primary button (`Start using Cmdr`, registered
via `setFooterOverride()`) just bumps `finishRequestTick`; the wizard shell's `onComplete` callback then runs
`notifyOnboardingComplete()` (which flips `isOnboarded: true`) and closes the sheet.

`updates.autoCheck` live-apply runs through `applyAutoCheckEnabled(enabled)` in `updates/updater.svelte.ts`: the
poll-loop interval handle lives at module scope so the listener can start/stop it in place and fire one immediate
`checkForUpdates()` on re-enable so users don't wait the full cadence. The matching entry in `settings-applier.ts`'s
`passthroughBackendHandlers` table is what makes the toggle work from anywhere (wizard, Settings UI, MCP) without an app
restart.

## Resume rule

`onboarding-state.svelte.ts::resumeStepFor(ctx)` picks the right step from the persisted flags + an FDA probe:

| macOS state                              | Resume step | Step 1 variant | Step 2 banner |
| ---------------------------------------- | ----------- | -------------- | ------------- |
| `notAskedYet`                            | 1           | first-ask      | (stuck)       |
| `allow` && `!hasFda` && `isOnboarded`    | 1           | revoked        | (stuck)       |
| `allow` && `hasFda` (and `!isOnboarded`) | 2           | (n/a)          | granted       |
| `allow` && `!hasFda` && `!isOnboarded`   | 2           | (n/a)          | stuck         |
| `deny`                                   | 2           | (n/a)          | denied        |

Linux always resumes at step 2 (no FDA gate).

## FDA gate

Two things stay gated on the FDA decision at app launch:

1. **Drive indexer** (recursive scan from `/` would touch iCloud, Photos, ...).
2. **Path-based icon fetches** in `volumes::list_locations` (NSWorkspace.iconForFile on `/Applications`, `~/Desktop`,
   etc. cascades into adjacent TCC services).

Both gates use the same predicate via `crate::fda_gate::is_fda_pending(fda_choice, os_fda_granted)`.

After the user decides:

- **Deny**: `startIndexingAfterFdaDecision()` clears the runtime gate, starts the MTP hotplug watcher, and starts the
  indexer. As the scan walks protected paths, macOS fires one TCC popup per folder.
- **Allow**: the user grants FDA in System Settings, then clicks `Restart Cmdr`. On next launch the OS check returns
  true, the gate is open at boot, and both the indexer and icon fetches run normally with no popups.

The Tauri command is idempotent. See `src-tauri/src/fda_gate.rs`, `src-tauri/src/volumes/CLAUDE.md` § "FDA gate", and
`src-tauri/src/indexing/CLAUDE.md` § "Defer indexer auto-start".

## Mount + onboarding flag

`routes/(main)/+page.svelte` decides whether to mount the wizard:

- `CMDR_FORCE_ONBOARDING=1` → mount wizard.
- `hasFda && isOnboarded` → no wizard; mirror `fullDiskAccessChoice` to `'allow'` if needed.
- `hasFda && !isOnboarded` → no wizard; mirror setting + call `notifyOnboardingComplete()` (covers pre-wizard users who
  already granted FDA).
- `deny && isOnboarded` → no wizard (user denied and finished onboarding).
- Anything else → mount wizard.

The `isOnboarded` boolean lives in `$lib/settings-store.ts`. It flips to `true` on full wizard completion via
`notifyOnboardingComplete()` (from `$lib/updates/updater.svelte`), so the auto-update "restart to apply" toast doesn't
fire during first-launch onboarding.

While the wizard is up, `+page.svelte` also calls `setOnboardingShowing(true)` so the updater suppresses the deferred
toast; `handleWizardComplete` flips it back. See `$lib/updates/CLAUDE.md` § "Onboarding gating".

## Testing

Two env vars (mirror `CMDR_MOCK_LICENSE`):

- `CMDR_FORCE_ONBOARDING=1` (read by `is_force_onboarding()` Tauri command in the backend): opens the wizard regardless
  of persisted state. Useful for design iteration without touching settings.
- `CMDR_MOCK_FDA=granted|denied|notgranted` (read in `permissions.rs::check_full_disk_access`): overrides the TCC probe
  so all banner branches can be tested without ever opening real System Settings. `granted` → `true`; `denied` /
  `notgranted` → `false`. The wizard distinguishes them via the persisted setting + a fresh probe on step-2 entry.

Run with both: `CMDR_FORCE_ONBOARDING=1 CMDR_MOCK_FDA=notgranted pnpm dev`.

## i18n (message catalog)

All user-facing onboarding copy lives in `$lib/intl/messages/en/onboarding.json` (keys `onboarding.<step>.<leaf>`),
resolved through the `$lib/intl` runtime: `tString()` for static and `{var}`-interpolated strings, `<Trans>` for the
many inline-component sentences (David's warm beta copy is dense with `<strong>`/`<em>`/`<LinkButton>` runs). The
base-en output is byte-identical to the pre-migration copy (a behavior-preserving MOVE), pinned by
`onboarding-i18n-parity.test.ts`.

Conventions specific to this area:

- **Inline-component sentences use `<Trans key=… snippets={{…}} />`.** Each component renders a same-named local snippet
  (`{#snippet strong(children)}<strong>{@render children()}</strong>{/snippet}`, etc.). The link snippets close over the
  component's own click handlers (`openLink(url)`, `openPrivacySettings()`, the GitHub source link), so the catalog
  holds ONLY the wrapped text, never a URL.
- **Empty-tag markers** (`<chip></chip>` for a `ShortcutChip`, `<alpha></alpha>` for a `StatusBadge`) are snippets that
  render the component and then `{@render children()}` (the children are empty; the render is a no-op that keeps the arg
  used, since the lint has no `argsIgnorePattern`).
- **Shared strings stay with their owners.** `systemStrings.*` (the localized macOS pane names) is passed in as a
  `{systemSettings}` placeholder, not copied into the catalog. `analyticsDef.description` (Step 3) and the cloud
  provider `preset.name` / `preset.description` (CloudProviderSetup) render from the settings/AI registries and are NOT
  in `onboarding.json` — they migrate with those areas.
- **Banner titles / footer labels moved out of the JS objects** (`bannerTitleByMode`, the `setFooterOverride([{label}])`
  arrays) into `tString()` calls so they translate too.

## Key decisions

**Decision**: Three-state setting (`notAskedYet` / `allow` / `deny`) instead of a boolean. **Why**: The app needs to
distinguish "never asked" (show first-ask), "granted but later revoked" (show revoked copy), and "user explicitly
declined" (don't re-prompt once onboarded). A boolean would conflate "not asked" with "denied".

**Decision**: No Escape handler on the wizard. **Why**: The wizard owns first-launch consent; dismissing without
choosing leaves the app with no recorded preference. The user must commit to Allow / Deny / Next on each step.

**Decision**: Allow requires a restart before advancing past step 1. **Why**: The FDA gate is set once at boot; clearing
it at runtime races background threads that resolve icons / scan paths into the TCC popups the gate suppresses. We hit
5–10 stacked popups once already; the restart costs the user one click and keeps the gate's invariant intact. See plan §
"FDA gate clear-on-Allow".

**Decision**: The Open beta page (step 3) is non-skippable; the AI step has no skip-to-finish. **Why**: Every
first-launch user must see the anonymous-analytics disclosure once (the opt-out default only reads as fair consent if it
was actually shown). So the AI step's only forward button ("Next") always `nextStep()`s to Beta. The Beta page itself
offers "Start using Cmdr!" (finish) and "One more optional setup step" (continue), so both forward paths start from Beta
and the user can't reach the app without seeing it. Don't re-add a skip-to-finish button on the AI step (it would bypass
the disclosure). The user can still opt out and skip the email on the Beta page, they just can't skip seeing it.

**Decision**: Step 1 footer button hidden in `decide` mode (body owns Allow / Deny). **Why**: The Allow / Deny choice is
the meat of step 1; placing the buttons inside the body groups them with the explanatory copy they belong to. The
wizard's footer remains consistent for the other steps (Back + Next / Finish / Restart Cmdr).

## Key gotchas

- **Deep-link host changed in Ventura.** macOS 13+ uses `com.apple.settings.PrivacySecurity.extension`; older macOS uses
  `com.apple.preference.security`. `openPrivacySettings()` picks via `get_macos_major_version`. The same version informs
  the modal copy: macOS 12 and older append new FDA entries at the end of the list (instead of alphabetical).
- **Getting Cmdr into the FDA list is a separate concern from detecting FDA, and it's macOS-version-dependent.** The
  mechanism (detect via a file `read()`; register via a directory `open()` on macOS 13+, file reads on macOS 12) lives
  in one place: the module doc of `src-tauri/src/permissions.rs`. Don't restate it here. Onboarding-relevant parts only:
  registration rides the heavy `check_full_disk_access` (fired at boot, on step-1 mount, and right before opening System
  Settings), and the "+" button fallback (step 1's `step-tip`) stays as the backstop for the edge cases that still
  wouldn't list Cmdr (a machine where none of the probe dirs exist, or a future OS change). References:
  [Apple Developer Forums #809549](https://developer.apple.com/forums/thread/809549),
  [Apple Developer Forums #757768](https://developer.apple.com/forums/thread/757768).
- **The wizard renders the app behind it.** First launch lands on `~`, so what peeks through the backdrop is friendly.
  No "white screen until wizard done" code path.
- **Linux skips step 1.** `isAtFirstStep()` returns `true` on step 2 for Linux so the Back button disables there. Step 1
  returns `null` on Linux as a safety net.

## Dependencies

- `$lib/tauri-commands`: `checkFullDiskAccess`, `checkFullDiskAccessQuiet` (Step 1's 500 ms grant-detection poller),
  `getMacosMajorVersion`, `openPrivacySettings`, `startIndexingAfterFdaDecision`, `openExternalUrl`,
  `notifyDialogOpened`, `notifyDialogClosed`, `isForceOnboarding`, `betaSignup` (Step 3's email signup)
- `$lib/settings-store`: `saveSettings`, `loadSettings`
- `$lib/shortcuts/key-capture`: `isMacOS`
- `$lib/system-strings.svelte`: localized system pane names
- `$lib/ui`: `Button`, `LinkButton`
- `$lib/beta-links`: `GITHUB_REPO_URL`, `GITHUB_ISSUES_URL`, `BOOK_A_CALL_URL`, `ABOUT_DAVID_URL` (Step 3's feedback +
  star-CTA links; shared with `AboutWindow.svelte`)
- `@tauri-apps/plugin-process`: `relaunch` (Allow-path footer button)
