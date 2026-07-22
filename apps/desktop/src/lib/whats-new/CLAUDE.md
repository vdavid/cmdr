# What's new popup (frontend)

After a silent auto-update, this popup shows the `CHANGELOG.md` slice between the version the user last saw and the one
running now, so the project's pace is visible. The backend (`src-tauri/src/whats_new/`) parses the embedded changelog
into a typed model; this frontend decides when to show it and renders it.

## Module map

- `whats-new.ts`: PURE decision logic. `decideWhatsNew(...)` → `show` / `stamp` / `wait` / `none`, plus numeric
  `compareVersions`. No `$state`, no IPC; unit-tested in `whats-new.test.ts`.
- `whats-new-trigger.svelte.ts`: the effectful layer. Owns `whatsNewState` (`$state`), reads/writes settings, fetches
  the slice over IPC, opens the dialog. Exports `runWhatsNewStartupTrigger` (auto), `openWhatsNew` (manual reopen),
  `closeWhatsNew`.
- `WhatsNewDialog.svelte`: the soft `ModalDialog` (`dialogId: 'whats-new'`), rendering releases via `snarkdown`.

## Must-knows

- **The lead renders in a `<div>`, never a `<p>`.** A lead can be block markdown (a numbered list → `<ol>`), invalid
  inside a `<p>`. Don't revert it.
- **`isOnboarded` discriminates fresh-install from inaugural-showcase** (both have no `lastSeenVersion`). Backwards, and
  either every fresh install eats a popup or the release shipping the feature never demos it. It lives outside the
  settings registry, so `+page.svelte` passes it in.
- **`wait` must NOT stamp** (onboarding or another startup modal is up → retry later); stamping would eat the changelog
  forever.
- **An auto-show with an empty slice silent-stamps**, never an empty popup (the empty state is manual-reopen only).
- **`{@html}` is trusted** (our committed `CHANGELOG.md`, backend-parsed): never feed it user input; fix bad entries in
  `CHANGELOG.md`, never add fixup logic here.
- **E2E boot suppresses the auto-check** (`maybeRunWhatsNew` early-returns unless `force`); `whats-new.spec.ts` drives
  the real path. Don't remove the gate.

The full show-once decision table, the manual and menu entry points (Help > What's new, Cmdr > Changelog…), the dev
override, the E2E seam, and the smoke checklist: `DETAILS.md`. Read it before any non-trivial work here: editing,
planning, reorganizing, or advising.
