# cmdr-about

"Tell me about this": the facts Cmdr states about a file or folder, and every rule a person could argue about (what a
quarantine field means, what counts as a type mismatch, what removal costs). Pure and tauri-free; the app gathers raw
inputs and hands them here. No fact depends on a model.

## Module map

- **`fact.rs`**: `Fact<T>` (known, unknown, not applicable, pending), `UnknownWhy` and its `Prominence`, `FactKind`,
  `AboutTarget`. **`text.rs`**: `ItemText`, `model_text`.
- **`payload.rs`, `digest.rs`**: what each fact carries. **`owner.rs`**: the owner claim, plus `OwnerQuestion` (what a
  path names, then how the host's lookups settle it).
- **`provenance.rs`** (quarantine, WhereFroms, xdg, `redact_for_egress`), **`sniff.rs`** (magic bytes against the
  extension): decoders of attacker-written bytes.
- **`card.rs`**: `plan`, `AboutPlan`, `AboutFact`, `AboutCard`. **`removal.rs`**, **`headline.rs`**: derived from a
  card. **`actions.rs`**: the footer's next actions. **`model.rs`**: `for_model`. **`gather.rs`**: the grace-then-late
  engine. **`fuzzing.rs`**: the fuzz target's property seam.

## Must-knows

- ❌ **No empty string, zero, or `false` for "don't know".** A fact is a `Fact`; a field Cmdr may not know is a nested
  `Fact`; an `Option` is a definite "the record has none", and its doc says which. `NoRecord` is never "never".
- ❌ **No "safe to delete", anywhere.** `RemovalConsequence` has no reassuring variant and must never gain one;
  `RecordedDownload` is never "you can get it again". `CantTell` is a valid answer.
- ❗ **What an item says about itself is a claim.** Every such string is an `ItemText` (the only constructor sanitizes),
  so a row or the projection can't receive one that skipped `model_text`. ❌ Never a plain `String` for a name, signer,
  agent, or branch. A `path` field is the exception: raw, for navigating, ❌ never shown.
- ❌ **Nothing reaches a model but `model::for_model`**: a download as its host or kind (`redact_for_egress`, the only
  redaction), no raw path. A new field needs its consent line and a version bump. `DETAILS.md` § The model's projection.
- ❗ **Decoders cap first** (64 KB xattr, 16 entries) and parse the WHOLE entry before any cut: a host read from a
  truncated address can be a piece of its user info.
- ❗ **The footer is the facts'** (`actions`): removal never first; only `web_page` turns an address into an action.
- ❗ **A signature is never "verified"**: unless `checked` is `ExecutableMatches`, the wording is "carries a signature
  from".
- ❌ **"Left over from an app that isn't installed" is never said of a group container or a `com.apple.*` id**: macOS
  keeps services outside the app folders, and a wrong "left over" invites deleting live data. `DETAILS.md` § The owner.
- ❗ **The plan decides the rows, before any job.** A row that can't apply is left out, ❌ never drawn as
  `NotApplicable`. `AboutCard::apply` refuses a fact the plan has no row for.
- ❗ **Prominence is decided here** and rides on the wire inside `Unknown`; ❌ the UI never decides what's worth a
  sentence.
- ❗ **The wire is one-way**: types serialize (`camelCase`) through specta, pinned to the app's version; doc comments
  land in `bindings.ts`.
- ❌ **No English.** The headline and the removal row are typed parts plus a template key; the frontend words them.
- ❌ **No OS `cfg`, no filesystem call, no `tauri`**: every decoder takes bytes and runs the same everywhere. A deadline
  in `gather` bounds how long the collector waits, never how long a job works.
- ❗ **Every fuzz assertion has a mutant on record** (`fuzz/fuzz_targets/about_provenance.mutants.json`); the seam's
  checks never call the predicate they guard.
- The public surface is capped by `index-crate-isolation`.

Design, every rule, and evidence: `DETAILS.md`. Read it before any non-trivial work here.
