# Disk cleanup advice — process notes for agents

Lessons from a real session where a general-purpose agent gave wrong recommendations. Keep this short and concrete;
expand as more pitfalls show up.

## The user's actual heuristic (use this)

Delete only directories where **both** are true:

1. **Filesystem-idle**: newest file mtime inside the dir is older than ~14 days.
2. **Process-idle**: no running app/agent currently owns the dir or its parent.

Anything that fails either signal stays — even if it's huge, even if `git` reports it merged, even if the dir lock flag
is unset.

## Pitfalls that cost real trust

### Worktrees: merge status lies, mtimes don't

- `git worktree list --locked` shows git's lock flag, not "in use." An unlocked worktree can be the focus of a live
  Claude Code session.
- `git log -1` shows last commit time. Agents read, build, and test for hours without committing. A 54-minute-old commit
  on a merged branch is almost certainly still active.
- The reliable signal is **newest in-tree file mtime**, not git state. If `find <worktree> -mmin -120 | head` returns
  anything, the worktree is live.

### App caches: check if the app is running before recommending

Before suggesting any of these get wiped, run `pgrep -fl '<app>'`:

| Cache path                                             | Owner to check                                   |
| ------------------------------------------------------ | ------------------------------------------------ |
| `~/Library/Caches/JetBrains`                           | `idea`, `webstorm`, `clion`, `pycharm`, `goland` |
| `~/Library/Application Support/Cursor` + caches        | `cursor`, `Cursor`                               |
| `~/Library/Caches/com.spotify.client`                  | `spotify`                                        |
| Chrome caches (`CacheStorage`, `Service Worker`, etc.) | `chrome`, `Google Chrome`                        |
| Firefox caches                                         | `firefox`                                        |

If the app is running: don't recommend the wipe. Either skip or downgrade to "quit the app first if you want to reclaim
this."

The single 2 GB IntelliJ heap-dump file (`~/java_error_in_idea.hprof`) is the exception — it's a one-off crash artifact,
not live state, safe to delete even while IntelliJ runs.

### Cost/benefit: don't recommend small wins with high refetch cost

| Bad recommendation            | Why                                                                                 |
| ----------------------------- | ----------------------------------------------------------------------------------- |
| Chrome `CacheStorage` (~2 GB) | Browser re-downloads pages the user is likely to revisit soon. Net cost > net gain. |
| `~/.cargo` registry (~2 GB)   | Reclaimed on next `cargo build` of the affected crate. Disruptive for active devs.  |
| `~/Library/Caches/pnpm`       | Same shape — pnpm refetches on next install.                                        |

Only recommend cache wipes where: size is big AND refetch is unlikely soon (dormant tooling, old AI model downloads,
abandoned package managers).

### Present candidates, not verdicts

Default the output to a three-column table: candidate · signals · reclaim. Don't put anything in a "safe to delete"
bucket unless **all** the following are true:

- Filesystem-idle (mtime ≥ 14 days).
- Process-idle (`pgrep`/`lsof` clean).
- Cheap to regenerate (or known-unused by the user).
- Not in a worktree/repo with current uncommitted state.

Otherwise list as "candidate, ask user."

## The tooling that actually helps

### Default path: Cmdr MCP `state` resource + `nav_to_path` + `sort`

This is the right starting point for almost every cleanup task. The pattern:

```
nav_to_path(pane, <dir>)
sort(pane, by="size", order="desc")
set_view_mode(pane, "full")              # full view exposes dir sizes and mtimes
ReadMcpResourceTool(cmdr-dev, "cmdr://state?include=panes")
```

Returns up to 76 entries with **size + mtime** in roughly **2 s per directory level**. Drilling four levels deep takes
~8 s total. Compare to `du -sh <dir>/*` at the same depth: **30–70 s per level, no mtimes**. Roughly 25–30× faster, with
the activity signal included.

Workflow:

1. Start at `/Users/<user>` sorted by size desc; the top 5 entries cover ~95% of the actionable space.
2. Drill into each one; same routine. Two or three drills is usually enough.
3. Make sure `showHidden: true` — dot-directories like `~/.ollama` (19 GB), `~/.npm` (20 GB), `~/.rustup` (8 GB),
   `~/.cache` (4 GB) are easy to miss and frequently the biggest wins. They don't show in `ls ~/` and a casual sweep
   skips them entirely.
4. mtimes come for free; combine with `pgrep` for the activity signal.

### Secondary: search MCP with explicit patterns

`mcp__cmdr-dev__search` with `type=dir`, a pattern (`target|node_modules|caches`), and `min_size` returns the answer in
<1 s for the "specific kinds of bloat" case. AI search (`mcp__cmdr-dev__ai_search`) hits the same backend; works well
when the query names what to look for, returns empty/sparse when it doesn't.

Known limit: `min_size` with no pattern (or with `*`) on dir search returns very few results. Don't rely on it for the
"show me every dir over N GB anywhere" query. Use the state walk for that.

### When to fall back to `du`

- Need _aggregated_ dir size where Cmdr's listing shows blanks (rare; happens for newly-mounted or unindexed volumes).
- Need a recursive "sum every `target/` under this tree" — Cmdr listings are one level at a time.

## Recommendation framing

### Project-version-aware cleanup for installed runtimes

For multi-version tooling (`rustup`, `nvm`, `pyenv`, `mise`, etc.), don't recommend version removal until you've checked
what the user's actual projects require. The pattern:

1. List installed versions (`rustup toolchain list`, `nvm ls`, etc.).
2. Grep the user's active project repos for pinned/required versions:
   - Rust: `rust-toolchain.toml` (`channel = `) and Cargo.toml `rust-version`.
   - Node: `.nvmrc`, `package.json` `engines.node`.
   - Python: `.python-version`, `pyproject.toml`.
3. Recommend removing only versions that:
   - Predate every project's MSRV / minimum supported version, and
   - Aren't the default/stable/nightly channels.
4. Keep one MSRV-matching version per project if the user runs MSRV-CI locally; drop it if they don't (and say so
   explicitly so they can override).

### Explain obscure dirs before recommending deletion

Items like `~/Library/Application Support/Claude/vm_bundles`, `~/.codeium`, `~/.u2net`, OrbStack data, UTM VM images
mean nothing to the user as path strings. Before recommending deletion, give a one-line "what is this and what's the
regenerate cost" — then frame the decision by feature use, not safety:

> "Claude desktop's sandboxed VM bundle (12 GB). Used by Computer-Use/agent features in the desktop app. Re-downloads on
> first use (~3 min). **Use those features → keep. Chat-only or terminal Claude Code → safe to delete.**"

The user evaluates "do I need this feature" instantly. "Is this safe to delete?" is the wrong question to push on them.

### APFS deletes are instant — don't sequence them defensively

`rm -rf ~/.ollama/models` (19 GB) took 0.17 s. `rustup toolchain uninstall` of four toolchains took 16 s mostly because
rustup is doing per-component bookkeeping, not because the disk is slow. Don't write multi-step "delete a little, check,
delete more" plans out of caution — APFS makes single big deletes cheap and trivially reversible (the user's Trash
workflow plus Time Machine cover the rare oops).

## Quick checklist before generating any recommendation

- [ ] Walked the top of the tree with state + size sort, not guessed from training data.
- [ ] Checked mtime ≥ 14 days for every "stale" claim.
- [ ] Ran `pgrep`/`ps` for every app whose cache I'm about to suggest wiping.
- [ ] Computed reclaim ÷ refetch-cost; dropped low-ratio items.
- [ ] Presented "candidates with signals," not "safe to delete," unless every signal lines up.

## Outcome of the session that prompted this note

**First round (agent's bad recommendations):** suggested a 54-min-old "merged" worktree the user was working in,
JetBrains caches while IntelliJ was running, and Chrome `CacheStorage` for a small win with high refetch cost. User
ignored all three, instead deleted Rust `target/` dirs in week-idle projects plus two month-old unused worktrees, and
reclaimed ~120 GB on their own.

**Second round (state-walk approach):** walked `/`, `/Applications`, `/Users/<user>` via `cmdr://state` sorted by size
desc with `showHidden: true`. Two drills surfaced `~/.ollama/models` (19 GB, idle 14 months) and `~/.npm` (20 GB,
package cache). Combined with `~/java_error_in_idea.hprof` (2.5 GB crash dump) and a rustup prune (4 toolchains
predating every project's MSRV, ~3 GB), reclaimed ~45 GB more without a single bad recommendation.

Free space went from 37 GB → 185 GB across the session. The shift was a tooling shift (state walk over `du` and search),
not just a heuristic shift.
