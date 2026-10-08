/**
 * MCP `tab move`: move a tab to another slot or to the other pane, then tell the agent
 * what the move did.
 *
 * The other `tab` actions are fire-and-forget behind a generation ack. A move isn't,
 * because it can be REFUSED (a pinned tab, a pane's only tab, a full pane), and the
 * frontend owns those rules: `moveTab` in `$lib/file-explorer/tabs/tab-state-manager.svelte`
 * is the one place they live, shared with the mouse. So the reply is a round-trip on the
 * request id carrying a typed `outcome`, which `parse_tab_move_response`
 * (`src-tauri/src/mcp/executor/app.rs`) reads. ❌ Never a sentence the backend would have
 * to match on.
 *
 * A move that went through replies only after both panes' tab lists reached the backend,
 * so a `cmdr://state` read right after the tool returns shows the tab where the reply
 * says it is.
 */

import type { McpTabMoveArgs } from '$lib/commands'
import type { ExplorerAPI } from './explorer-api'

export async function moveTabForMcp(request: {
  explorer: ExplorerAPI | undefined
  args: McpTabMoveArgs
}): Promise<void> {
  const { explorer, args } = request
  const reply = async (body: Record<string, unknown>): Promise<void> => {
    if (args.mcpRequestId === undefined) return
    const { emit } = await import('@tauri-apps/api/event')
    await emit('mcp-response', { requestId: args.mcpRequestId, ...body })
  }

  if (!explorer) {
    await reply({ ok: false, error: 'Explorer is not ready' })
    return
  }

  const result = explorer.moveTab({
    fromPane: args.pane,
    tabId: args.tabId,
    toPane: args.toPane,
    toIndex: args.toIndex,
  })
  if (result.moved) {
    await explorer.syncTabsToMcp()
    await reply({ ok: true, outcome: 'moved', toIndex: result.toIndex })
    return
  }
  await reply({ ok: result.reason === 'unchanged', outcome: result.reason })
}
