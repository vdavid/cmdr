/**
 * A new tab that says whether the pane took it.
 *
 * The backend's "Open in Cmdr" delivery opens several tabs in a row and has to stop at the
 * pane's limit, or the extra folders would overwrite the current tab. The frontend owns
 * that limit (`MAX_TABS_PER_PANE`), so it names the outcome on the request id and
 * `parse_tab_opened_response` (`src-tauri/src/tools/actions/app.rs`) reads it. ❌ Never a
 * sentence the backend would have to match on.
 */

import type { McpTabOpenArgs } from '#lib/commands/index.js'
import type { ExplorerAPI } from './explorer-api'

export async function openTabForMcp(request: {
  explorer: ExplorerAPI | undefined
  args: McpTabOpenArgs
}): Promise<void> {
  const { explorer, args } = request
  const body = explorer
    ? { ok: true, outcome: explorer.openTabInPane(args.pane) ? 'opened' : 'atLimit' }
    : { ok: false, error: 'Explorer is not ready' }
  const { emit } = await import('@tauri-apps/api/event')
  await emit('mcp-response', { requestId: args.mcpRequestId, ...body })
}
