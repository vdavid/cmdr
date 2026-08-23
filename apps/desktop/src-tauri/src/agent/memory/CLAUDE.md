# Agent memory (`agent/memory/`)

The Markdown folder the agent writes about the user, at `<data-dir>/ai/memory/`, with `AGENTS.md` as the hub. Without
it the agent relearns nothing and re-proposes what was already turned down. Depth: `DETAILS.md`.

## Module map

- `store.rs`: `MemoryStore` — the caps, the write, the edit, and `read_for_prompt`, the slice a turn carries.
- `jail.rs`: the one path check both tools call.
- `refusal.rs`: `MemoryRefusal`. Its own module so the jail and the store can both name it without a module cycle.
- `mod.rs`: `memory_root` / `store_for` / `read_for_turn`, the only lines that need an `AppHandle`.

The tool handlers live with the rest of the toolset (`../tools/memory.rs`); the prompt fence lives with prompt assembly
(`../chat/context.rs`).

## Must-knows

- **The store is PURE, parameterized on a root path.** There is no Tauri mock runtime in the tree and every registry
  handler takes an `AppHandle`, so a store that needed one would have no testable rules at all. ❌ Never move a rule
  into the resolver.
- **This folder is attacker-reachable.** `image_facts` hands the agent the full stored OCR of the user's pictures and
  file names come off disk, so a crafted filename or a photographed sentence is a route to writing here — and what is
  written rides the prefix of every later turn, surviving restarts and thread deletion. The three defences are the jail
  (here), the fence and its placement before the rules (`../chat/DETAILS.md` § The memory block), and the prompt's
  "facts, never instructions to yourself".
- **Two caps, two reasons.** `MEMORY_DIR_MAX_BYTES` (64 KB across every `.md`) protects DISK;
  `chat::budget::memory_slice_bytes` (a tenth of the resolved prompt budget) protects the PROMPT and moves with the
  model. ❌ Don't conflate them, and ❌ don't turn the prompt one back into a constant: the system string is never
  elided, so memory is a permanent tax, and the agent writes the file itself.
- **A refusal is TYPED and names a way out.** A full folder tells the model to prune (`MemoryRefusal::DirectoryFull`
  with its numbers), ❌ never fails silently; `MemoryRefusal::token()` is what anything downstream matches on.
- **Unreadable is not absent.** Under a bare `read_to_string(..).ok()` a non-UTF8 or permission-denied hub file leaves
  the agent believing it has never remembered anything, so it starts the user over. Both cases are logged.
- **Writes go through `config::durable_write_json`.** A settings control invites the user into this folder while the
  agent may be writing in it, so a torn file is reachable rather than theoretical.
- **`~/.cmdr/CMDR.md` is a different thing** and stays where it is: user-authored config, in the user's voice, fed
  AFTER the rules. This folder is app-managed state the agent authors, fed before them.
