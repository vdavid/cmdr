# Handling an outside contributor's pull request

Our own PRs are plumbing that `pnpm land` opens (`scripts/land/CLAUDE.md`), but people send real ones. The goal is that
a contributor does nothing beyond opening one: no rebase request, no formatting fixups, no "please squash". `main` moves
several times an hour, so anything we ask someone to redo is stale before they read the message.

## The flow

1. **Take the code into a worktree.** `~/.claude/scripts/new-worktree.sh pr<N>-<slug>`, then
   `git fetch origin pull/<N>/head` and `git cherry-pick FETCH_HEAD`. A conflict in a generated file (`bindings.ts`) is
   normal: resolve it and let `bindings-fresh` confirm.
2. **Fix whatever CI would reject, ourselves.** An outside tree is usually not `rustfmt` / `oxfmt` clean, and new
   must-knows can push a `CLAUDE.md` past its word budget. Land those as separate `style:` / `docs:` commits so the
   contributor's own commit stays theirs.
3. **Leave their commit message alone.** It feeds the release notes, but our conventional-commits rule is for us, and
   rewriting someone's words to satisfy it buys nothing.
4. **Land with `pnpm land -record <N>`.** It lands the branch like a plain `pnpm land`, then records the PR as merged
   (below). Re-run it at any stage; once PR N reads merged, it says so and stops.
5. **Comment and thank them**, naming what landed and what we changed on top.

## Recording the PR head so GitHub marks it merged

GitHub marks a PR merged only when its head SHA becomes an ancestor of the base branch, or when the merge runs through
GitHub's own API. Cherry-picking rewrites SHAs, so without a record the PR stays open forever. The record is an `ours`
merge of the PR head on top of `main`: it makes the head an ancestor while leaving the tree byte-identical.

`-record N` builds it in a land of its own, after the branch landed: a `record-pr-<N>` branch holding the current
`origin/main` plus that merge (`chore: record PR #<N> as merged`), checked for an empty tree diff and the PR head's
ancestry before it's pushed. It goes through CI like any land, fast-forwards `main`, waits for PR N to read merged, and
deletes the record branch. Since it changes no file, CI skips every lane, `hygiene` included, and goes green in about a
minute (`docs/tooling/ci.md` § The hygiene job). The why and the mechanics: `scripts/land/DETAILS.md` § Decision: record
a contributor PR in its own land.

- **Gotcha / Why:** the record's diff against `main` is empty, which reads like a mistake. That's what it's for: it
  carries only the ancestry. Don't "clean it up".
- ❌ **Don't put the merge on the content branch.** A merge commit can't ride the rebase a busy `main` forces, so `land`
  refuses such a branch once `main` moves.

We take these merge commits in an otherwise linear history on purpose: the Merged badge is part of what a contributor
gets for the work, and a gray "Closed" reads as a rejection.
