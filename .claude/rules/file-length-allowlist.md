❌ NEVER modify `scripts/check/checks/file-length-allowlist.json` unless the user explicitly asks for it. The
file-length check is warn-only — it doesn't fail the suite. The allowlist exists to track current file sizes; bumping it
as a side effect of a feature change hides growth that should be addressed by trimming the file or splitting it. If a
file you're touching exceeds its allowlisted count and the warning is annoying, leave it as a warning and surface it to
the user, don't silently raise the limit.
