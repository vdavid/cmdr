# Decisions waiting on David

**Problem**: a question with no answer looks exactly like a task nobody picked up, so it sits inside a 600-line spec and
keeps that spec alive for years. These are the calls that gate work but are not themselves work. Most take a minute.

Decisions that gate exactly one effort live in that effort's spec instead. A call that gates nothing but is still worth
making sits with the work it came from: `later/idle-cost-follow-ups.md` holds the CLIP idle-unload and compute-unit
calls, and the question of whether the rescan walk may read `SYSTEM_DIR_EXCLUDES`.

## Human-facing copy shipped as agent drafts

Principle 4 says anything meeting human eyes is made or closely reviewed by a human. These went out as drafts. All are
translated into nine locales, so a change means re-translating those keys, and none is urgent.

1. **Five operation-queue strings.** ⚠️ Three evolved past their drafts during the build, so review the shipped text:
   `queue.chip.tooltip` (renders as `Copying · 214 items · to Backup · 42% · 1m 20s left`), `queue.failureToast.title`
   (`Couldn't finish copying`, selecting on operation type), `queue.chip.failed`
   (`{countText} operations couldn't finish. Open the operation queue to see why.`), plus
   `fileOperations.transferProgress.queuedToastCount` and the three dismiss labels. Also worth a look, added after the
   drafts: `queue.chip.ariaLabel` and `queue.chip.scanningAriaLabel`.
2. **Four rename-chaining toasts**, in `en/fileExplorer.json`: `chainKeptOriginalName`,
   `chainKeptOriginalNameAndOthers`, `unconfirmed`
   (`Couldn't confirm the rename of "{name}". The volume may be slow, so the rename may still have gone through.`), and
   `unconfirmedAndOthers`.
3. **Three Duplicate strings**: `commands.fileDuplicate.label`, its palette description
   (`Make a copy of the selected files in the same folder`), and `menu.file.duplicate`. Note the nine locale labels were
   chosen as each language's own Finder word rather than a dictionary translation, for example Hungarian "Megkettőzés";
   the choice of convention is worth knowing even under the translations-reviewed-later caveat.
4. **`Hide Others` and `Show All`** kept muda's Title Case while every neighbour is sentence case. Matching AppKit's own
   convention is defensible, but it is not written down anywhere, so today it reads as an oversight. Fix or document.

## Product calls that gate real work

5. **Should a file that exhausts its retries stop the operation, or should the batch carry on?** Today one unrecoverable
   file ends a 700-file copy. Carrying on needs a terminal event shape that can say "finished, N files missing", a
   frontend that shows which ones, and journal semantics for a partially-successful operation. Several days, and it is a
   product call first. Recorded at `transfer/DETAILS.md` § "Not done here".

6. **Per-rule approval for a long job's tail.** For a 500-file rename, should approval move from per-item to per-rule,
   where the user carefully reviews a trial batch of five to 10 and the remainder runs as one background operation with
   progress, cancel, and undo? Options: **(a)** yes, accepting that this is a write-engine change and that the plan-call
   compaction idea then evaporates; **(b)** no, keep per-item approval and build compaction instead; **(c)** defer both.
   This has blocked its two dependent milestones since July.

## One maintenance call

7. **A deliberate `invariant-density` ratchet pass.** The check warns repo-wide on four subsystems and has been drifting
   up unnoticed because it is warn-only. `crates/cmdr-index` sits at 371 rules and 2.96 per kloc against the frontend's
   0.87, and it is also the codebase's top bug source. `AGENTS.md` says the fix is to make each invariant
   unrepresentable in a type rather than to raise the number. Worth scheduling rather than letting it drift.
