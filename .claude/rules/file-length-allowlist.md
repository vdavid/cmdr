Rules for `scripts/check/checks/file-length-allowlist.json`:

✅ **OK without asking**: removing an entry for a file that no longer exists, or lowering an
existing entry's number when the underlying file shrank below it. Both are tightening the
contract — they surface growth earlier, never hide it.

❌ **Never without explicit user consent**: adding a new entry, raising an existing entry's
number, or any other change that loosens the contract. The allowlist exists to track current
file sizes; bumping it as a side effect of a feature change hides growth that should be
addressed by trimming or splitting the file. If a file you're touching exceeds its allowlisted
count and the warning is annoying, leave it as a warning and surface it to the user.

The file-length check is warn-only — it doesn't fail the suite — so leaving the warning is
always safe.
