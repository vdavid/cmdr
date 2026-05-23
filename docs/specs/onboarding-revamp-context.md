# Onboarding revamp — context bundle

Frozen record of every decision David made before the plan was written. The plan lives in `onboarding-revamp-plan.md`;
this file is the "why" the plan refers back to. Don't edit this once the plan is approved — re-open the discussion in
the plan instead.

## The change in one sentence

Replace the single Full Disk Access (FDA) modal with a multi-step, soft-sheet onboarding wizard that covers ~90% of the
viewport over the main UI, with two required steps (FDA decision, AI provider) and one optional step (Networking /
Indexing / Updates / MTP), plus the ability to re-open from the menu and command palette.

## David's draft copy (verbatim — do not paraphrase in implementation)

### Step 1 — Full Disk Access

```
Welcome to Cmdr!  {about 20% larger than body font, NOT a giant hero}

**You probably just want to start using the app.** Sorry to bother you with this
first, but it's needed.

You see, Cmdr is a file manager, and it needs to access your disk to see all
your files. macOS doesn't automatically grant permission to this.

Would you like to give this app full disk access? Here's what that means:
{revoked-state copy from FullDiskAccessPrompt.svelte if applicable}

- **Pro:** The app will access your entire disk without nagging you for
  permissions to each folder like Downloads, Documents, and Desktop.
- **Con:** Full disk access is pretty powerful. It lets the app read any file
  on your Mac. Only grant this if you trust Cmdr. Cmdr uses this right
  respectfully, and is [source-available](https://github.com/vdavid/cmdr) if
  you feel unsure.

If you decide to allow: {from here, same content as FullDiskAccessPrompt.svelte}
```

### Step 2 — AI

Three branches depending on FDA outcome:

- **FDA granted (detected via `checkFullDiskAccess()` on step transition):**
  `Thanks for granting Full Disk Access! Now, the app can access your disk. Great!`
- **FDA denied:**
  `You chose not to enable Full Disk Access. We respect that. You'll then shortly get a few permission requests from macOS for Cmdr to access your Desktop, Downloads, and similar folders. Accept/reject these at will. You can change all of this later in your System Settings.`
- **User clicked "Allow" but FDA still not granted (e.g. didn't toggle in Settings, or didn't restart):**
  `You said you wanted to enable Full Disk Access, but Cmdr doesn't seem to have gotten it. You might need to restart the app (do it now! We'll continue from here!) or go to your System Settings > Privacy & Security > [Full Disk Access](deep-link via openPrivacySettings) and find Cmdr or manually add it with the little "+" button at the bottom.`

Then:

```
Now, the last necessary step: AI stuff

Cmdr has a bunch of AI features that you _may_ want and may not want. AI is a
controversial topic these days.

Here is how you do common actions with and without AI:

| Feature     | With AI                                                                                     | Without AI                                                                                              |
| ----------- | ------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------- |
| Search      | You say "my recent fish-related presentations", agent sets your filters                     | You type something like "*fish*.ppt", and select the "after 1st of this month" filter                   |
| Mass-rename | You say "add ISO date prefix", agent sets your rename pattern, you review and apply at will | You use the batch rename UI to manually set the rename pattern, review and apply                        |
| Select      | You say "select all image files", agent suggests selection, you review and apply at will    | You press `⌘+` and type something like "*.jpg,*.png,*.gif,*.heic,*.webp,*.jpeg", review and apply       |

Based on this, do you want AI or not?
- [ ] Yes, I want AI (recommended), and my AI provider is: {selected}
    - LEFT column: scrollable selector list with all 15 cloud providers from
      `cloud-providers.ts` (OpenAI, Anthropic, Google Gemini, Groq, Together AI,
      Fireworks, Mistral, OpenRouter, DeepSeek, xAI, Perplexity, Azure OpenAI,
      Ollama, LM Studio, Custom). All 15 present; smaller number visible at once;
      scrollable. Keyboard: arrow keys + type-to-jump.
    - RIGHT column: setup instructions for the selected provider, with the API
      key input, auto-tester, and model selector embedded as numbered tutorial
      steps. Steps get a checkmark as they're completed (e.g., "Step 3: Paste
      your API key here" gets a ✓ when the auto-check returns connected, then
      "Step 4: Pick a model" gets ✓ when one is selected). Live, no manual
      check button needed.
- [ ] Yes, I want AI, but I want to be super private, and I don't mind a bit
  dumber model that takes up about 2 GB of space and a bit of CPU at every use.
  (Still an okay solution. No data leaves your machine. Cmdr tries to deliver
  updates for the best small local model available.)
- [ ] Thanks but no thanks, no AI for me

Buttons: [Start using Cmdr!] [One more optional setup step]
  {The second button has the accent color, the first is bare/secondary. This is
  intentional: we want to nudge users toward step 3 without forcing them.}
```

### Step 3 (optional)

```
(Optional) Step 3

Nice! You're _almost_ ready to use Cmdr, and you chose to do a detailed setup.
So, here you go, a few easy choices — if you don't care too much, just click
the button below, all of these are just options, and the defaults are for your
benefit:

1. Do you want **Networking** _on_ or _off_?
   Having it _on_ means you can connect to SMB servers like company network
   shares, a home NAS, and the such. The only cost is a macOS permission
   dialog that pops up and asks you to allow "Local network access", and one
   for "Accepting incoming connections". Both dialogs are harmless, but if you
   don't know what these are, they might be scary or annoying.
    - [x] Networking on (recommended)
    - [ ] Networking off (can enable it in Settings later)

2. Do you want **Drive indexing**?
   Drive indexing is totally cool! Gives you two main things: 1. Instant
   search of your whole drive. Think Spotlight, but even faster. 2. Real-time
   folder sizes for your whole drive. You always know how much stuff you have
   in each folder. If you turn this off, you only get `<DIR>` for the sizes.
   The cost is a 300 MB index on your drive, but no extra CPU or memory use
   after the first 2–3 minutes of you first starting the app, or starting it
   after a long time. It's a cheap feature considering the benefits.
    - [x] Drive indexing on (recommended)
    - [ ] Drive indexing off (can turn it on later in Settings)

3. **Automatic updates**?
   If you enable this, Cmdr makes a tiny network request to a central license
   server at each app start plus once every 24 hours, and you always get the
   latest updates. If disabled, you'll keep your current version, and zero
   automated network requests (except for periodic license checks _if_ you
   have a Commercial license).
    - [x] Updates on (recommended)
    - [ ] Updates off

4. MTP?
   If you enable this, Cmdr can **connect to Android phones, Kindles, cameras**,
   some music players, and any other device that supports the protocols called
   MTP or PTP. The cost is that macOS _also_ wants to connect to these (and
   it usually fails — that's why you can't just use Finder to copy photos from
   Android phones), so Cmdr has to suppress that macOS process while it's
   running. When you quit Cmdr, this is politely restored. But it's a bit of
   a cost, so:
    - [x] MTP support on (recommended)
    - [ ] MTP support off

Button: [Start using Cmdr]
```

## Resolved questions (with David's exact answers)

### Pre-questions round 1

1. **AI default**: change from `'local'` to `'off'`. The existing post-FDA AI offer toast goes away — the wizard becomes
   the only path to enable AI on first launch.
2. **Cloud providers in step 2**: show all 15 from `cloud-providers.ts`, scrollable list. Not a curated subset.
3. **"Bring a Claude Code / ChatGPT subscription"**: out of scope for now. API key only.
4. **FDA "allow but didn't grant" detection**: one-shot `checkFullDiskAccess()` call on step 2 entry, choose copy branch
   from the result. No polling.
5. **Step 3 settings**:
   - Networking on/off: set `network.enabled`. Don't proactively trigger the macOS Local Network prompt (it fires on
     first SMB action via the existing `network.firstTriggerDone` flow).
   - Drive indexing on/off: set `indexing.enabled`. Settings-applier already starts/stops the runtime. Cache cleanup
     behaviour is out of scope here.
   - Auto updates: set `updates.autoCheck`.
   - MTP: set `fileOperations.mtpEnabled`. Step 3 is mostly about giving users a chance to turn things OFF with full
     context. Defaults stay on.
6. **Soft dialog component**: new component `OnboardingWizard.svelte` (not a variant of `ModalDialog`). Own backdrop
   blur, no drag, no Escape, no × button, full-bleed rounded panel sized to ~90% of viewport. Adds `'onboarding'` to
   `SOFT_DIALOG_REGISTRY`.
7. **Existing users on upgrade**: silent skip of the wizard (they have `isOnboarded: true`) PLUS a one-time `info` toast
   nudging them that the `Cmdr > Onboarding…` menu item now exists, so they can review the new options if curious. Toast
   fires once and never again (gated by a new hidden setting, e.g. `onboarding.upgradeNudgeShown`).
8. **App behind wizard**: render the full app behind the wizard backdrop normally (no "white screen until wizard done").
   First-launch lands on `~`, so what peeks through the edges is friendly.
9. **Keyboard contract**: Tab cycles within step; Enter on primary advances; Escape disabled (no accidental dismiss);
   provider list supports arrows + type-to-jump.
10. **Mount point**: `routes/(main)/+page.svelte`. Replace `showFdaPrompt` with `showOnboarding`, replace
    `handleFdaComplete` with the wizard's overall `onComplete`.

### Pre-questions round 2

1. **Re-invocation from menu / palette**: always start at step 1. If FDA is already granted, step 1 copy reflects that
   ("Cmdr currently has Full Disk Access. You can revoke any time in System Settings.") with a single "Next" button.
2. **Back button + FDA**: leave the FDA buttons live on step 1. User can change their mind any time. "Allow" still
   requires the real macOS grant + restart; the wizard cannot fake it.
3. **Step 3 indexing cache cleanup**: out of scope. Step 3 only flips `indexing.enabled`; existing settings-applier
   handles the runtime stop.
4. **Subscription auth placeholder**: leave out entirely. No "Coming soon" affordance.

### Pre-questions round 3

1. **Local-model download timing**: kick off download in the **background as soon as the user picks the private/local
   option in step 2**, with no in-wizard progress UI. If they switch away, cancel; if they switch back,
   re-`startAiDownload()` and let the existing HTTP-Range resume pick up. The existing toast can show in the corner if
   it does — fine either way; do NOT add suppression logic for it.
2. **Mid-flow crash recovery**: each step persists its decision on advance. `isOnboarded` only flips on full completion.
   Next launch starts at the first not-yet-decided step. After step 1 + restart, the user lands directly on step 2
   (which is the dominant "Allow + restart" flow).
3. **Linux**: skip step 1 entirely. Step 2 leads with `Welcome to Cmdr!` and no FDA-related copy.
4. **Step indicator**: subtle dot row at the top, with the optional step's dot styled distinctly (open / muted) so users
   see "2 mandatory + 1 optional", not endless.
5. **Back button**: `←` button bottom-left with tooltip `Back`. Always lets the user return to a previous step.
6. **Re-entry points**: add `Cmdr > Onboarding…` menu item (place under "Check for updates" in the app menu) and add a
   command-palette command for the same. Both routes call the same trigger.

### Pre-questions round 4

1. **Menu re-invocation lands on step 1.**
2. **Back from step 2 leaves FDA buttons live** (the test for FDA-already-granted collapses step 1 to a single-Next
   variant, see round 2 #1).
3. **Step 3 indexing-cache cleanup out of scope** (confirmed twice).
4. **Subscription placeholder excluded** (confirmed twice).
5. **Design shift sanctioned**: lift cues from the recently redesigned Settings (more rounded, macOS-sheet vibe, frosted
   backdrop). Any new tokens or patterns introduced for the wizard go into `docs/design-system.md`. Don't leave them
   stranded in the wizard's scoped styles.

## Things to preserve from `FullDiskAccessPrompt.svelte`

- The TCC re-probe before `openPrivacySettings()` (without it Cmdr doesn't appear in the FDA list — critical).
- Ventura vs older copy switch via `getMacosMajorVersion()`.
- The "Tip: click '+' button at the bottom" fallback for the macOS 26 Tahoe regression. Lives in step 1's instructions
  and the "didn't get FDA" branch copy on step 2.
- `systemStrings.systemSettings` for the localized System Settings pane name.
- `startIndexingAfterFdaDecision()` on Deny.
- The "Cmdr is source-available" GitHub link in the Con bullet.

## AI toast machinery cleanup (scope)

The post-FDA "Download AI?" offer toast must go away:

- Delete the `'offer'` state in `AiToastContent.svelte` (and its switch case).
- Delete the `pendingOffer` field and `notifyAiOnboardingComplete()` from `ai-state.svelte.ts`.
- Delete the `onboarded` gate that suppresses Offer at startup (the wizard now owns first-launch AI consent).
- Stop calling `notifyAiOnboardingComplete()` from `routes/(main)/+page.svelte`.
- Keep the runtime toast states (`downloading`, `installing`, `ready`, `starting`) — they're useful while the local
  model downloads after a wizard pick.
- The `dismissAiOffer` and `optOutAi` Tauri commands may have no callers after this cleanup. Decide per-call-site
  whether to delete them or keep them for future settings-side use.

## Testing strategy

- **Env vars** (mirror the `CMDR_MOCK_LICENSE` pattern):
  - `CMDR_FORCE_ONBOARDING=1` (frontend, in `routes/(main)/+page.svelte`) — override the `isOnboarded` gate so the
    wizard always shows.
  - `CMDR_MOCK_FDA=granted|denied|notgranted` (backend, in `permissions.rs::check_full_disk_access`) — override the TCC
    probe so all four step-2 branches can be tested without ever opening real System Settings.
- **Tier 3 Vitest** (component a11y + behaviour): one file per step. Mount, walk keyboard, assert.
- **Tier 2 Playwright**: one spec walking the full happy path (Allow + grant, pick cloud → enter mock API key → pick
  model → step 3 → finish) and the edge-case branches (Allow + didn't grant, Deny, Linux skip-step-1, re-entry from
  menu).
- **Real-API smoke**: David's OpenAI key lives in macOS Keychain:
  `security find-generic-password -s OPENAI_API_KEY -a veszelovszki -w`. Use `gpt-5.5` model. He has $2500 credits
  expiring in a week — go wild. Use this to verify the cloud connection-check pipeline ends-to-ends with a real
  provider, at least once per milestone that touches AI.
- **Each E2E test ≤ 1–2 s**. If a wizard spec takes longer, restructure.
- Don't fix the pre-existing failing test
  `File viewer selection and copy › drag within viewport selects the dragged range` — another agent has it.

## Notes for the planning agent

- The plan goes in `docs/specs/onboarding-revamp-plan.md` (sibling of this file).
- Reference this file with relative links; don't restate David's copy verbatim in the plan, just point back here.
- Use milestones. Each milestone must end with a committable state, full `./scripts/check.sh` green, and `--only-slow`
  green (per the user's workflow). Suggested milestones (the planner is free to refactor):
  1. Foundations: context bundle (done), wizard skeleton component, dialog registry entry, mount point swap, env-var
     mocks, settings registry additions, AI default flip, AI toast cleanup. End state: wizard renders an empty 90% sheet
     with step dots and Back button.
  2. Step 1 (FDA): port and adapt `FullDiskAccessPrompt.svelte` content into the new step. Re-entry variant when FDA
     already granted. Linux skip.
  3. Step 2 (AI): the meaty one. Provider list (left), per-provider instructions with embedded API-key flow (right),
     three radio choices, three FDA-state copy branches, local-model background download orchestration.
  4. Step 3 (optional): four toggles with the long-form explanations.
  5. Re-entry: menu item, command palette command, upgrade-nudge toast for legacy users. Plus `--only-slow` and a
     real-API smoke run.
  6. Polish: design-system updates, docs sweep (architecture.md, onboarding/CLAUDE.md, ai/CLAUDE.md, ui/CLAUDE.md as
     applicable), a11y audits (tier 3 per component, tier 2 wizard spec), final `./scripts/check.sh --include-slow`.
- Read these in full before drafting:
  - `AGENTS.md`
  - `docs/architecture.md`
  - `docs/design-principles.md`
  - `docs/design-system.md`
  - `docs/style-guide.md`
  - `apps/desktop/src/lib/onboarding/CLAUDE.md`
  - `apps/desktop/src/lib/ai/CLAUDE.md`
  - `apps/desktop/src/lib/ui/CLAUDE.md`
  - `apps/desktop/src/lib/settings/CLAUDE.md`
  - `apps/desktop/src-tauri/src/fda_gate.rs`
  - `apps/desktop/src-tauri/src/permissions.rs`
  - `apps/desktop/src/lib/onboarding/FullDiskAccessPrompt.svelte`
  - `apps/desktop/src/lib/ai/AiToastContent.svelte`
  - `apps/desktop/src/lib/ai/ai-state.svelte.ts`
  - `apps/desktop/src/lib/settings/sections/AiSection.svelte`
  - `apps/desktop/src/lib/settings/sections/AiCloudSection.svelte`
  - `apps/desktop/src/lib/settings/cloud-providers.ts`
  - `apps/desktop/src/lib/settings/settings-registry.ts`
  - `apps/desktop/src/lib/ui/ModalDialog.svelte`
  - `apps/desktop/src/lib/ui/dialog-registry.ts`
  - `apps/desktop/src/routes/(main)/+page.svelte` (FDA orchestration around lines 420–570)
