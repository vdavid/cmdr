# Tool lines (`agent/line/`)

What the rail's tool line says about one call, as typed facts: what it looked at (`ToolSubject`), what came back
(`ToolOutcome`), what the model was shown (`ToolPreview`), and whether it repeats an earlier call (`repeat_marks`).
Named after the UI's "tool line".

## Module map

- `mod.rs`: the wire vocabulary (`LineRef`, the enums, `ToolPreviewRow`) and the string bound.
- `subject.rs` / `outcome.rs` / `preview.rs` / `repeat.rs`: one pure function each. `tests/`: a table per tool.

## Must-knows

- **Pure functions of what `main.db` stores** (a call's arguments, its result content), so the live stream and a
  reloaded thread compute the same facts, and an old thread needs no migration. ❌ Never take state the store lacks.
- **Facts, never words.** Every field is a typed enum, a number, a path, or the model's own text; the frontend picks
  the copy. The one exception: `ToolPreview.problem`, the verbatim sentence the model was told.
- **Read OUR OWN result keys** (`available`, `repeatedCall`, `readyForReview`, `problem`, `refused`, `total`, ...),
  never wording, and ❌ never add a key to a tool result to make this easier: the model pays tokens for it.
- **`outcome_of`'s check order is load-bearing**: the breaker keeps `problem` beside `repeatedCall`, and every propose
  problem carries `readyForReview: false`. `DETAILS.md` § The check order.
- **A line is `(messageId, callId)`, never the call id alone**: ids collide across rounds.
- **Every derivation is total and bounded**: any argument or result shape gives a fallback (`Unparsed`, `Answered`),
  never a panic; untrusted strings are cut at `MAX_TEXT_CHARS`.
- **New agent tool** = a `subject_of` arm, an `outcome.rs::answered` arm, a `preview.rs::rows_of` arm (all
  exhaustive, no `_`), and a success fixture (`every_known_tool_has_a_success_row` pins it).

Per-tool mapping, the decisions, and what the preview may claim: `DETAILS.md`.
