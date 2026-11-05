import { describe, expect, it } from 'vitest'
import { appAnswers, rawAnswers, silentCommands } from './answers.js'
import { pluginHandler } from './plugins.js'
import { volumeAnswers } from './volumes.js'

/**
 * The commands the WASM engine answers (`crates/cmdr-listing-wasm/src/operations_host/commands/`,
 * `folder_sizes.rs`, and `search.rs`).
 * The backend tries its TS tables first, so a TS answer for one of these would silently shadow the
 * real write engine, its journal, or its undo planner.
 */
const engineCommands = [
  'copy_between_volumes',
  'move_between_volumes',
  'move_files',
  'delete_files',
  'trash_files',
  'start_scan_preview',
  'scan_volume_for_conflicts',
  'destination_write_access',
  'resolve_write_conflict',
  'get_volume_space',
  'create_directory',
  'create_file',
  'rename_file',
  'check_rename_permission',
  'move_to_trash',
  'list_operations',
  'latest_operation_progress',
  'get_transfer_tree_view',
  'pause_operation',
  'cancel_operation',
  'path_exists',
  'destination_exists',
  'destination_root_echo',
  'stat_paths_kinds',
  'undo_operations',
  'rollback_operation',
  'get_recent_operation_log_entries',
  'get_operation_log_detail',
  // The folder sizes, off the engine's fold over the volume.
  'get_dir_stats',
  'get_dir_stats_batch',
  // ⌘F, the search core over the same volume (`src/search.rs`).
  'prepare_search_index',
  'search_files',
  'search_files_streaming',
  'cancel_search',
  'release_search_index',
  'parse_search_scope',
  'get_system_dir_excludes',
  'sort_search_results',
  'get_recent_searches',
  'add_recent_search',
  'remove_recent_search',
  'clear_recent_searches',
  'apply_recent_searches_max_count',
]

const camelCase = (snake: string) => snake.replace(/_([a-z0-9])/g, (_, c: string) => c.toUpperCase())

describe('the engine-owned commands', () => {
  it('have no TS answer to shadow the engine', () => {
    const tables = [appAnswers, volumeAnswers] as Record<string, unknown>[]
    for (const command of engineCommands) {
      const name = camelCase(command)
      expect(
        tables.some((table) => name in table),
        name,
      ).toBe(false)
      expect(rawAnswers[command], command).toBeUndefined()
      expect((silentCommands as readonly string[]).includes(name), name).toBe(false)
      expect(pluginHandler(command), command).toBeUndefined()
    }
  })
})
