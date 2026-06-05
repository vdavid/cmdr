/**
 * Sort handlers: the focused-pane column setters (`sort.by*`), order setters
 * (ascending / descending / toggle), and the per-pane MCP `sort.set` (explicit
 * column + order on a specific pane).
 */
import type { CommandArgs } from '$lib/commands'
import type { CommandHandlerRecord } from './types'

export const sortHandlers = {
  'sort.byName': ({ explorerRef }) => {
    explorerRef?.setSortColumn('name')
  },

  'sort.byExtension': ({ explorerRef }) => {
    explorerRef?.setSortColumn('extension')
  },

  'sort.bySize': ({ explorerRef }) => {
    explorerRef?.setSortColumn('size')
  },

  'sort.byModified': ({ explorerRef }) => {
    explorerRef?.setSortColumn('modified')
  },

  'sort.byCreated': ({ explorerRef }) => {
    explorerRef?.setSortColumn('created')
  },

  'sort.ascending': ({ explorerRef }) => {
    explorerRef?.setSortOrder('asc')
  },

  'sort.descending': ({ explorerRef }) => {
    explorerRef?.setSortOrder('desc')
  },

  'sort.toggleOrder': ({ explorerRef }) => {
    explorerRef?.setSortOrder('toggle')
  },

  'sort.set': ({ explorerRef, dispatchArgs }) => {
    // Per-pane sort from the MCP `sort` tool: an explicit column + order on a
    // SPECIFIC pane (the `sort.by*` / `sort.ascending` commands act on the
    // focused pane only).
    const { pane, column, order } = dispatchArgs as CommandArgs['sort.set']
    void explorerRef?.setSort(column, order, pane)
  },
} satisfies Partial<CommandHandlerRecord>
