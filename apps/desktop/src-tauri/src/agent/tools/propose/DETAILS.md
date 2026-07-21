# Rename proposal details

The store is feature-local because its opaque ids and immutable rows are the authority boundary for review and apply commands. Entries expire in memory and are deliberately not persisted in chat history. A successful preflight records both the exact allowed row-id set and server-only source fingerprints; Apply atomically consumes that pair, so a dialog cannot replay an already-started plan or substitute a different subset.

Proposal validation reads the `PaneStateStore` cache and index registration only. It does not call live filesystem APIs: a dead mount must not hang an agent turn, and symlinks remain links rather than targets.

Preflight owns row warnings as well as blockers. It compares the final filename extension case-insensitively and marks
extension additions, removals, and changes without blocking them. A renamed dotfile still has no extension; a trailing
dot is an empty extension and therefore differs from no extension. The same warning list carries dependency-cycle
metadata. Preflight peels acyclic dependencies from free destinations and marks only rows left in closed multi-file
cycles, so the frontend renders backend findings instead of re-deriving filename or graph semantics.
