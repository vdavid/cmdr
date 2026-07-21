# Rename proposal details

The store is feature-local because its opaque ids and immutable rows are the authority boundary for review and apply commands. Entries expire in memory and are deliberately not persisted in chat history. A successful preflight records both the exact allowed row-id set and server-only source fingerprints; Apply atomically consumes that pair, so a dialog cannot replay an already-started plan or substitute a different subset.

Proposal validation reads the `PaneStateStore` cache and index registration only. It does not call live filesystem APIs: a dead mount must not hang an agent turn, and symlinks remain links rather than targets.
