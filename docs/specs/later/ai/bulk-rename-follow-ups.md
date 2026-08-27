# What the reviewed bulk rename deliberately left

Ask Cmdr's natural-language bulk rename shipped hardened: an exclusive local rename primitive that can't clobber a
destination created since review, a dependency planner that spends one temporary name per cycle and none anywhere else,
live overwrite- and missing-source detection driven off the `directory-diff` stream, extension and cycle warnings in the
review dialog, a prompt contract that forces truncation disclosure, and honest provenance in the operation log. Every
design decision lives beside the code: `apps/desktop/src-tauri/src/agent/tools/propose/DETAILS.md` (evidence, preflight
blockers and warnings, the deliberately non-durable acceptance),
`apps/desktop/src-tauri/src/file_system/write_operations/DETAILS.md` (the exclusive primitive and the planner), and
`apps/desktop/src-tauri/src/operation_log/DETAILS.md` (provenance).

Two things were named and consciously not done. Neither blocks anything shipped, and neither restates a mechanism: each
points at the doc that owns it.

## 1. A remote hallucinated source refuses the whole plan

**The gap**: a model can invent a filename. On a local volume that row stays reviewable and preflight blocks it as
`SourceMissing`, so the user sees a `(doesn't exist)` badge and can tell exactly what the agent got wrong. On a remote
volume the whole plan is refused at the proposal boundary instead, one invented name taking 40 good rows with it.
`missing_local_child` in `apps/desktop/src-tauri/src/agent/tools/propose/rename/plan.rs` is where the split lives, and
its doc comment carries the current rule.

**Why it's shaped that way**: proposal construction is synchronous and validates against the pane cache and index
registration only, so for a remote path it can't tell an absent direct child from a path escape. That boundary's whole
rule is that it never touches a live mount (`apps/desktop/src-tauri/src/agent/tools/propose/CLAUDE.md`), because a dead
mount must not hang an agent turn.

**The call**: accept the asymmetry, or make proposal construction authoritative and async for remote paths. The second
buys a live probe against exactly the mount the boundary refuses to touch, so it wants a real report behind it rather
than a hypothetical.

**Trigger**: a user renaming over SMB and losing a whole plan to one invented name.

## 2. Nothing checks a reply's coverage claim against what the tool returned

**The gap**: when `list_pane_files` truncates, the system prompt requires the reply to say it inspected `returned` of
`total` items using those exact numbers and never to imply full coverage, and a focused test in
`apps/desktop/src-tauri/src/agent/chat/system_prompt.rs` pins all three requirements. A model that ignores the rule
isn't caught: nothing compares what the reply claims against what the tool actually delivered.

**Cost**: real, and the obvious version is the wrong one. Deciding whether a sentence claims full coverage means reading
the model's prose, which breaks on a paraphrase or a locale and would be classification by string-matching in all but
name. The narrow version is affordable: refuse a rename plan whose row count equals `total` when the listing it was
built from came back `truncated`.

**Trigger**: evidence a model actually lies about coverage. The prompt rule carries no counted signal today, so this
starts with one.
