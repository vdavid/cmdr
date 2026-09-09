# Onboarding details

Pull-tier docs for `apps/desktop/src/lib/onboarding/`: architecture, flows, and decision rationale. Must-know invariants
and gotchas live in `CLAUDE.md`.

Owns first-launch consent: Full Disk Access (macOS only), AI provider, the open-beta analytics disclosure, terms
acceptance, and a small optional-settings step. Renders the `OnboardingWizard` (a soft-sheet that covers ~90% of the
viewport over the running app) as the single first-launch path.

Flow: FDA (1) → AI (2) → Open beta (3) → Optional (4). The Beta page is **non-skippable** (see § "Step 3 (Open beta)"
and the Decision below): the AI step's forward button always lands the user there, and only the final Optional step
finishes onboarding.

## Key files

- **`OnboardingWizard.svelte`**: Soft-sheet wizard shell: backdrop, step-dot indicator, Back button, primary footer
  button, Escape-swallow. Tab containment via the shared `use:trapFocus` (no `onEscape` — dismissal requires committing
  to a step).
- **`OnboardingStepShell.svelte`**: Per-step inner frame (padding, scroll container). Steps render their body inside.
- **`OnboardingLanguagePicker.svelte`**: The language escape hatch in the wizard header's right cell (a globe glyph plus
  `SettingSelect` on `appearance.language`). See "The language escape hatch" below. It overrides `SettingSelect`'s
  `min-width` to hug its trigger: that trigger is a borderless macOS pop-up button sized to its own text, so a fixed
  wrapper width would park the visible control short of the panel's right padding edge with dead space after it.
- **`StepFda.svelte`**: Step 1 (macOS only): Full Disk Access. Three variants: first-ask, revoked, already-granted.
- **`StepAi.svelte`**: Step 2: AI provider picker. FDA-outcome banner (or none), comparison table (Without AI / With
  AI), three radio choices, single "Next" forward button.
- **`CloudProviderPicker.svelte`**: Step 2 left column: scrollable listbox of all 15 cloud providers. Single tab stop
  via `aria-activedescendant` (no roving focus); Arrow / Home / End / type-to-jump move the active option.
- **`CloudProviderSetup.svelte`**: Step 2 right column: the provider header and status line around the shared
  `$lib/ai-provider-setup/ProviderSetupSteps`. Providers with editable OpenAI-compatible endpoints, including Custom,
  still require a stored API key before the endpoint check runs.
- **`StepBeta.svelte`**: Step 3 (Open beta, non-skippable): personal open-beta intro (feedback channels: in-app, GitHub,
  Discord, book-a-call) + usage-stats disclosure + `analytics.enabled` opt-out switch + the crash-report disclosure +
  optional `analytics.email` contact field + the required terms checkbox. Footer = "Start using Cmdr!" (finish here) +
  "One more optional setup step" (continue), both gated on the terms. Reuses the Settings `UpdatesSection`
  email/`betaSignup` wiring.
- **`StepOptional.svelte`**: Step 4 (optional): networking, indexing, updates, MTP toggles bound to existing registry
  settings.
- **`OnboardingToggleCard.svelte`**: the bordered card of one registry-backed `<SettingSwitch>` (title, description
  snippet, switch, caption) that `StepBeta`'s analytics opt-out and `StepOptional`'s four toggles render, so the two
  steps stay pixel-identical. The description snippet is styled by the parent's own `.toggle-desc` / `.toggle-list`. An
  optional `details` snippet + `detailsLabel` adds the info glyph beside the title (see § "The info glyph").
- **`onboarding-state.svelte.ts`**: Wizard state machine: step cursor, step-1 variant, step-1 footer mode, step-2 banner
  mode, `openWizard()` / `resumeStepFor()` etc.

## Status

All four steps are real. The wizard is re-openable from the macOS app menu and the command palette (both platforms), and
the legacy `FullDiskAccessPrompt.svelte` modal is gone — the wizard is the single first-launch path on macOS. Existing
users on upgrade see a one-time `info` toast pointing at the menu item. `CMDR_FORCE_ONBOARDING=1` forces the wizard
regardless of persisted state for dev / E2E iteration.

## Re-entry points

Four surfaces open the wizard after first launch:

| Surface         | macOS                                             | Linux                          | Internal command id   |
| --------------- | ------------------------------------------------- | ------------------------------ | --------------------- |
| Menu item       | `Cmdr > Onboarding…` (under "Check for updates…") | (none; palette-only by design) | `cmdr.openOnboarding` |
| Command palette | "Onboarding…" (both platforms)                    | "Onboarding…"                  | `cmdr.openOnboarding` |
| MCP             | `dialog` tool with `type: "onboarding"`           | same                           | (none; direct event)  |
| Search coverage | "Set up full disk access" in the coverage note    | (never offered)                | (host callback)       |

### The search route in

A search that walks folders the index doesn't cover can be REFUSED one, and a refusal is the one coverage gap with a way
out. So the coverage note offers it, and the offer lands here rather than in a second permission screen: step 1 is
already the page that explains Full Disk Access, opens System Settings, polls for a live grant, and handles the restart
(`SearchDialog.svelte` → `+page.svelte`'s `onGrantFullDiskAccess` → `openOnboardingFromMenuOrPalette(..., 'palette')`).

Three conditions gate the offer, and the search side owns all three
(`lib/search/coverage-note.ts::offersFullDiskAccess`): a folder was actually REFUSED (a NAS snapshot tree Cmdr declines
to read on purpose is a different typed list, and no permission opens one), this is macOS, and Cmdr doesn't already have
the permission — probed with `checkFullDiskAccessQuiet`, never the loud `checkFullDiskAccess`, since a search runs often
and the loud one fires a TCC-registration storm per denial. The dialog closes on the way in, because the wizard is the
app's modal and the user pressing this is heading for System Settings and a restart.

All three of the original surfaces route through the same handler
(`routes/(main)/startup-gates.ts::openOnboardingFromMenuOrPalette`), which opens the wizard at the first reachable step
(step 1 on macOS, step 2 on Linux) regardless of `isOnboarded`. The plan's round-3 #1 codifies "menu re-entry always
opens at step 1"; `openWizard()` enforces this by checking the `source` argument.

`ctx.dialogs.openOnboarding` returns a `Promise` that resolves once the wizard is actually up (the handler loads
settings and probes for Full Disk Access first), so a caller can act on the open wizard. The dev-only dialog gallery is
the one that does: it dispatches `cmdr.openOnboarding` and then `setCurrentStep(...)` to preview a specific page, since
step 1 won't advance without a real Allow / Deny (`lib/dialog-gallery/DETAILS.md`).

**Why no Linux menu entry**: the wizard's design language is macOS-centric (frosted backdrop matches macOS sheets,
"Restart Cmdr" copy assumes the Quit & Reopen flow, FDA-relevance). Adding a redundant menu entry next to the palette
command would clutter Linux's GTK menu bar for marginal benefit. Palette discovery is good enough; the upgrade-nudge
toast names it on first launch after upgrade.

### Upgrade nudge

Existing users (anyone with `isOnboarded === true` and `onboarding.upgradeNudgeShown === false`) see one `info` toast on
the first launch after they update past the wizard revamp:

- macOS: "We've added new onboarding options. Open Cmdr > Onboarding… to review them."
- Linux: "We've added new onboarding options. Open the command palette and run Onboarding… to review them."

The toast fires from `resolveOnboardingMount()`'s two wizard-skipping branches (so it only runs when the wizard is NOT
mounting; no need for an extra `onboardingShowing` check). It writes `onboarding.upgradeNudgeShown = true` synchronously
after firing, so it never appears again on the same machine.

The toast is suppressed under `isE2eRun()` so it doesn't leak into Playwright's first-spec-of-the-run state (each E2E
shard gets its own fresh data dir, so the nudge would otherwise fire once per shard launch and trip the fixture safety
net). The firing logic itself is unit-tested in `routes/(main)/startup-gates.test.ts`; the E2E suppression is a
target-mode gate, not a behaviour change.

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
2. **Comparison table**: "without AI vs with AI" for Search, Mass-rename, Select. Each row label carries its own glyph
   (`search` / `pencil` / `list-checks`), `aria-hidden` because the label beside it already names the row. The "With AI"
   column is the rightmost and carries the accent flair (tint + sparkle) to draw the eye. The feature column's header is
   `sr-only`: the row labels say what each row is, so a visible "Feature" over them was noise, but the header still has
   to EXIST or a screen reader hears a blank with every cell in that column.
3. **Three radio choices, in the order no AI → local → cloud.** Cheapest commitment first, so all three cards fit above
   the fold and only the last one opens a provider panel underneath it; before, cloud sat first and its panel pushed the
   other two options off screen. The pre-selection comes from the persisted `ai.provider` (default `off`), so a
   crash-then-resume user lands on their previous pick. Picking cloud reveals `CloudProviderPicker.svelte` (left) and
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

### What "off" turns off

"Thanks but no thanks" is the clearest answer a user can give, so `StepAi.persist()` lands it on every piece of state
that says AI is on, not just the one radio it came from. Three of them exist, in two different stores:

1. **`ai.provider = 'off'`** (registry setting).
2. **Ask Cmdr consent, revoked** via `ask-cmdr/ask-cmdr-consent.svelte::revokeConsent()`. This is NOT a setting: it's a
   record in `main.db` behind the consent commands, and it's what `settings/sections/AskCmdrSection.svelte`'s "Ask Cmdr
   is on / off" status reads. Without this step, a user who declined AI in the wizard still found Settings saying "Ask
   Cmdr is on".
3. **`askCmdr.proactive = false`**. The registry ships it `default: true` on purpose (the other gates keep it harmless
   for someone who never opted into AI), so it stays armed unless something explicitly turns it off. An explicit "no AI"
   is exactly that something.

The end state is `NeedsConsent` for `WakeReadiness`, so the status corner's wake indicator stays silent.

**❌ Picking cloud or local never grants consent.** Only the `'off'` branch touches consent, and only in the revoking
direction. Consent is a separate deliberate act behind the disclosure copy (`askCmdr.consent.*`, shown in the rail's
gate and the settings section), and the backend enforces it structurally in the send path; granting it as a side effect
of choosing a provider would be a consent bypass. `StepAi.test.ts` asserts the absence explicitly for both branches.

**❌ Switching back from `'off'` to a provider does not re-arm `askCmdr.proactive`.** Turning AI on again shouldn't
silently restart an agent that starts conversations on its own; Settings › AI › Ask Cmdr is where that goes back on.

**Revoking has a real backend side effect, by design**: `agent::wake::inbox::Inbox::purge_if_consent_withdrawn` drops
the proactive pipeline's stored rows once readiness no longer permits them. That's the point of an explicit "no", and
it's why the cloud/local branches must not reach this call.

**Neither failure strands the user.** `revokeConsent()` is a no-op for someone who never consented (the store deletes
two absent `meta` rows), and both it and the surrounding persist are wrapped: a revoke that throws is logged through the
module's `getAppLogger` and the rest of the persist still runs, and any other persist failure is logged and still
advances. The footer's `advanceBusy` guard always clears in a `finally`. Same reasoning as the no-key-blocks-advance
rule above: the wizard never traps someone on a step.
### The missing-API-key gate (confirm once, never block)

Cloud picked, the provider's `requiresApiKey` set, and `getAiApiKeyStatus(providerId).isSet === false`: the first Next
doesn't advance. It sets `footerNote` in `onboarding-state`, and the wizard renders it as a warning glyph plus one
sentence to the LEFT of the footer buttons, vertically centred on the button label at one line or two. The second Next
persists and advances. So the no-key-blocks-advance rule still holds (nothing is ever hard-blocked), and nobody sails
past a half-configured AI setup without hearing about it.

The state (`keyWarningShown`) clears on the next thing the user does, document-level and capture-phase so a control that
stops propagation still counts. **Events inside `.wizard-footer` are exempt, and that exemption is what makes the gate
work at all**: the second press has to reach `handleGoToBeta` with the flag still set. It can't be "ignore the click
that raised the warning" either, because Enter on the focused button fires `keydown` BEFORE the click, so a blanket
clear would disarm the gate one beat before the press it belongs to.

A failed keychain read logs and returns `false`: a wizard the user can't leave is worse than a warning they don't get.

### Connection-check pipeline

The steps, the key persist, and the connection check all belong to `$lib/ai-provider-setup/`, shared with Settings › AI
› Provider; `apps/desktop/src/lib/ai-provider-setup/DETAILS.md` owns the mechanism. `CloudProviderSetup.svelte` is the
wizard's frame around it: the provider header, and a quiet status line reading `controller.status` / `controller.error`
(Settings renders the same states as a row with recheck buttons).

The one rule that's the wizard's own: it never disables advance based on connection status. The auto-check is purely
informational, so a user who wants to fetch their key later isn't trapped on step 2.

### `pushConfigToBackend()` belt-and-braces

The `settings-applier.ts` listener also calls `pushConfigToBackend()` on any `ai.provider` / `ai.cloudProvider` /
`ai.cloudProviderConfigs` change, so the wizard's explicit `await` is redundant in the steady state. The reason it's
there: the listener fires per-setting-change, so if the user flips three settings in one tick we get three async
invocations racing the wizard's `onComplete()`. The explicit `await pushConfigToBackend()` in `StepAi.persist()` orders
the backend reconfigure before the user lands in the app deterministically.

## Step 3 (Open beta)

`StepBeta.svelte`: David's personal open-beta intro, the analytics disclosure, an optional contact channel, and the
required terms acceptance. Four blocks:

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

Between 2 and 3 sits the **crash-report disclosure**, a caption with no switch of its own. `updates.crashReports`
defaults ON, and a default that sends something has to be disclosed where the analytics one is rather than only in
Settings. It carries no toggle deliberately: the switch lives in Settings > Updates & privacy, and this step already
asks enough of a first launch. It IS, though, the only place a first launch hears about that default at all (an existing
install gets the CHANGELOG instead, and there's no in-app notice), so ❌ don't drop the caption without giving new users
that disclosure somewhere else.

The analytics and email blocks reuse `settings/sections/UpdatesSection.svelte`'s exact wiring: the email field runs on
the shared `settings/sections/beta-email-signup.svelte.ts` (`createBetaEmailSignup()`: the `betaSignup` call, the
email-pattern + `lastSubmittedEmail` resend guard, the typed success/failure feedback), so the onboarding page and
Settings behave identically.

The footer has two buttons: a secondary **Start using Cmdr!** that finishes onboarding right here (skipping the optional
step, via `requestWizardComplete()`) and a primary **One more optional setup step** that `nextStep()`s to the Optional
step. There is no skip-to-finish that bypasses this page: every first-launch user sees the analytics disclosure once,
because the AI step always lands here and both buttons start from this page.

### Terms acceptance

A fourth block closes the page: a required checkbox reading "I've read and agree to the terms and conditions", with the
phrase linking `TERMS_URL` through `openExternalUrl` like every other link on the step. It's the last thing on the last
page a user can't skip, and it gates both footer buttons.

**Why a checkbox at all.** "By downloading, installing, or using Cmdr, you agree to these terms" is browsewrap, which US
courts treat as presumptively unenforceable absent a deliberate act of assent (Berman v. Freedom Financial Network, 30
F.4th 849 (9th Cir. 2022)). Without that act, every liability limit in the terms rests on nothing. The tick IS the act,
so: never pre-tick it, never add a path that reaches the app around it, and keep the consent sentence an unconditional
first-person statement of agreement (the parity test pins it word for word for exactly that reason).

**Why the version, not a boolean.** Consent to a superseded document isn't consent to the current one. `TERMS_VERSION`
(`$lib/legal/terms`, the terms page's `lastUpdated` date) is stored with the acceptance as
`onboarding.termsAcceptedVersion` + `onboarding.termsAcceptedAt` (an ISO instant). On mount the step ticks the box only
when the stored version EQUALS the current one, so bumping the constant after a terms change re-asks everyone. Unticking
clears both fields (to `''`) rather than leaving a stale record we couldn't stand behind. The write doesn't wait out the
settings save debounce: it `setSetting`s both, then `forceSave()`s, because a quit right after the tick would otherwise
lose a record of consent.

**Why the buttons are blocked, not disabled.** While the box is unticked, both footer buttons carry `blockedReason`
(`WizardFooterButton`), which the wizard renders as `aria-disabled` + dimmed + `not-allowed` cursor + the reason as a
tooltip, while keeping the button focusable and still firing `onclick`. The handler then calls `revealTermsCheckbox()`:
`scrollIntoView({ block: 'center' })` (`behavior: 'auto'` under `prefers-reduced-motion`) plus focus on the checkbox
itself. A truly `disabled` button fires no click and takes no focus, so a user who pressed it would get silence, and a
keyboard user couldn't even reach it to find out why the wizard won't move. Focusing the control, not just scrolling to
it, is what makes the keyboard path equal to the pointer one.

**That focus MUST pass `preventScroll: true`.** Ark's hidden checkbox input is a real 1×1 element, off screen when the
press happens, so a plain `focus()` runs its own scroll-into-view, and in WKWebView that cancels the smooth
`scrollIntoView` above and wins with a near-no-op. The press then does nothing visible while focus sits on a control the
user can't see, which reads exactly like a dead button and an unclickable checkbox; the fix is a step back and forward,
which remounts the step. Measured in the running app before the fix: 4 runs out of 4 ended ~350 px short of the terms
block, and only when the input wasn't already focused, which is what made it look intermittent. With `preventScroll`, 3
runs out of 3 landed correctly.

**Marking it required.** The red asterisk after the block heading is `aria-hidden` decoration; `<Checkbox required>`
puts `aria-required` on the control, which is what a screen reader announces. Neither alone is enough (see
`docs/design-system.md` § "Checkbox and radio group").

## Step 4 (optional setup)

Four toggle blocks, each bound to an existing registry setting via `<SettingSwitch>`. Defaults stay ON; the step is
about letting the user turn things OFF with full context, not about asking for opt-in.

### The info glyph

Each card shows a half-line `*.summary` and parks its full `*.desc` in a tooltip behind an info glyph beside the title.
Four paragraphs of prose made the LAST step of onboarding a wall of text, which is the worst place for one: the user is
trying to get into the app. The summary carries the trade-off in a line; the tooltip is there for whoever wants the why.

The glyph is a `<button>`, not a decorated span, so the tooltip opens on Tab as well as hover (the action fires on
`focus`, which is also why a native `title` is banned app-wide). Its content goes through the tooltip action's
`contentEl`, so it can carry real `<p>` and `<ol>` elements rather than one run-on line; `OnboardingToggleCard` renders
it into a `<div hidden>` host and hands over the INNER element, since an adopted element keeps its own `hidden`.

That host sits outside `.toggle-text` on purpose: a hidden sibling in there would still count for `:last-child`, and the
summary above it would keep a paragraph gap under it with nothing to separate.

The card's `captionPlacement="inline"` moves "Recommended: on" onto the switch's left, so the verdict reads as the
switch's own label instead of a footnote under it. The rest of that caption ("You can change this any time in Settings")
closes each tooltip as its own paragraph, where there's room for it.

Each sentence of the tooltip copy sits on its own line: the catalog values carry real `\n`s and `.toggle-desc` is
`white-space: pre-line`. Six sentences in one block is the wall of text the info glyph existed to avoid, and CSS can't
break at sentence boundaries. A translation without the newlines still renders, as one paragraph.

**Lists here and on step 3 are a flex row: a `counter()` in `::before` plus the sentence in one span.** They sit flush
with the paragraphs around them, and only a wrapped line hangs in under the words. ❌ Not `text-indent`, the obvious way
to write that: it INHERITS, reaches the anonymous flex item inside any `inline-flex` descendant, and pulled the keys out
of a `ShortcutChip` and across the sentence beside it. And the sentence needs its span because flex makes every element
child its own item, so a leading `<LinkButton>` would be cut off from the ": …" after it by the row's own gap
("GitHub : Add issues").

Copy that reaches a tooltip goes through `<Trans>`, which renders TEXT, never HTML, so an HTML entity in the catalog
shows up literally: `&lt;DIR&gt;` used to render as `&lt;DIR&gt;` on screen, and `2&ndash;3` as `2&ndash;3`. The en dash
is now the literal character, and the folder-size placeholder is passed in as `{dirPlaceholder}` from
`fileExplorer.dirSize.dirPlaceholder`, so the sentence can't name a placeholder the Size column doesn't actually show.

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
`crates/cmdr-index/src/indexing/lifecycle/DETAILS.md` § "FDA-deferred root auto-start".

A third thing reads the permission at launch without being part of that gate: the one-shot first-run pane layout, which
opens the right pane on `~/Downloads` only when Cmdr already has Full Disk Access. It probes with
`checkFullDiskAccessQuiet` (like step 1's poller, never the loud `checkFullDiskAccess`) and, on anything other than a
grant, leaves both panes on `~` so no TCC dialog can appear before the user has decided anything. It deliberately does
NOT read `onboarding.completed`, which flips to `true` in the same boot for a fresh install on an already-granted Mac.
The rule and its guardrails: `file-explorer/pane/DETAILS.md` § "First-run pane layout".

## Mount + onboarding flag

`routes/(main)/startup-gates.ts::resolveOnboardingMount` decides whether to mount the wizard (unit-tested row by row in
its sibling `startup-gates.test.ts`):

- `CMDR_FORCE_ONBOARDING=1` → mount wizard.
- `hasFda && isOnboarded` → no wizard; mirror `fullDiskAccessChoice` to `'allow'` if needed.
- `hasFda && !isOnboarded` → no wizard; mirror setting + call `notifyOnboardingComplete()` (covers pre-wizard users who
  already granted FDA).
- `deny && isOnboarded` → no wizard (user denied and finished onboarding).
- Anything else → mount wizard.

The onboarded flag is the hidden `onboarding.completed` setting. It flips to `true` on full wizard completion via
`notifyOnboardingComplete()` (from `$lib/updates/updater.svelte`), so the auto-update "restart to apply" toast doesn't
fire during first-launch onboarding.

While the wizard is up, the updater's mirror flag is set too, so it suppresses the deferred toast. Both writes go
through `+page.svelte`'s `setOnboardingVisible()`, the single seam for opening and closing the wizard (see
`routes/(main)/DETAILS.md` § Startup gates), and `handleWizardComplete` flips it back. See `$lib/updates/CLAUDE.md` §
"Onboarding gating".

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

## The language escape hatch

Cmdr resolves the UI language from the Mac's ordered language preferences
(`apps/desktop/src-tauri/src/intl/DETAILS.md`), so a first launch can open in a language the user doesn't read. Every
other way to change it — Settings > Appearance > Language, the command palette — is labeled in that same language, so
the way out has to be on the screen they're already looking at. That screen is the wizard.

**It's frame, not a step.** The header is a three-column grid (empty cell, step dots, picker), which keeps the dots
centred and leaves the step sequence untouched: step 3's consent page is non-skippable, and a language step would have
been one more thing between a user and the app.

**Nothing about it needs reading.** The globe glyph carries the meaning, and every row is its own endonym, resolved in
the option's OWN locale by `languageOptions()` (`settings/definitions/appearance.ts`): the `en` row reads "English"
whatever the app currently speaks, with no untranslated-string exception to maintain. The `'system'` row names what it
resolves to ("System default (Svenska)").

**Wiring is `SettingSelect` on `appearance.language`**, the same control the Settings picker uses, so a pick goes
through `setSetting()` → `settings-applier.ts` → `setLocale()` and the open wizard re-renders in place. No restart, no
special case. An explicit pick retires `'system'` permanently, which is the point: an implicit choice stays implicit and
an explicit one is permanent, so a resolved tag is never written back (`$lib/intl/DETAILS.md` § What `'system'` resolves
to).

**The menu portals into the wizard OVERLAY, not into `document.body`.** Body would put it under the scrim
(`--z-dropdown` < `--z-modal`) and outside `use:trapFocus`, whose leak guard would yank focus back out of the menu the
moment zag focuses its content. The overlay is inside the trap and above the panel, and still escapes the panel's
`overflow: hidden`, which would otherwise clip a menu whose selected row sits near the bottom of the list.
`ui/DETAILS.md` § Select covers the `portalContainer` prop.

**A pick here reports itself.** The picker passes `SettingSelect`'s `onPicked` to
`trackLanguageChanged('onboarding', …)`, which is half the population of the `language_changed` event (the Settings
picker is the other half). Somebody reaching for this control on first launch is the strongest evidence we get that
auto-selection landed badly, since nothing in the UI asks how a translation reads. Prop shapes and the rules that keep
the signal honest: `src-tauri/src/analytics/DETAILS.md` § The language events.

**No notice for already-onboarded users.** David cut the one-time banner/toast half deliberately (2026-08-19): an app
that switches to your own language and says nothing is behaving normally, not doing something that needs announcing.
Don't build it "just in case"; the `CLAUDE.md` bullet is the enforcing line.

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
first-launch user must see the usage-stats disclosure once (the opt-out default only reads as fair consent if it was
actually shown). So the AI step's only forward button ("Next") always `nextStep()`s to Beta. The Beta page itself offers
"Start using Cmdr!" (finish) and "One more optional setup step" (continue), so both forward paths start from Beta and
the user can't reach the app without seeing it. Don't re-add a skip-to-finish button on the AI step (it would bypass the
disclosure). The user can still opt out and skip the email on the Beta page, they just can't skip seeing it.

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
- `$lib/settings`: `getSetting`, `setSetting`, `forceSave` (the `onboarding.*` keys: the FDA choice, the completed flag,
  and Step 3's terms acceptance)
- `$lib/legal/terms`: `TERMS_VERSION`, `TERMS_URL` (Step 3's terms checkbox)
- `$lib/shortcuts/key-capture`: `isMacOS`
- `$lib/system-strings.svelte`: localized system pane names
- `$lib/ui`: `Button`, `LinkButton`
- `$lib/beta-links`: `GITHUB_REPO_URL`, `GITHUB_ISSUES_URL`, `BOOK_A_CALL_URL`, `ABOUT_DAVID_URL` (Step 3's feedback +
  star-CTA links; shared with `AboutWindow.svelte`)
- `@tauri-apps/plugin-process`: `relaunch` (Allow-path footer button)
