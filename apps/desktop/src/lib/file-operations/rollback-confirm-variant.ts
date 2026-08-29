/**
 * What rolling back will DO to the user's files, which is the only thing
 * `RollbackConfirmDialog`'s body has to get right.
 *
 * The three `undo*` values mirror the backend's `inverse_kind`
 * (`src-tauri/src/operation_log/rollback.rs`), the one place that decides whether an
 * inverse deletes, moves, or renames. Its own module, so the pure modules that pick a
 * variant can import the type without reaching into a `.svelte` file.
 */
export type RollbackConfirmVariant =
  /** A copy or move still running: stop it and delete what it has written. */
  | 'stopAndDelete'
  /** Undoing a finished copy, new file/folder, or compress: delete what it made. */
  | 'undoByDeleting'
  /** Undoing a finished move or trash: the files travel back, nothing is deleted. */
  | 'undoByMovingBack'
  /** Undoing a finished rename: the names change back, nothing moves. */
  | 'undoByRenamingBack'
