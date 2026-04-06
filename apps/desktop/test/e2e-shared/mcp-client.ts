/**
 * Lightweight MCP client for E2E tests.
 *
 * Wraps `fetch()` calls to the Cmdr MCP server so test files can drive the
 * app via tool calls and resource reads without manual JSON-RPC boilerplate.
 */

import type { TauriPage, BrowserPageAdapter } from '@srsholmes/tauri-playwright'

type PageLike = TauriPage | BrowserPageAdapter

let mcpPort: number | null = null

// ── Initialization ──────────────────────────────────────────────────────────

/** Discovers the actual MCP port from the running app via Tauri IPC. */
export async function initMcpClient(tauriPage: PageLike): Promise<void> {
  mcpPort = await tauriPage.evaluate<number>(`window.__TAURI_INTERNALS__.invoke('get_mcp_port')`)
  if (!mcpPort) throw new Error('MCP server not running — enable it in Settings > Developer')
}

// ── Core JSON-RPC helpers ───────────────────────────────────────────────────

export async function mcpCall(tool: string, args: Record<string, unknown>): Promise<string> {
  if (!mcpPort) throw new Error('Call initMcpClient() first')
  const res = await fetch(`http://localhost:${String(mcpPort)}/mcp`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      jsonrpc: '2.0',
      id: Date.now(),
      method: 'tools/call',
      params: { name: tool, arguments: args },
    }),
  })
  const json = (await res.json()) as { error?: { message: string }; result?: { content?: { text?: string }[] } }
  if (json.error) throw new Error(`MCP error: ${json.error.message}`)
  const text = json.result?.content?.[0]?.text
  if (!text) throw new Error(`Unexpected MCP response: ${JSON.stringify(json)}`)
  return text
}

export async function mcpReadResource(uri: string): Promise<string> {
  if (!mcpPort) throw new Error('Call initMcpClient() first')
  const res = await fetch(`http://localhost:${String(mcpPort)}/mcp`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      jsonrpc: '2.0',
      id: Date.now(),
      method: 'resources/read',
      params: { uri },
    }),
  })
  const json = (await res.json()) as { error?: { message: string }; result?: { contents?: { text?: string }[] } }
  if (json.error) throw new Error(`MCP error: ${json.error.message}`)
  return json.result?.contents?.[0]?.text ?? ''
}

// ── Convenience wrappers ────────────────────────────────────────────────────

/** Selects an MTP volume and waits for it to load. */
export async function mcpSelectVolume(pane: 'left' | 'right', name: string): Promise<string> {
  return mcpCall('select_volume', { pane, name })
}

/** Navigates a pane to an MTP path. */
export async function mcpNavToPath(pane: 'left' | 'right', path: string): Promise<string> {
  return mcpCall('nav_to_path', { pane, path })
}

/** Waits for an item to appear in a pane. */
export async function mcpAwaitItem(pane: 'left' | 'right', itemName: string, timeoutS = 15): Promise<string> {
  return mcpCall('await', { pane, condition: 'has_item', value: itemName, timeout_s: timeoutS })
}

/** Waits for the pane path to contain a substring. */
export async function mcpAwaitPath(pane: 'left' | 'right', pathSubstring: string, timeoutS = 15): Promise<string> {
  return mcpCall('await', { pane, condition: 'path_contains', value: pathSubstring, timeout_s: timeoutS })
}

/** Navigates to parent directory. */
export async function mcpNavToParent(): Promise<string> {
  return mcpCall('nav_to_parent', {})
}

/** Moves cursor to a file by name. */
export async function mcpMoveCursor(pane: 'left' | 'right', filename: string): Promise<string> {
  return mcpCall('move_cursor', { pane, filename })
}

/** Switches focus to the other pane. */
export async function mcpSwitchPane(): Promise<string> {
  return mcpCall('switch_pane', {})
}
