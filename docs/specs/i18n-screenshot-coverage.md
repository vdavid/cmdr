# Screenshot coverage: every dialog a translator can be shown

**Status**: every milestone below is DONE. **Owner**: David.

Translators get a `@key.screenshot` per string so they can see where their words land. Coverage went from 1549 / 2743
keys (56%) at `c1710d66c` to **2046 / 2743 (75%)**: 1178 direct (up from 910) and 868 representative. The run went from
68 surfaces with three dead passes to **133 surfaces, 0 failed, every pass green**.

Areas now fully covered: `ai`, `crashReporter`, `errors`, `mtp`, `queryUi`, `search`, `shortcuts`, `updates`, `viewer`.
Biggest remaining gaps: `settings` (226 uncoupled, mostly `settings.mediaIndex` behind conditional UI), `fileExplorer`
(147), `askCmdr` (76).

Area docs to read first: `apps/desktop/src/lib/intl/messages/DETAILS.md` § Screenshots, `docs/guides/i18n.md` §
Screenshots, `apps/desktop/test/e2e-playwright/CLAUDE.md` (overlay-closing rules, the `ensureAppReady` focus contract),
`apps/desktop/src/lib/dialog-gallery/DETAILS.md`.

## The approach that carries most of this

**Drive the dialog gallery from its registry instead of hand-staging dialogs.** `DIALOG_GALLERY_ENTRIES`
(`src/lib/dialog-gallery/gallery-registry.ts`) already enumerates every registered soft dialog with its reviewable
states, and the main window already listens for `debug-open-gallery-dialog` to open any `(dialogId, stateId)` with
fixtures wired. `apps/desktop/test/e2e-playwright/i18n-capture-gallery.ts` walks that registry, so a dialog gets a
translator screenshot the day it gets a gallery row. The `dialog-gallery-coverage` check already fails when a registered
dialog has no row, which makes the whole chain self-maintaining.

Three limits the pass keeps, all about not lying to translators:

- **Main-window hosts only.** The gallery renders every row over the main window, including the three dialogs that
  really live in the settings or viewer window. Those rows are skipped rather than photographed on a backdrop they never
  have in production.
- **Novel states only.** A state is shot only when it resolves a key no earlier surface recorded. That's why the pass
  runs LAST: a hand-staged capture of the production path always beats a gallery preview of the same dialog, and the run
  doesn't write ~90 near-identical images.
- **Every drop is recorded** in `capture-skipped.json`, which is tracked, so a state that stops opening shows up in the
  diff instead of vanishing quietly. `gallery-redundant:` is the pass working as designed; `gallery-unavailable:` is a
  gap someone may want to close.

This needed one production change: the gallery's gate went from `import.meta.env.DEV` to
`import.meta.env.DEV || __CMDR_I18N_CAPTURE__` in `+layout.svelte` and `listener-setup.ts`. The capture binary's
frontend is a production Vite build, so `DEV` alone left the gallery out of exactly the build that needs it. A shipped
build sets neither flag, re-verified by grepping the bundle for the harness's marker literals
(`dialog-gallery/DETAILS.md` § What actually reaches a production bundle).

## Milestones

1. **Green the harness.** DONE. The `shortcuts` window was never broken: its skip blamed a tauri-playwright eval hang,
   but the real cause was the window missing from the E2E `playwright.json` capability, fixed in `a633c9a19` the day
   after the skip was written. The skip block is gone and `shortcuts` is an ordinary surface. The license and FDA passes
   got a generous first-listing wait (`awaitFirstListing`) instead of `ensureAppReady`'s 15 s: they're the Nth launch of
   a multi-launch run against a data dir with an index backlog, often on a machine that's also compiling.
2. **Registry-driven soft dialogs.** DONE: `i18n-capture-gallery.ts`.
3. **The operation queue.** DONE: `captureQueueWindow` captures `/queue` empty, with one Running + one Queued row, and
   with two failed rows (staged from the same same-lane copies `operation-queue.spec.ts` uses, plus two copies of a
   never-created source, which is the cheapest deterministic failure). All 18 `queue.*` keys were uncoupled and the
   window wasn't even in the skip list, so nothing flagged it. The namespace has since outgrown the window:
   `captureOperationChipSurfaces` covers the `queue.chip.*` / `queue.failureToast.*` keys, which render in the MAIN
   window's corner and only while an operation is in flight or a failure is retained.
4. **Ask Cmdr.** DONE: `i18n-capture-ask-cmdr.ts` walks consent → empty → one exchange → the threads panel. Replies come
   from the scripted fake LLM (`CMDR_E2E_ASK_CMDR_FAKE=1`, now set for the main capture launch), which both answers the
   message and opens the composer's provider gate.
5. **Acknowledgements, and the pane volume chooser.** DONE. Acknowledgements opens by its own `app.acknowledgements`
   command, waiting on the loaded package list rather than the dialog shell. The volume chooser is a pane-owned overlay,
   not a registered soft dialog, so neither the dialog tranche nor the gallery pass reached it, and it's the only place
   the sidebar's group headings and the favorites empty state render.
6. **Representative mappings for the families with no surface of their own.** DONE, and the reason coverage jumped
   further than the new captures alone explain: `queryUi.` and `search.` → the search dialog, `licensing.dialog.` → the
   license dialog, `fileOperations.delete.` / `.transferProgress.` → their own dialogs, `updates.` → Settings > Updates,
   `viewer.` → the viewer window. The table moved to `scripts/representative-screenshots.ts`; it's curated data that
   grows with the UI, while the coupler around it is machinery that doesn't. **Every note in it is a first draft and
   translator-facing: worth a read before a locale ships.**

Remaining, in rough value order: `settings.mediaIndex` (80 keys behind conditional UI in Settings > Indexing > Image
indexing, and the biggest single cluster left), `fileExplorer.navigation` (the SMB connection tooltips and favorites
errors the volume chooser doesn't show), `askCmdr`'s tool/undo lines (they need an agent that actually runs a rename),
and the `queue.*` row states a two-op queue never reaches.

## Gotchas already paid for

- **A window that "hangs" under tauri-playwright is usually an ACL problem.** Every eval's result rides back on the
  plugin's own `plugin:playwright|pw_result` IPC, which Tauri gates per window. A window missing from the generated
  `playwright.json` capability can receive a script but never answer, so the plugin waits out its 30 s ceiling and even
  `1+1` reads as a hang. Add the label when you add the window.
- **A stale "known-broken" comment costs more than the bug.** The shortcuts skip outlived its cause by weeks, and the
  capture report proved every run since had been capturing the window fine. When a skip has a TODO on it, re-check the
  claim before trusting it.
- **Never point a representative mapping at a screenshot the run doesn't produce.** When Settings > AI split into three
  surfaces, `settings-ai.png` stopped existing and all 101 `ai.*` couplings silently vanished while the catalog kept the
  dangling refs. Audit `REPRESENTATIVE_SCREENSHOTS` targets against the fresh report after any surface rename.
- **Toasts nothing staged can strand the run.** The virtual MTP device announces itself on its own schedule; the spec
  sweeps toasts once more at the end for exactly this reason (`e2e-playwright/DETAILS.md` § Toast lifecycle).
- **`i18n:shots` masks its exit code if you pipe it.** `capture && couple`: a failed capture skips the coupler, and
  `| tail` reports tail's success. Read `capture-failed.json` to know what really happened.
- **The capture build's profile override only costs you once.** `--config profile.release.debug-assertions=true`
  rehashes every dependency fingerprint, so the FIRST such build compiles the graph from `libc` up (~15 min). Cargo
  keeps that fingerprint set, so later capture builds recompile only what changed.
- **A dialog that swallows Escape poisons the surfaces after it.** The onboarding wizard doesn't close on Escape by
  design, and `rerender` re-resolves every MOUNTED string, so a wizard left up recorded its keys against the next
  dialog's surface and would have coupled onboarding copy to a screenshot of the operation log. The gallery pass clicks
  the wizard out explicitly and warns when any preview survives its close. If you add a dialog that ignores Escape, add
  its exit there too.
- **Scripts import siblings with a real `.ts` extension.** Node's type stripping won't resolve a `.js` specifier to a
  `.ts` file, so `from './representative-screenshots.js'` fails at runtime while typechecking clean
  (`apps/desktop/scripts/CLAUDE.md` § Must-knows).
- **A capture binary doesn't survive `pnpm check svelte`.** Naming a group runs its SLOW lanes too, and an E2E lane
  rebuilds the app binary without `CMDR_I18N_CAPTURE_BUILD`. The next `pnpm i18n:capture` then dies on every surface
  with `__cmdrI18nCapture not installed`, which reads like a harness bug and isn't. Run the checks first, or pass
  `--build` again afterwards.
