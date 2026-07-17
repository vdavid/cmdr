# Style guide

Writing, code, and design styles.

## Writing

- Voice
  - **The website speaks product-first: no "I", no "we"**: On getcmdr.com, write "Cmdr indexes your drive" and "your
    feedback shapes what's next", never "we built" or "I'm improving". "We" overstates a one-person project; "I"
    overemphasizes that it's one person. Exceptions: FAQ questions and testimonials speak as the user ("Can I use
    it…?"), blog posts are signed personal writing, and legal pages keep the conventional "we".
  - **The app may speak as David where it's deliberately personal**: the onboarding beta step and the About window sign
    their copy ("Hi, I'm David!"). That's intentional warmth, not a violation of the website rule.
- Specific terms
  - **Folder vs directory**: We know these mean the same. We allow both. Use whichever feels better in each situation.
    Like, on the backend, listing "folders" with `readdir` feels wrong, but also, "folder" comes more natural on the
    front-end and end-user docs.

## Code

Comments: Only add docs that actually add info. No tautologies.

- ✅ Add meaningful comments for public functions, methods, and types to help the next dev.
- ❌ BUT DO NOT use comments for stuff like `Gets the name` for a function called `getName` :D
- ⚠️ Before adding a comment, try using a more descriptive name for the function/param/variable.
- ❌ DO NOT repeat TypeScript/Rust/Go types in the docs (like `@param`/`@returns`).
- ✅ USE comments to mark caveats, tricky/unusual solutions, formats (`YYYY-MM-DD`), and constraints (`must end with /`)

### Rust

- Max 120 char lines, 4-space indent, cognitive complexity threshold: 15, enforced by clippy.
- Wrap comments (`//`, `///`, `//!`) at 100 chars. Stable `rustfmt` doesn't enforce this (it's a manual convention). To
  bulk-reflow, run nightly `rustfmt` once with `unstable_features = true`, `wrap_comments = true`, `comment_width = 100`
  in `rustfmt.toml`, then revert the config.

### Agent-facing docs

Agent-facing docs (`CLAUDE.md`, `DETAILS.md`, `AGENTS.md`, everything under `docs/`, and `.claude/rules/`) are read by
AI agents as a linear token stream, not a 2D layout. So:

- **No two-column tables.** Use a bullet list instead, one item per row, formatted `- **Title**: details`: a bold title,
  then a literal ": " (colon + space) separator, then the rest of the row. Never a dash or em-dash as the separator. The
  padding in an aligned table wastes tokens and, worse, any edit reflows every row, which causes spurious git merge
  conflicts. A three-column table whose first column is a pure sequential index counts as two columns: drop the index (a
  numbered list works if the index is meaningful) and bullet the two real columns. The `docs-table-hygiene` check
  enforces this (error-level, no allowlist: a two-column table is always convertible).
- **No column wider than 100 characters.** oxfmt pads every cell in a column out to its widest cell, so a single
  prose-or-list cell taxes the whole column with padding tokens. A column that wide isn't tabular data: trim the cell,
  or destructure the table into sections (a `###` heading per row) or bullets. `docs-table-hygiene` enforces this too,
  error-level and no allowlist (every wide column is fixable by trimming or destructuring).
- **Genuine matrices stay tables.** A table with three or more meaningful columns (a capability grid with ✓/✗ across
  several columns, a comparison table) is legitimately 2D, so keep it. The check flags only the two-column case and
  oversized columns, never a real matrix whose cells stay short.

This is the doc-specific form of the user-level `agent-facing-docs` rule (skip column-alignment padding; prefer simple
bulleted lists). Human-facing markdown is exempt: `README.md`, `CONTRIBUTING.md`, `CHANGELOG.md`, and everything under
`apps/website/` and `brand/` stay human-optimized and may use tables freely.

## Design

See [design-principles.md](design-principles.md) for product design values (UX, accessibility, cancellation, platform
behavior). Read it when designing features or making UX decisions.

Always read the [frontend guide](frontend-guide.md) too if you're working on front end stuff like TS/Svelte or Astro!
