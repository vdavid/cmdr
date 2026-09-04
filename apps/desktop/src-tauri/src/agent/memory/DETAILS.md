# Agent memory — details

What the agent writes about the user, where it lives, and what stops it from being an instruction channel.
Must-knows: `CLAUDE.md`.

## Where it lives, and why not `~/.cmdr/`

`<data-dir>/ai/memory/`, resolved through `config::resolved_app_data_dir`. Three reasons, and the third is the one that
bites if it is ignored:

- It is app-managed state, not user config. The user may open the folder and read it, but they don't author it.
- `app_data_dir()` is already the canonical per-OS path on all three platforms, so nothing here does per-platform work.
- It inherits `CMDR_DATA_DIR` isolation for free. Sharing a home dotfile would mean an E2E run writing personal facts
  into the developer's real agent memory, and every worktree's dev instance sharing one file.

`~/.cmdr/CMDR.md` stays a dotfile in home for exactly the opposite reasons: hand-edited, dotfiles-repo-able, the user's
own voice. See `../chat/DETAILS.md` § Which `CMDR.md`, and how much of it.

`AGENTS.md` is the hub. `outcomes.md` sits beside it and is the one file here the MODEL does not author (below). Both
are auto-fed, which is what keeps the "**no read or list tool**" decision true with two files: nothing here has to be
discovered to be read, and every tool schema would ride in the cached prefix of every turn, the rail's included. Add
both the moment a file arrives that isn't auto-fed.

## The decision ring (`outcomes.rs`)

Every other write here happens because a model chose to make it. `outcomes.md` is written MECHANICALLY, once per
decided proposal, by `../outcomes.rs`, with no model call in the loop. That one difference sets its whole shape.

**Decision: a fixed-size ring, rewritten whole, with a RESERVED slice of the folder cap.**
**Why**: a `DirectoryFull` refusal on this path has nobody to relay it to. The tool refusals work because a model reads
them and prunes; a mechanical writer has no turn to answer in, so the write must be one that cannot be refused. Two
halves make that true:

- The ring is bounded at `OUTCOMES_MAX_BYTES` (4 KB) and `OUTCOMES_MAX_ENTRIES` (40), oldest evicted first, and a write
  replaces the whole file. `used − replaced + new` therefore nets to roughly zero however many decisions land.
- `MemoryStore::write` and `edit` price against `MEMORY_MODEL_MAX_BYTES`, the folder cap MINUS that reserve. Without
  that, a model that filled its own notes would silence the channel that teaches it what the user wants: silently, and
  worst for the person who uses the agent most.

**Decision: its own file rather than a section of the hub.**
**Why**: the prompt slice takes the hub's HEAD, so a section appended to `AGENTS.md` would either be cut off first or
push the model's own notes out of the window, depending which end it sat at. Two files let each have its own share:
`read_for_prompt` gives the ring a bounded quarter of the slice and the hub the rest.

**Decision: the ring is cut from the OLD end for the prompt; the hub is cut from the tail.**
**Why**: opposite reasons, and getting it backwards is silent. The hub's head is the model's own summary of the person
and its tail is detail. The ring's tail is the freshest lesson and its head is the one already superseded.

Non-entry lines (a heading, a note the model added with `memory_edit`) are dropped on the next rewrite: the file is the
ring's, and keeping stray prose would grow it without bound. Everything else here applies unchanged: the jail, the
durable write, and "Forget everything", which takes the ring with the rest because it is about the user too.

## The jail (`jail.rs`)

One function, `resolve(root, requested)`, called by both tools before either touches the disk. Four checks in an order
that matters:

1. **Lexical shape.** Trim, reject empty, then walk `Path::components()`: `Normal` accumulates, `CurDir` is skipped, and
   `ParentDir` / `RootDir` / `Prefix` refuse outright. This catches an absolute path, and one climbing out with `..`,
   before any syscall runs. A path that names nothing once the `.` segments come out is `NoPath`, not an empty join.
2. **Extension.** `.md` only, case-insensitive. A folder that can hold a `.sh` or a `.json` is a folder the agent can
   drop something executable or config-shaped into.
3. **The symlink walk.** Every component from the root down is `symlink_metadata`'d, the file included. A planted
   `link.md` is a lexically perfect relative path, so nothing above this catches it.
4. **`canonicalize` of the PARENT, then a containment re-check.** ⚠️ The parent, never the file: `canonicalize` fails on
   a path that doesn't exist yet, so canonicalizing the target would refuse every first write. The parent is created
   (only after step 3 clears it), canonicalized, and checked to sit under the canonicalized root; the file name is a
   single validated `Normal` component joined onto the result.

Every escape attempt has a test in `tests.rs`, and each asserts both the typed refusal AND that nothing landed outside.
Asserting only the `Err` would pass for a refusal that wrote first.

## The two caps

**`MEMORY_DIR_MAX_BYTES` = 64 KB, across every `.md` under the root.** A disk guard. `used_bytes()` walks the folder and
counts `.md` files only, so a stale temp or something the user dropped in can't jam the agent out of its own memory. A
write is priced as `used − replaced + new`, so rewriting the hub reclaims what the old copy held; without that, memory
would jam at half the cap and never recover.

The TOOLS price against `MEMORY_MODEL_MAX_BYTES`, which is that cap minus the decision ring's reserve (below). Over it,
`MemoryRefusal::DirectoryFull { used, cap, wanted }` comes back and the tool turns it into a sentence telling the model
to prune with `memory_edit`, quoting the refusal's own `cap`. ⚠️ Never the folder constant, or the model prunes toward
a ceiling it cannot reach. ⚠️ Never a silent failure either: a model that believes it saved something it didn't will
keep answering as if it had.

**The prompt slice** is `chat::budget::memory_slice_bytes(prompt_budget)`, a tenth of what the budget has left after the
fixed overhead. It is a SHARE rather than a constant for two reasons that a byte cap can't cover:

- The system string is never elided (`context::assemble_prompt` tightens tool results only), so every byte of memory is
  a permanent tax on every turn of every thread. At `MIN_LOCAL_CONTEXT_TOKENS` the resolved budget is 19,660 tokens and
  the prefix takes 6,263 of it; a flat 8 KB of memory would take 2,048 more, on top of a paged tool result's 8,000,
  leaving little for the digest, the envelope, the history, and everything else the turn carries.
- **The agent writes this file itself.** A flat cap lets it permanently degrade its own chat, and nothing in the loop
  would tell anybody why the replies got worse.

`read_for_prompt` cuts at the nearest character boundary below the limit and appends a note saying it was cut and to
prune with `memory_edit`. A silent cut leaves the model reading a sentence that stops mid-thought and treating it as the
whole of what it once knew.

## The injection surface

The write path is reachable from text the agent read. `image_facts` returns the full stored OCR of the user's images
(the widest derived-content egress the agent has), and file names come off disk. So a crafted filename or a picture of a
sentence can get the agent to write a line into `AGENTS.md`, and that line then rides the cached prefix of every later
turn, including ones that call `propose_suggestions`. It survives restarts and thread deletion.

Three defences, and all three are load-bearing:

1. **The jail** keeps the writing inside one folder of Markdown.
2. **Placement and fencing** (`../chat/context.rs`): memory goes BEFORE the rules, not after them, inside a fence whose
   closing marker the fenced content cannot produce, under a line saying the block is data that never overrides what
   follows. Appending it the way `CMDR.md` is appended would put agent-written text in the strongest override position
   the prompt has.
3. **The write instruction** (`../chat/system_prompt.rs`): notes record facts about the user and their preferences,
   never instructions to itself, and a note that reads as an order is to be reported and removed.

⚠️ The tools are callable **from the rail, not only from a wake**. That is intended ("remember this for me") and it is
also what makes 1–3 necessary rather than theoretical.

## The two controls the user gets

Both live in Settings › Ask Cmdr, under "What Ask Cmdr remembers", and both exist for the same reason: the folder is
the agent's, the notes in it are about the user.

- **Open memory folder.** `ask_cmdr_memory_folder` (`../../commands/agent/memory.rs`) resolves the root, creating it so
  the click lands somewhere real before the first note exists, and the settings window emits `reveal-path` at the main
  window, which points a pane at it. ⚠️ The path can only come from Rust: it moves with `CMDR_DATA_DIR`, so a frontend
  that built it would walk a dev build into the production folder. `reveal-path` exists because `execute-command`, the
  only other settings-to-main dispatch, carries a bare `command_id` and no payload.
- **Forget everything.** `MemoryStore::forget_all` deletes every `.md` under the root, subfolders included, and reports
  the count behind the `forget-memory` confirmation. It leaves the folders and any non-`.md` file alone, for the same
  reason `used_bytes` skips them: anything the agent could never have written is the user's. Keeping the root means the
  next write lands without the jail recreating it.

## What diagnostic bundles do NOT pick up

Verified rather than assumed, and recorded so nobody re-audits it every time something new lands in the data dir
(against `main`, 2026-08-23): crash and error report bundles never reach this folder. `diagnostics_snapshot.rs` does one
non-recursive `read_dir` of the data dir and keeps only `index-*.db` names, reading sizes; the log half takes
`cmdr.log*` out of the LOG dir. The full walk, and the one residual hole (`cmdr.log` itself does ship, so memory content
must never reach a log line), is in `docs/security.md` § What a bundle never picks up.

## Testing shape

⚠️ **This module's split exists because of a testing constraint, not an aesthetic one.** There is no Tauri mock runtime
in the tree (`chat/runtime/tests.rs` says so outright) and every registry handler takes an `AppHandle`. So everything
that decides anything is in `MemoryStore`, parameterized on a root `Path` and unit-tested against a `tempdir`, and the
`AppHandle` half is `store_for` — eight lines of path resolution with no rules in them. Putting a rule in the resolver
puts it somewhere no test can reach.
