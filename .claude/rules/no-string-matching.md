# No string-matching for error/state classification

❌ Don't branch on substrings of a message, stderr, or error title to classify errors, state, or control flow: wording
is for users and breaks silently on copy edits, OS localization, or upstream reformatting. Use a typed enum variant, an
errno, or an explicit flag that crosses the IPC boundary. Tests too: prefer
`matches!(err, VolumeError::AlreadyExists(_))` over `err.message.contains(...)`. Enforced by `error-string-match` (Rust)
and `cmdr/no-error-string-match` (TS). Opt out only when unavoidable, with the documented `allowed-error-string-match` /
`eslint-disable` comment plus `LC_ALL=C` and a snapshot test.
