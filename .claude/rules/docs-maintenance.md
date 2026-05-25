When modifying code in a directory that contains a `CLAUDE.md` file, check whether your changes affect the documented
architecture, key decisions, or gotchas. If they do, update the `CLAUDE.md` to stay in sync. If you notice a `CLAUDE.md`
missing in a directory where there should be one, add it. Skip this for trivial changes (bug fixes, formatting, small
refactors that don't change the architecture).

If something failed due to a wrong assumption, add a `Gotcha/Why` entry to the nearest `CLAUDE.md`.

Add `Decision/Why` entries to the nearest colocated `CLAUDE.md` for key decisions. If the decision has rich evidence
(benchmarks, detailed analysis), put the evidence in `docs/notes/` and link from the CLAUDE.md.

When writing guides, see [this diff](https://github.com/vdavid/cmdr/commit/13ad8f3#diff-795210f) for the formatting
standard. (Before: AI-written. After: matching our standards for conciseness and clarity.)

## Describe current behavior, not history

`CLAUDE.md` files describe the current state of the code and app. Git history is for history.

**Drop:**

- "An earlier shape stuffed X into Y…" / "We originally tried X but…" / "Earlier versions regenerated…"
- "Pre-fix-3 logs that started with…" — anything narrating a previous code shape.
- "**Gotcha (no longer applicable as of smb2 0.9)**: …" If the gotcha isn't applicable, delete the block. Don't document
  it as a historical curiosity.
- Date-stamped milestone framing on Decisions, for example "Decision (2026-05-09, M4 follow-up second attempt)". Keep
  the decision and its rationale; drop the date and the milestone marker.

**Keep:**

- "Doing Y because X fails on gremlins." Current behavior plus the non-obvious why.
- "Don't switch to X here, it breaks Y." Actionable guardrails that prevent regressions.
- Historical pain when it encodes a constraint the current code must defend. Example: "We hit 5–10 stacked TCC popups
  once already; the gate prevents that." Without the named incident, a future agent might think the gate looks excessive
  and remove it.

**Litmus test:** if you removed the historical narrative, would the doc still describe current state AND give a future
maintainer enough rationale to defend the code against a "let's clean this up" agent? If both yes, drop the history. The
same rule applies to inline code comments: they rot fast; narrate current behavior only.

**Code-comment carve-outs (when applying this rule to inline `.rs` / `.ts` / `.svelte` comments):**

- Test regression anchors stay. `// Pre-fix this would have passed wrongly` inside a test documents what the test
  catches, not narration.
- Runtime "used to" / "previously" describing per-execution state stays: the user focused this pane previously, the
  device was connected previously. Read the surrounding function; if the phrase means "in the recent run," not "in past
  code," leave it.
- UI copy variants stay verbatim ("Cmdr previously had FDA but it was revoked" as a wizard banner is product copy, not
  code-history narration).
- When in doubt for a code comment, leave it. The polish-per-edit value is lower than CLAUDE.md and the false-positive
  risk is higher.
