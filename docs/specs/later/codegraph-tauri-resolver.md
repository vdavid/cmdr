# Teach CodeGraph to trace Cmdr's Tauri IPC

Status: not started. Priority: low / opportunistic (spare-capacity work). Owner: unassigned.

## Overview (the impact)

Today CodeGraph is **blind across Cmdr's Tauri IPC boundary**. Ask it "what calls this Rust command?" and it returns "no
callers" even when the whole frontend uses it. Ask for the blast radius of changing a command or an event and it stops
dead at the Rust/TS seam. Two facts measured directly in this codebase:

- `codegraph_callers(get_mcp_port)` → **"No callers found"**, while `commands.getMcpPort()` is called from
  `tauri-commands/settings.ts:159` and consumed in Settings.
- Every `#[tauri::command]` and every emitted event is, to CodeGraph, a dead end on one side of the wire.

This project closes that gap: a **Tauri IPC framework resolver** for CodeGraph that joins TS `commands.*` / `events.*`
(and raw `invoke()` / `listen()`) to their Rust handlers and emitters. Once it lands, **every agent's call-graph and
impact queries become correct across Cmdr's Rust↔TS boundary** with zero per-query effort:

- `codegraph_callers` on a Rust command returns its real frontend callers.
- `codegraph_impact` on a command or event includes the frontend consumers, so "what breaks if I change this" finally
  spans the wire, the single most common refactor question in a Tauri app.
- Event emit↔listen edges become visible (made trivially joinable by the
  [typed-events migration](../typed-events-plan.md), which gave every event a `PascalCase` Rust struct ↔ `camelCase` TS
  symbol pairing).

It feeds **the graph agents already query**, not a separate tool, so the win is automatic and invisible-in-a-good-way.

### Honest scope and caveats (read before committing time)

This was recommended _against_ as a pure-ROI play, and that reasoning still holds; it's listed here because it's a
pleasant, well-scoped, opportunistic contribution, not because it's urgent. Be clear-eyed:

- **It's navigation / impact-analysis, not correctness.** Tauri commands are already compile-time-safe via
  `tauri-specta` (rename a command → `bindings.ts` regenerates → the TS wrapper fails `svelte-check` + `bindings-fresh`
  fails). So this can't catch a class of bug that isn't already caught; it makes _exploration and blast-radius_ correct.
- **It does NOT fix CodeGraph's intra-Rust call-graph weakness.** Separately measured: `codegraph_callers` returned **0
  of 18 real Rust callers** for `is_fda_pending_runtime` (it misses path-qualified `crate::module::fn()` calls). That is
  a different CodeGraph limitation, **out of scope** here. This resolver fixes the _cross-language IPC_ edges only.
- **Delivery is an upstream PR, and that gates everything.** CodeGraph resolvers are compiled in-tree; there is no
  external plugin loader (verified in 0.9.9). So the resolver lives in a fork of `colbymchenry/codegraph` and ships via
  PR. CodeGraph releases roughly **weekly** (0.9.0 → 0.9.9 in ~2 weeks), so running a long-lived fork carries a real
  rebase tax. **Open a maintainer issue first and get a soft yes before writing code** (see Milestone 0). If the
  maintainer is slow or lukewarm, stop: the fork tax outweighs the gain.

## Background a future agent needs (this is "later" work; assume no context)

**What CodeGraph is:** a local code-knowledge-graph indexer + MCP server (`@colbymchenry/codegraph`, MIT). It parses
each language with tree-sitter into nodes (functions, structs, components, etc.) and edges (calls, imports, references),
stored in SQLite, queried via MCP tools (`codegraph_search`, `codegraph_callers`, `codegraph_impact`, etc.). Cmdr uses
it (see `.codegraph/` at repo root and David's setup notes in `~/.claude/CLAUDE.md`).

**Why it can't see IPC:** the link between a TS `commands.getMcpPort()` call and the Rust `get_mcp_port` fn is a runtime
IPC hop, not an AST edge, and the two sides are different languages with different names (`getMcpPort` ↔
`get_mcp_port`). No per-language extractor can bridge that; it needs a cross-language _framework resolver_.

**Why now / why feasible:** CodeGraph's 0.9 line shipped cross-language native↔JS bridge resolvers (React Native bridge,
TurboModules/Fabric, Swift↔ObjC, "native-to-JS event channels"). **Tauri IPC is structurally the same problem** as the
React Native bridge: a native (Rust) side and a JS side joined by name across a runtime boundary, plus native↔JS event
channels (our typed events). So this is no longer novel cross-language research; it follows an established, shipped
pattern with a direct template.

## Verified findings (checked against CodeGraph `0.9.9` source, do re-verify against latest)

Cloned and inspected `github.com/colbymchenry/codegraph` at `0.9.9` (latest on npm as of 2026-06-02; David's local
install was `0.7.9`, so update locally too, pinning the newest ≥14-day-old version per the use-latest-dep-versions
rule).

- **Resolvers live in `src/resolution/frameworks/`**, one file each, registered in `frameworks/index.ts` (~142 lines).
  In-tree only; no external plugin API.
- **The interface is `FrameworkResolver` at `src/resolution/types.ts:157`**:
  `{ name, languages?: Language[], detect(), resolve(ref, context), extract?(filePath, content) }`. `extract()` returns
  `{ nodes, references }` (references are `UnresolvedRef` with a `referenceName`); the resolution pipeline matches a ref
  to a node by name and emits a typed edge.
- **`react-native.ts` (481 lines) is the direct template.** It declares
  `languages: ['javascript','typescript','tsx', 'jsx','objc']` so `extract()` sees both JS and native files; it indexes
  native methods **"by JS-visible name"**; and `resolve()` **"only redirects JS callers"**, matching a JS callsite to
  the native node (`entries.find(e => e.node.language === 'objc')`) and returning `{ resolvedBy: 'framework' }`. Swap
  "ObjC method via NativeModules" for "Rust fn via `commands.*` / `invoke()`" and that is the Tauri resolver. Other
  useful references: `fabric.ts` (411), `expo-modules.ts` (198), `swift-objc.ts` (299), `rust.ts` (335). **No `tauri.ts`
  exists** (confirmed novel).
- **Edges from heuristic bridges are marked `provenance: 'heuristic'`** with metadata. The IPC link is inferred, so do
  the same (honest, and what their dynamic-dispatch playbook expects).
- **Tests: Vitest, fixture-based.** Tests write real files into temp dirs (`fs.mkdtempSync`, cleaned in `afterEach`),
  run the real pipeline via the `CodeGraph` class, and assert on nodes/edges via real SQLite (no mocking). Template:
  `__tests__/frameworks-integration.test.ts`. Dev loop: `npm run build`, `npm test` / `npx vitest run <file>`,
  `npm run cli -- index <proj>` then `... query <proj> <symbol>`.
- **Contribution bar (from CodeGraph's CLAUDE.md):** validate on small/medium/large real repos with ≥3 flow prompts
  each; deterministic `codegraph_explore` probes confirming the flow connects end-to-end; synthesized edges marked
  `provenance:'heuristic'`; record results in `docs/design/dynamic-dispatch-coverage-playbook.md`; add a CHANGELOG
  `## [Unreleased]` entry. **Cmdr is the "large real repo"** for this validation.

## Size estimate

~**350–500 LoC** for the resolver itself (`tauri.ts`), benchmarked against `react-native.ts` (481) and `fabric.ts`
(411). ~**800–1,000 LoC total** for a mergeable PR (≈half production, ≈half tests + a fixture Tauri sample project +
CHANGELOG + playbook entry). Single-file feature, heavy test ratio, which is why it's so TDD-friendly.

## What the resolver actually does

Make it **general** (the maintainer wants Tauri support, not Cmdr-specific support), covering both the typed-specta
convention Cmdr uses and the raw Tauri convention other apps use:

**Commands** (Rust fn ← TS call):

- Rust side: a fn annotated `#[tauri::command]` is the target node (CodeGraph's Rust extractor already emits it). Index
  it by its JS-visible name.
- TS side: emit a reference from each `commands.fooBar(...)` (tauri-specta) **and** each `invoke('foo_bar', ...)` (raw)
  call site.
- Join: tauri-specta converts `snake_case` Rust → `camelCase` TS deterministically, so `getMcpPort` ↔ `get_mcp_port`.
  Raw `invoke('foo_bar')` already uses the exact `snake_case` wire string (even easier, no conversion).

**Events** (Rust emit ↔ TS listen):

- Rust side: a struct deriving `tauri_specta::Event` (or a `.emit("name", payload)` call) is the source. Index by name.
- TS side: emit references from `events.fooBar.listen(...)` / the typed `on*` wrappers in `tauri-commands/` **and** raw
  `listen('event-name', ...)`.
- Join: Cmdr's typed events pair `PascalCase` Rust struct (`VolumeSpaceChanged`) ↔ `camelCase` TS symbol
  (`events.volumeSpaceChanged`) ↔ `kebab-case` wire name (`volume-space-changed`); raw `listen('volume-space-changed')`
  uses the kebab string. All deterministic.
- Edge direction: model emit→listen and/or a shared event node both sides reference, matching how `react-native.ts` /
  the native-to-JS event-channel work models it (check the template).

**Gracefully skip what can't be statically resolved** (don't error): runtime-built names like Cmdr's `mcp-*` relay
(`app.emit(event, ...)` with `event: &str`) and `viewer:file-changed:<session_id>`. These are intentionally string-based
(see the typed-events plan's "stays string-based" list) and a resolver can't model a name it can't read. Mark them
unresolved and move on.

## Milestones

**M0: Gate on the maintainer (do this first, before any code).** Open an issue/discussion on `colbymchenry/codegraph`
proposing a Tauri native↔JS bridge resolver "in the vein of your React Native / Fabric / native-to-JS-channel
resolvers." Confirm: (a) they want it in-tree, (b) the cross-language edge shape (TS ref → Rust node) is acceptable, (c)
`provenance:'heuristic'`. **If no clear yes, stop here.** DONE: a maintainer signal to proceed.

**M1: Fork + green baseline.** Fork `colbymchenry/codegraph`, clone to `~/projects-git/vdavid/codegraph`,
`npm install && npm run build && npm test` green on latest `main`. Read `react-native.ts`, `fabric.ts`, `swift-objc.ts`,
`types.ts` (the interface), `frameworks/index.ts` (registration), and `frameworks-integration.test.ts` (test pattern).
DONE: baseline builds + tests pass; you can articulate the RN resolver's extract/resolve flow.

**M2: Commands, TDD.** Red→green: write failing unit tests for `extract()` (a `commands.fooBar(` and an
`invoke('foo_bar'` each yield a ref with the right `referenceName`) and `resolve()` (the ref matches a Rust
`#[tauri::command] fn foo_bar` node, JS callers only). Then implement the command half of `tauri.ts`. Register it in
`frameworks/index.ts`. DONE: command edges resolve in unit + a small fixture; `provenance:'heuristic'`.

**M3: Events, TDD.** Same red→green for events: `events.fooBar.listen` / `on*` wrappers / raw `listen('foo-bar'` resolve
to the Rust `Event` struct / emit site. Handle the `PascalCase`/`camelCase`/`kebab` joins. DONE: event edges resolve;
dynamic-name relays are gracefully skipped (asserted).

**M4: Fixture integration.** A minimal fixture Tauri project (one command, one call site, one typed event emit+listen)
under the test harness; index it via the `CodeGraph` pipeline; assert the cross-language edges exist via SQLite, per
`frameworks-integration.test.ts`. DONE: end-to-end fixture green.

**M5: Cmdr acceptance (the headline probe).** Point the patched CodeGraph at the Cmdr repo and confirm the
known-currently-broken queries now work: `codegraph_callers(get_mcp_port)` returns `tauri-commands/settings.ts`'s
`getMcpPort`; pick 2–3 more commands + an event for ≥3 flow prompts (the contribution bar). Record in the
dynamic-dispatch playbook. DONE: the measured-broken cases are green on Cmdr.

**M6: PR.** CHANGELOG `[Unreleased]` entry, the playbook record, a tidy PR referencing the M0 issue. Iterate with the
maintainer. DONE: merged upstream. Then delete the fork; Cmdr benefits once the local CodeGraph is updated to the
release that includes it.

## Test plan / TDD

This is near-ideal for TDD because the resolver is pure (`extract`/`resolve`: string + AST → structured output, no I/O,
no runtime, no app launch, the inverse of the typed-events migration's E2E-only regression):

1. **Pure unit (red→green per case):** content snippet in → expected nodes/refs out; ref in → expected resolved node.
2. **Fixture integration:** temp-dir Tauri project → real pipeline → SQLite edge assertions (their harness).
3. **Real-world acceptance:** the `get_mcp_port` "no callers" case is a ready-made failing system test that goes green
   when it works. Use it as the north star.

## Acceptance criteria (DONE for the whole project)

- A `tauri.ts` resolver merged into `colbymchenry/codegraph`, registered, covering typed (`commands.*`/`events.*`) and
  raw (`invoke`/`listen`) Tauri IPC, with `provenance:'heuristic'` edges.
- Unit + fixture-integration tests in the CodeGraph style, all green.
- On Cmdr: `codegraph_callers` / `codegraph_impact` return the cross-language callers/consumers for commands and events
  that previously showed none (`get_mcp_port` and friends).
- CHANGELOG + dynamic-dispatch-playbook entries.
- Dynamic-name relays (`mcp-*`, `viewer:file-changed:<id>`) are skipped without error.

## Risks and caveats (footer)

- **Upstream gate is load-bearing.** Weekly releases → a long-lived fork is costly. M0 is not optional; a lukewarm
  maintainer is a stop signal, not a "fork and maintain it ourselves" signal.
- **Re-verify the source.** These findings are pinned to `0.9.9`; CodeGraph moves fast. Re-read `react-native.ts`, the
  interface, and the registration before building; the native-bridge infra may have shifted (likely improved).
- **Generality vs. Cmdr-specificity.** Cmdr's typed `commands.*`/`events.*` + `on*` wrappers are the clean case; a
  general resolver must also handle raw `invoke`/`listen`. Don't ship a Cmdr-only resolver to upstream.
- **Value ceiling.** Navigation/impact, not correctness (specta already guards command name-drift). Worth doing for the
  craft + the ecosystem gap + the automatic, zero-maintenance-once-merged Cmdr benefit, not because Cmdr is hurting
  without it.

## References

- This codebase: the typed-events groundwork that makes events name-joinable:
  [`typed-events-plan.md`](../typed-events-plan.md) (note: that file lives in `docs/specs/`, may have been swept; the
  typed-events pattern is in `lib/ipc/CLAUDE.md`).
- CodeGraph: [`github.com/colbymchenry/codegraph`](https://github.com/colbymchenry/codegraph) (MIT),
  [framework-resolver design](https://github.com/colbymchenry/codegraph/blob/main/docs/plans/2026-04-24-framework-resolver-extract.md),
  [languages reference](https://colbymchenry.github.io/codegraph/reference/languages/). Key files:
  `src/resolution/types.ts` (interface), `src/resolution/frameworks/{react-native,fabric,swift-objc,rust,index}.ts`,
  `__tests__/frameworks-integration.test.ts`.
- David's CodeGraph setup notes (native `better-sqlite3` build, node-22 pin, version cooldown): `~/.claude/CLAUDE.md` §
  CodeGraph. Relevant when updating the local install to a release that includes this resolver.
- Prior art that is NOT this (typed-IPC generators, not analyzers): [TauRPC](https://github.com/MatsDK/TauRPC),
  `tauri-specta` (which Cmdr already uses).
