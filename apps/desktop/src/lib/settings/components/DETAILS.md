# Settings UI primitives details

Depth and rationale for the setting-row primitives. `CLAUDE.md` holds the must-knows that prevent silent breakage.

## Password-input modes

`SettingPasswordInput` runs in one of two modes based on whether `onchange` is passed:

- **Store-driven (default):** `id` is mandatory and the component reads/writes the settings store directly. Use for
  passwords persisted as plain settings.
- **Controlled:** when both `value` and `onchange` are provided, the component bypasses the store, treats `value` as
  external state, and calls `onchange(newValue)` on every input. Use when the value lives in the OS secret store (AI
  cloud provider API keys) or any other backing store that isn't `settings.json`. In this mode `id` is still required
  for label/aria fallbacks but isn't read or written.

The `$effect` that syncs `internalValue` from `externalValue` is gated on `onchange` being set, and the
`onSpecificSettingChange` subscription is skipped in controlled mode so secret-store updates don't get clobbered by
stale store reads.

## `SettingsSidebar` and `SettingsContent` live here, not a sibling dir

They're paired with the primitives because the settings window has exactly one consumer of each (the `routes/settings`
page). Splitting them into `lib/settings/window/` would add a directory boundary that crosses the same import edges
back. They share the `SettingsSection` wrapper styling and the same registry hooks, so they sit alongside the rest.
