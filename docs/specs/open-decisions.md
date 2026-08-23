# Decisions waiting on David

**Problem**: a question with no answer looks exactly like a task nobody picked up, so it sits inside a 600-line spec and
keeps that spec alive for years. These are the calls that gate work but are not themselves work. Most take a minute.

Decisions that gate exactly one effort live in that effort's spec instead: the reconcile-governor shape is in
`idle-cost.md`, and the module-cycle ratchet metric and FTP's concurrency knob are in `backend-as-a-crate.md`.

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

7. **Does the `write_operations` module tangle get scoped into the backend-crate effort, or stand alone?** It is now the
   app crate's largest genuine cycle at 11 sibling modules with no parent node, and nothing owns it. Scoping it needs an
   edge analysis first, about half a day.

## Three questions I recommend closing as already answered

Each was left open in a spec and has since been settled by shipped code or by measurement. They need ratification, not
design.

8. **How aggressive should the SMB session deadline be?** **Recommend: leave `smb2`'s defaults.** The ECHO keepalive
   means the base deadline no longer has to be sized for the slowest healthy case, and Cmdr's per-file retry absorbs a
   breach as a blip. Nothing measured says the current numbers hurt.
9. **Should an SMB reconnect be silent?** **Recommend: yes, silent, with the evidence available on demand.** It already
   is silent, `smb_diagnostics.rs` already exposes the counters for anyone investigating, and a toast per reconnect
   would fire on every laptop lid-close.
10. **Credit budget, or a sanity ceiling on the concurrency window?** **Recommend: close as answered.** The question has
    no well-defined answer as posed: credits gate write frames connection-wide while the window gates concurrent FILES,
    so there is no file-level value a credit budget could produce. The 32 ceiling stays because the NAS plateaus at 12
    on both corpus shapes.

## Maintenance calls

11. **A deliberate `invariant-density` ratchet pass.** The check warns repo-wide on four subsystems and has been
    drifting up unnoticed because it is warn-only. `crates/cmdr-index` sits at 371 rules and 2.96 per kloc against the
    frontend's 0.87, and it is also the codebase's top bug source. `AGENTS.md` says the fix is to make each invariant
    unrepresentable in a type rather than to raise the number. Worth scheduling rather than letting it drift.

12. **A per-test duration budget for the Rust suites**, mirroring the two seconds E2E already enforces. **Recommend:
    build it, but budget the MARGIN, not the duration.** The blocker is gone: one clean full run on 2026-08-23 is 6,599
    tests in 26 s wall clock, with **12 over two seconds**, three over three, and one over five. (`~/cmdr-test-log.csv`
    logs only about 30 passes per run because `testLogSlowSeconds` is 1.0 s, which is by design, not the parser bug this
    entry used to suspect.) What a flat two-second rule would buy is the wrong thing, though: of the four causes behind
    every Rust flake measured this month, three have no duration signal at all (0.25 s, 1.6 s, and a config fact with no
    runtime), while the fourth ranks perfectly by **cap ÷ idle runtime** — every test two saturated full-suite runs
    killed sits at the thin end of that ratio, topped by `busy_db_is_retried_not_deleted` at **1.4×**, which no duration
    rule catches without also catching a dozen honest two-second tests. Seeding a margin ratchet still needs David's OK
    (it's a new allowlist). Evidence: `docs/notes/rust-test-flake-analysis-2026-08-23.md`.

    **David's answer (2026-08-23): don't build the margin ratchet, and the standard he wants is different from both
    options above.** He wants a test flagged when it takes longer than two seconds **on a SATURATED machine**, not an
    idle one, on the grounds that almost every test here can be refactored to finish in a very short time. That inverts
    the framing: the idle-machine measurement this entry argues from is the wrong baseline, and a test needing more than
    two seconds under load is a test to fix rather than a threshold to tune around. Nothing is being built for it right
    now; capture the standard so a future duration check is measured under saturation and not against the 26 s idle run.
