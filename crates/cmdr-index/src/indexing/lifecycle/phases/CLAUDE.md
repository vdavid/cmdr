# Covering a volume in phases

The volume's first index, in the order its owner cares about: the host's folders, then `$HOME`, then the drive. Every
walk is add-only and resumable, so a quit keeps what it bought. There is no first full scan.

`mod.rs` the driver (its own `Utility` thread) + `PhaseHandle`; `stitch.rs`; `queue.rs` the ranked order; `visits.rs`
where the user is looking; `completion.rs` the two stamps and what completing owes. The start itself is
`../manager/phased.rs`.

## Must-knows

- **The stitch is what makes phases compose.** ❌ Never skip it: an ancestor scope's frontier never shrinks on its own,
  so the next phase re-walks everything and each root takes the SERIAL repair. It marks ONE directory at a time, files
  included, flushes between the upserts and the mark, stamps the CURRENT epoch, and ❌ never marks a directory it
  couldn't read.
- **A phased start prepares its database through writer MESSAGES**, ❌ never a second write connection. No
  `EXCLUSION_POLICY_KEY` stamp ⇒ every coverage query answers "walk everything" and nothing EVER converges, silently.
  The stamp's condition is `entry_count <= 1 || we-just-truncated`; both misreadings are silent.
- **Completion is DERIVED**: "the frontier under this root is empty". ❌ Never remembered, and ❌ never a "didn't shrink
  twice" rule. Abandoned ground leaves the frontier, so one wedged directory can't hold it open.
- **The completion ORDER is enforced by a flush**, stamp before collapse. Collapse the branch set first and one shallow
  anchor in that window truncates the index that just finished.
- **`working` (a phase queued or running) is what scan entries refuse against; `walking` (reading the disk now) is only
  the verifier's.** ❌ Never `mgr.scanning`: `cover_context_for` returns `None` under it, so our own walks would fail.
- **One `cover()` per frontier root, joined**, with the drain batched to once per phase. ❌ Don't hand one call a whole
  frontier: the cancel check inside `cover` is not a queue check point.
- **`home_covered_at` drives exactly ONE subscriber** (the media + importance early kick) and ❌ nothing else — not
  freshness, not the badge, not rescan routing. `~/Library` goes last in its phase and the signal doesn't wait for it.
- **Ask the host through the seams it already has**: `priority_roots` (an ORDER, never a scope) and `open_listings` on
  the reporter's tick. ❌ Not `verify_directory`, which fires for the opposite pane and every refresh.
- **A rescan before full coverage RESTARTS the phases**, ❌ never truncates and ❌ never errors. Every door goes through
  `cover_or_scan`; the deferred-rescan mechanism answers for COMPLETED volumes only, so the two never overlap.
- **The escape hatch is `defaults write com.veszelovszki.cmdr PhasedFirstIndex -bool false`** + relaunch, arriving as
  `IndexConfig::phased_first_index`. Off, a phased partial takes today's TRUNCATING rebuild; that's the intended row.

Depth on every bullet, plus the measurements behind the flush batching and the `~/Library` decision: `DETAILS.md`. Read
it before any non-trivial work here: editing, planning, reorganizing, or advising.
