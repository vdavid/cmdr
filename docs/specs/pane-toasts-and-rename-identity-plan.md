# Pane-scoped transient toasts + path-keyed rename editor

Two UX-integrity fixes in one worktree (`toast-rename`), planned from a live-diagnosed incident and a code-intel sweep.
Executing agent: read this whole file first; the Evidence section is load-bearing (two earlier design premises were
WRONG and are corrected here — don't reintroduce them).

## Evidence (verified 2026-07-07, live MCP diagnosis + code sweep)

- The toast store is ONE module-level array with ONE `ToastContainer` per window (`(main)/+layout.svelte:317`).
  `dismissTransientToasts()` (`toast-store.svelte.ts:225`) removes every transient toast GLOBALLY.
- Its two prod call sites: `listing-loader.ts:219` (start of every `loadDirectory`) and `rename-flow.svelte.ts`
  `handleRenameInput` (every keystroke while renaming).
- **Corrected premise 1**: watcher events NEVER call `loadDirectory` — they arrive as incremental `directory-diff`s
  handled by `listing-diff-sync.svelte.ts`, which does not touch toasts. Every `loadDirectory` caller is
  navigation-class: user navigation, volume switch, mount/redirect/fallback, MTP-connect (`FilePane.svelte:1870`),
  tab-reachable retry (`FilePane.svelte:1846`), and SMB reconnect (`smb-view-state.svelte.ts:113`). The observed "toasts
  die within ~2 s in a busy session" incident is explained by background navigation-class events (for example SMB
  reconnect retries, which the incident session's logs were full of) firing `loadDirectory` in SOME pane and wiping ALL
  transients app-wide. A quiet-session diagnostic showed toasts living their exact full timeout (removed by the
  `ToastItem` timer at 4,002 ms), so there is no other killer.
- **Corrected premise 2**: the inline-rename editor mounts BY INDEX — `FullList.svelte:930` / `BriefList.svelte:921`
  render it where `renameState.target?.index === globalIndex`, and NOTHING reconciles `target.index` after activation
  (`listing-diff-sync` reconciles cursor + selection indices but not the rename target). A diff that inserts/removes a
  row above the renamed file shifts the row while the editor stays at the stale index → **the editor renders on the
  wrong row TODAY** (latent; same data-safety class as the paste-rename latch bug fixed in b0de3824f). The row `{#each}`
  is already keyed by `file.path` (`FullList.svelte:892`), so path is the natural identity.
- `toastGroup` is an EVICTION axis (`makeRoomForNewToast`), keyed by content type ('go-to-path', 'transfer-queue',
  'downloads'). Do NOT overload it as a pane tag.
- Rename already survives ordinary watcher diffs (only `listing-diff-sync.svelte.ts:126` cancels, when the diff removes
  the renamed file itself). `loadDirectory` cancels rename on navigation (correct, keep). Scrolling the row out of the
  virtual window unmounts the editor → its `onblur` cancels (existing, keep).
- Doc bug found in passing: `rename/CLAUDE.md:35` documents a "cumulative scroll > 200 px discards the rename" mechanism
  that does not exist in code (the real mechanism is unmount → blur → cancel). Fix the doc line.

## Design decisions (settled; don't relitigate)

1. Toasts gain an optional origin: `originPane?: 'left' | 'right'` on `ToastOptions` and `Toast`. Undefined = app-
   global. `toastGroup` stays untouched.
2. New `dismissTransientToastsForPane(pane)`: removes only transient toasts with `originPane === pane`.
   `dismissTransientToasts()` (global) stays for the debug panel, but the two prod call sites switch to the pane-scoped
   call.
3. `loadDirectory` dismisses only ITS pane's transients. App-global toasts (updater, transfer-complete, downloads,
   indexing, licensing…) survive any navigation. The other pane's toasts survive too. This kills the incident class: a
   background SMB retry in one pane can no longer eat the other pane's (or the app's) feedback.
4. `handleRenameInput`'s per-keystroke dismissal also becomes pane-scoped (it exists to clear that pane's stale
   validation toasts).
5. Tag by what the toast DESCRIBES, not which pane initiated it: tag only toasts about THIS pane's directory or a
   pane-local action (rename validation/errors, navigate/paste refusals, paste-as-file feedback). Toasts describing
   app-global state stay UNTAGGED — notably clipboard set/cut confirmations ("N items ready to move" describes the
   shared clipboard; the other pane consumes it), transfer toasts (span two panes), updater/indexing/downloads. To make
   forgetting impossible rather than documented: FilePane exposes a pane-bound add helper that injects its own
   `originPane` (pane-owned code uses it; plain `addToast` stays for global toasts). ⚠️ Deliberately UNTAGGED: the SMB
   reconnect toast in `smb-view-state.svelte.ts` — its own `loadDirectory` call would dismiss it instantly if
   pane-tagged. Leave it global; don't "helpfully" tag it.
6. The rename editor mounts BY PATH. Extract the mount predicate into a shared, tested helper (for example
   `shouldMountRenameEditor(target, row)`) used by BOTH FullList and BriefList (today they duplicate the inline index
   comparison), with the condition `target?.path === row.path`. Wrong-row rendering becomes impossible by construction
   (the `{#each}` is already path-keyed and Svelte 5 throws on duplicate keys, so path uniqueness is an enforced
   invariant). A diff that shifts rows (inserts/removes OTHER rows) now has the editor FOLLOW its file; a diff that
   changes the TARGET's own path (external rename/delete) is a removal → existing diff-cancel, NOT a follow.
   `target.index` is retired outright: verified, its only consumers are the two mount predicates (grep of all of
   `apps/desktop/src`), so delete the field from `RenameTarget` and its setter.
7. Cancel semantics stay EXACTLY as today: navigation cancels, removed-file diff cancels, scroll-out/blur cancels,
   Escape cancels (file kept). No caret preservation across remounts (out of scope). Surviving F5's `cacheGeneration`
   wipe is out of scope (it unmounts rows → blur-cancel; acceptable, user-initiated).
8. Small explicit calls: (a) `replaceExisting` (same-id re-add) keeps the FIRST toast's `originPane`, consistent with
   its existing partial-replace of other fields — accepted, don't "fix". (b) The removed-file cancel in
   `listing-diff-sync` may switch from name-compare to path-compare (`c.entry.path`) as an optional cosmetic cleanup —
   within one listing they're equivalent; don't frame it as a safety fix. (c) Two existing tests mock
   `dismissTransientToasts` and must switch to the pane-scoped call: `listing-loader.test.ts` and
   `navigation-transaction-handlers.test.ts`. The debug panel keeps the global function.

## Milestones

### M1: characterization + neutral extraction

- Toast store: pin today's global dismissal (all transients die regardless of any tag). This is CHARACTERIZATION, not a
  red (`originPane` doesn't exist yet); it gets UPDATED in M2 to the scoped contract.
- Rename: extract `shouldMountRenameEditor(target, row)` with today's index-based logic VERBATIM (behavior-neutral
  refactor; both views call it — a real dedup). No behavior change in M1.

### M2: pane-scoped transient toasts

- `originPane` on ToastOptions/Toast; `dismissTransientToastsForPane`; switch both prod call sites; the pane-bound add
  helper from decision 5; tag per decision 5; update the M1 toast pin to the new contract.
- Tests (genuine red-first where the contract is new): pane A dismissal spares pane B's transients, spares UNTAGGED
  transients (literally add one with no `originPane`), and spares a PERSISTENT toast tagged pane A (a naive filter that
  forgets the `dismissal === 'transient'` guard must fail this); global dismissal (debug) still clears all transients;
  eviction (`toastGroup`) unaffected by origin; `replaceExisting` keeps the first origin.
- Docs: `ui/DETAILS.md` § Toast system gains the origin/dismissal contract; one `ui/CLAUDE.md` guardrail line
  ("pane-owned transient toasts go through the pane-bound helper / carry `originPane`, or they survive that pane's
  navigation").

### M3: path-keyed rename editor (genuine red)

- Write the DESIRED tests against the still-index-based `shouldMountRenameEditor` and SEE THEM FAIL for the right
  reason: (a) `target={path:'/a'}` vs `row={index:5, path:'/b'}` at the target's old index → must NOT mount (red: index
  logic mounts on the wrong row); (b) `target={path:'/a'}` vs `row={index:7, path:'/a'}` → MUST mount (red: index logic
  refuses to follow). Then switch the predicate to `target?.path === row.path` → green.
- Retire `target.index`: delete the field from `RenameTarget` and its setter. The COMPILER is the primary enforcer — any
  surviving `.index` comparison in either view is a TS error, so both views are forced onto the path predicate.
- Keep all cancel sites. Optional cosmetic: path-compare in `listing-diff-sync`'s removed-file cancel (decision 8b).
- Fix the `rename/CLAUDE.md` scroll-200px doc line to describe the real unmount→blur→cancel behavior.
- Docs per the docs rule (rename DETAILS: the identity model, why index was a data-safety bug, and the win that a rename
  keystroke no longer wipes unrelated global toasts).

### Verification (lead runs live, after implementation)

- Two-pane toast isolation: toast in right pane, navigate left → survives; SMB-retry-style background loadDirectory can
  only clear its own pane.
- Rename follows its row: active rename in a dir, create/delete files ABOVE it from a shell so diffs shift rows → editor
  stays on the correct file; typing + Enter renames the right file. Repeat in a 100k+ entry directory.
- Edge: start a rename near the BOTTOM of the viewport, insert rows above so the followed row scrolls out of the virtual
  window → unmount → blur-cancel. Confirm that outcome is acceptable (expected: rename cancels, file keeps its name —
  same as scrolling away today).
- Archive (zip) rename + MTP rename sanity (their diffs come only from in-app mutations; nothing should change).
- Full `pnpm check` in the worktree.

## Non-goals

- Watcher-reread toast suppression (no such dismissal exists — corrected premise 1).
- Caret/selection preservation across editor remounts; F5 `cacheGeneration` survival; per-pane toast overlays.
- Any change to eviction (`toastGroup`/`maxInGroup`) or persistent-toast behavior.

## Invariants

- The rename editor NEVER renders on a row whose `file.path !== target.path` (by construction).
- A transient toast without `originPane` is never dismissed by any pane's navigation or rename typing.
- No behavior change for persistent toasts, eviction order, or cancel semantics.
