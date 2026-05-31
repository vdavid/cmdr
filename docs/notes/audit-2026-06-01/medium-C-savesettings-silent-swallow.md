# saveSettings swallows persist failures with no log, losing FDA + onboarding state

**Severity:** medium **Lens:** C — Error handling **Confidence:** high

## Location

`apps/desktop/src/lib/settings-store.ts:62-78` (the `catch {}`) Callers that matter:
`lib/onboarding/StepFda.svelte:78,97`, `routes/(main)/+page.svelte:415` (`fullDiskAccessChoice`),
`lib/updates/updater.svelte.ts:63` (`isOnboarded: true`).

## What

`saveSettings()` wraps the whole `store.set(...)` + `store.save()` sequence in
`try { ... } catch { /* Silently fail - persistence is nice-to-have */ }`. It returns `void` either way — callers can't
tell the write failed, and nothing is logged. Three keys flow through it: `showHiddenFiles`, `fullDiskAccessChoice`, and
`isOnboarded`. The latter two are not "nice-to-have" — they gate the onboarding wizard and the Full Disk Access flow.

## Why it matters

If `store.save()` fails (disk full, the JSON store is locked by another process, the data dir became unwritable on a
flaky mount), the FDA "allow"/"deny" decision and the "onboarding complete" flag are silently dropped. On next launch
the app reads defaults (`fullDiskAccessChoice: 'notAskedYet'`, `isOnboarded: false`) and re-runs onboarding / re-prompts
for Full Disk Access — a confusing regression for the user, with zero log trail to diagnose since the `catch` is empty.
The same silence hides any corruption of the user's `showHiddenFiles` preference. This violates the "protect the user's
data" and "communicate what's actually happening" principles, and contradicts the AGENTS guidance that Tauri-API
failures be `await`ed inside try/catch _and logged_.

## Evidence

```ts
export async function saveSettings(settings: Partial<Settings>): Promise<void> {
  try {
    const store = await getStore()
    if (settings.showHiddenFiles !== undefined) {
      await store.set('showHiddenFiles', settings.showHiddenFiles)
    }
    if (settings.fullDiskAccessChoice !== undefined) {
      await store.set('fullDiskAccessChoice', settings.fullDiskAccessChoice)
    }
    if (settings.isOnboarded !== undefined) {
      await store.set('isOnboarded', settings.isOnboarded)
    }
    await store.save()
  } catch {
    // Silently fail - persistence is nice-to-have
  }
}
```

```ts
// StepFda.svelte:78  — FDA decision persistence
await saveSettings({ fullDiskAccessChoice: 'allow' })
// updater.svelte.ts:63 — onboarding completion
await saveSettings({ isOnboarded: true })
```

## Suggested fix

At minimum, log the caught error (`log.error('Failed to persist settings {keys}: {err}', ...)`) so a failed
FDA/onboarding write leaves a diagnosable trail. Better: let `saveSettings` reject (or return a success boolean) so the
FDA and onboarding call sites can react — retry, surface a toast, or at least not advance the wizard as if the choice
stuck. The `showHiddenFiles` toggle can stay best-effort, but it should still log on failure rather than vanish.

## Notes

Note this is the legacy `lib/settings-store.ts`, distinct from the newer `lib/settings/settings-store.ts` (which uses an
in-memory cache + debounced `scheduleSave()`). The FDA and onboarding flows still go through this legacy file. The two
`void saveSettings(...)` fire-and-forget calls in `DualPaneExplorer.svelte` (1222, 2308) only carry `showHiddenFiles`,
so they're lower-stakes, but they inherit the same silent-swallow.
