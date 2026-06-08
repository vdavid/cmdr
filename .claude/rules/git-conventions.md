## Commit messages

We land changes directly on `main`, so the commit message is where a change gets explained. Make it good.

- **Lead with impact.** The title says what the change achieves and why, not the mechanics. This feeds `CHANGELOG.md`
  and the release notes, which are impact-focused.
- Optional prefix is fine ("Bugfix: ", "Docs: ", "Tooling: ", "File viewer: ", etc.).
- Verbose bodies are welcome: don't compress to fit a length limit. Prefer bullets for details. No hard title length
  cap; keep it tight without mangling it.
- No word wrap in the body (let the terminal/viewer wrap). Enclose entities in `` ` ``. No `Co-Authored-By`.

## PRs

We don't use PRs. Changes land on `main` via fast-forward merge from a worktree branch. If David ever asks for a PR
explicitly: casual/informal title, a concise bulleted description (no headings), and a single "## Test plan" heading at
the bottom explaining how the change was tested.
