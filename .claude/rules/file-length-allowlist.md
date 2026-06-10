Rules for `scripts/check/checks/file-length-allowlist.json`:

✅ **Automated (no action needed)**: the check shrink-wraps the `files` section on local runs — it removes entries for
files that no longer exist or shrank under the threshold, and ratchets entries down when the file has more than 10%
slack. Don't do these edits by hand; just run `pnpm check file-length` and commit the rewrite.

❌ **Never without explicit user consent**: adding a new entry (to `files` or `exempt`), raising an existing entry's
number, or any other change that loosens the contract. The allowlist exists to track current file sizes; bumping it as a
side effect of a feature change hides growth that should be addressed by trimming or splitting the file. If a file
you're touching exceeds its allowlisted count and the warning is annoying, leave it as a warning and surface it to the
user.

The `exempt` section is for generated files whose length is not actionable (for example `bindings.ts`); entries need a
reason and the same consent rule applies to adding one.

The file-length check is warn-only — it doesn't fail the suite — so leaving the warning is always safe.

The same contract applies to `scripts/check/checks/claude-md-length-allowlist.json` (the `claude-md-length` check, which
caps push-tier CLAUDE.md word counts): the check shrink-wraps stale `files` entries on local runs (remove gone /
under-threshold, ratchet >10% slack down), and adding or raising an entry needs explicit user consent. If a CLAUDE.md
you're touching exceeds its allowlisted word count, move depth into the colocated `DETAILS.md` rather than bumping the
number; leaving the warn is safe (it's warn-only too).
