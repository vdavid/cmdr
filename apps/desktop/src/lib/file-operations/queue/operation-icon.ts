import type { WriteOperationType } from '$lib/ipc/bindings'
import type { IconName } from '$lib/ui/icons/icon-map'

/**
 * The glyph for a queue row, by operation type. Explicit arms for every type:
 * the wire values are snake_case (`create_folder` / `create_file`), so a
 * camelCase typo would silently fall through to the `trash-2` default. The
 * instant metadata ops (rename / mkdir / mkfile) usually flash by too fast to
 * see, but a slow MTP one shows the right glyph.
 */
export function operationTypeIcon(operationType: WriteOperationType): IconName {
  switch (operationType) {
    case 'copy':
      return 'copy'
    case 'move':
      return 'folder-input'
    case 'rename':
      return 'pencil'
    case 'create_folder':
      return 'folder-plus'
    case 'create_file':
      return 'file-plus'
    case 'delete':
    case 'trash':
      return 'trash-2'
    // A zip edit (add/delete/rename inside, or copy/move into/out of a `.zip`).
    case 'archive_edit':
      return 'file-archive'
  }
}
