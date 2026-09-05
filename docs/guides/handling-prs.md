# Handling an outside contributor's pull request

We don't open PRs ourselves (`AGENTS.md` § Workflow), but people send them. The goal is that a contributor does nothing
beyond opening one: no rebase request, no formatting fixups, no "please squash". `main` moves several times an hour, so
anything we ask someone to redo is stale before they read the message.

## The flow

1. **Take the code into a worktree.** `~/.claude/scripts/new-worktree.sh pr<N>-<slug>`, then
   `git fetch origin pull/<N>/head` and `git cherry-pick FETCH_HEAD`. A conflict in a generated file (`bindings.ts`) is
   normal: resolve it and let `pnpm check` confirm against `bindings-fresh`.
2. **Fix whatever CI would reject, ourselves.** An outside tree is usually not `rustfmt` / `oxfmt` clean, and new
   must-knows can push a `CLAUDE.md` past its word budget. Land those as separate `style:` / `docs:` commits so the
   contributor's own commit stays theirs.
3. **Leave their commit message alone.** It feeds the release notes, but our conventional-commits rule is for us, and
   rewriting someone's words to satisfy it buys nothing.
4. **Rebase onto current `main`, run `pnpm check`, FF-merge, push.**
5. **Record the PR head** so GitHub marks it merged (below).
6. **Comment and thank them**, naming what landed and what we changed on top.

## Why the PR still reads "Open" after it lands, and how to fix it

GitHub marks a PR merged only when its head SHA becomes an ancestor of the base branch, or when the merge runs through
GitHub's own API. No endpoint just sets the flag. Cherry-picking and rebasing both rewrite SHAs, so after a normal
landing the original head sits nowhere in `main`'s history and the PR stays open forever.

Record it with an `ours` merge, which makes the head an ancestor while leaving `main`'s tree byte-identical:

```sh
git fetch origin pull/<N>/head:refs/tmp/pr<N>
git merge -s ours --no-ff refs/tmp/pr<N> -m "chore: record PR #<N> as merged (landed as <sha>)"
git push origin main
```

Check both of these before pushing: `git diff <pre-merge-sha>` must print nothing, and
`git merge-base --is-ancestor refs/tmp/pr<N> HEAD` must succeed. GitHub closes the PR with the Merged badge on push.

**Gotcha / Why:** the merge commit's diff against its first parent is empty, which reads like a mistake. That's what it
is for. The content landed in step 4; this commit carries only the ancestry GitHub needs to recognize it. Don't "clean
it up".

We take these merge commits in an otherwise linear history on purpose: the Merged badge is part of what a contributor
gets for the work, and a gray "Closed" reads as a rejection.
