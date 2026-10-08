# Managed policy (frontend)

The organization's MDM policy as this window shows it. The backend reads, decides, and enforces everything
(`src-tauri/src/managed_policy/CLAUDE.md`); this holds its `ManagedPolicyView` for the UI.

## Module map

- `managed-policy.svelte.ts`: the reactive view, fetched once per window and followed through `managed-policy-changed`
  (`initManagedPolicy`), plus `getSettingLock` / `isSettingLocked` (pinned) / `isSettingManaged` (any lock).
- `overlay.ts`: `lockedValue` and `lockAllowsWrite`, the pure twins of Rust `overlay` / `refuses_write`.
- `ai-refusal.ts`: `managedAiRefusalMessage`, the ONE copy map for `ManagedAiRefusal`; every surface holding one words
  it here.
- `ManagedPolicySummary.svelte` + `policy-summary.ts`: the "Managed by your organization" card (Settings › Updates &
  privacy), one line per restricted area, worded from the view alone.

## Must-knows

- **The frontend never decides policy.** It renders `lockedSettings` and the typed outcomes; ❌ never derive a lock from
  `usageStatsDisabled`, `ai.mode`, or a key name. A new lock goes in `managed_policy/locked.rs`.
- **The settings store owns the overlay.** `initializeSettings` starts this before `initialized`, so no reader sees a
  value the policy overrides, and `getSetting` returns the locked value. ❌ Never persist it: the policy overlays, it
  never rewrites, so removing the profile brings the person's own value back.
- **Pinned vs narrowed.** A `fixed` lock disables the whole row; a `disallowedValues` lock (`ai.provider` under
  `DisableCloudAI`) leaves the row usable and rules out only its values, so the control disables those options.
- **Restricted windows (viewer, queue) never fetch it**: they render none of these settings.
- **The barrel is imported lazily** (`await import('$lib/tauri-commands')`): the settings store imports this file, and a
  static barrel import would drag the whole IPC surface into every settings consumer.

Flows and decisions: `DETAILS.md`. Read it before any non-trivial work here: editing, planning, reorganizing, or
advising.
