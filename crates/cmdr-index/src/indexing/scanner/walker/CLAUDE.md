# Local guarded walker

The hang-tolerant engine every local walk runs on, plus `bulk_read` (`getattrlistbulk`, macOS). It exists because a
disconnected File Provider mount blocks `readdir` forever: a condemned read is ABANDONED (subtree pruned, dir left
unmarked, a replacement worker spawned), so a hung dir costs one worker for one timeout, never the scan. The scan driver
and the exclusion policy are `../CLAUDE.md`.

## Must-knows

- **Never rayon.** Workers are dedicated 8 MB-stack OS threads: File Provider reads descend XPC chains that overflow
  rayon's 2 MB stack.
- **The guard measures PROGRESS, not elapsed time.** Every read publishes what it has delivered through `ReadProgress`,
  and only a read that STOPPED PRODUCING is abandoned. ❌ Never re-cap total duration: elapsed time can't tell a
  200,000-entry dir from a dead mount, and that cap once dropped 661,411 rows.
- **A reader that can't report progress is still bounded** by the fallback timeout — that path is the exception, ❌ not
  the model to copy.
- **Subtree give-up after `DEFAULT_GIVE_UP_AFTER` (32) consecutive failed reads**, sticky per dir and reset by a
  successful sibling. Throttle, not exclude: ❌ no path denylist.
- **`bulk_read` degrades, it doesn't drop.** Every entry is validated against `ATTR_CMN_RETURNED_ATTRS`; a miss yields
  `stat: None` and the caller falls back to `symlink_metadata`. ❌ Never report a size the parser didn't read, and ❌
  never widen the parse without widening that validation.
- **Each batch publishes through `ReadProgress` as it arrives** — that IS how the watchdog knows this read is working.
  ❌ Don't buffer a whole directory before reporting.

The engine's design, the progress-timeout rules, the give-up budget, and the bulk-reader parse: `DETAILS.md`. Read it
before any non-trivial work here: editing, planning, reorganizing, or advising.
