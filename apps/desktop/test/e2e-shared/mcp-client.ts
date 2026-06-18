/**
 * Lightweight MCP client for E2E tests.
 *
 * Wraps `fetch()` calls to the Cmdr MCP server so test files can drive the
 * app via tool calls and resource reads without manual JSON-RPC boilerplate.
 */

import type { TauriPage, BrowserPageAdapter } from '@srsholmes/tauri-playwright'

type PageLike = TauriPage | BrowserPageAdapter

let mcpPort: number | null = null
let mcpToken: string | null = null

// ── Initialization ──────────────────────────────────────────────────────────

/** Discovers the actual MCP port and bearer token from the running app via Tauri IPC. */
export async function initMcpClient(tauriPage: PageLike): Promise<void> {
  // Port: prefer the env pin when the launcher set one (`CMDR_MCP_PORT`, e.g. the
  // i18n-capture orchestrator). The IPC `get_mcp_port` returns a `Promise<u16>`,
  // and some tauri-playwright eval channels resolve a Promise-returning eval to
  // the channel's truthy success flag (`true`) rather than the awaited value,
  // which then poisons the fetch URL (`http://localhost:true/mcp`). The env pin
  // sidesteps that entirely; the IPC stays the fallback for launchers that don't
  // pin a port.
  const envPort = process.env.CMDR_MCP_PORT
  const pinnedPort = envPort !== undefined && /^\d+$/.test(envPort) ? Number(envPort) : undefined
  mcpPort =
    pinnedPort ??
    (await tauriPage.evaluate<number>(
      `(async function() { return await window.__TAURI_INTERNALS__.invoke('get_mcp_port'); })()`,
    ))
  if (!mcpPort) throw new Error('MCP server not running: enable it in Settings > Developer')
  mcpToken = await tauriPage.evaluate<string>(
    `(async function() { return await window.__TAURI_INTERNALS__.invoke('get_mcp_token'); })()`,
  )
  if (!mcpToken) throw new Error('MCP server has no auth token: is it running?')
}

/** Authorization header for every authenticated `/mcp` request. */
function authHeaders(): Record<string, string> {
  if (!mcpToken) throw new Error('Call initMcpClient() first')
  return { 'Content-Type': 'application/json', Authorization: `Bearer ${mcpToken}` }
}

/** Idempotent init: calls `initMcpClient` only if the port hasn't been discovered yet. */
export async function ensureMcpClient(tauriPage: PageLike): Promise<void> {
  if (mcpPort) return
  await initMcpClient(tauriPage)
}

// ── Core JSON-RPC helpers ───────────────────────────────────────────────────

export async function mcpCall(tool: string, args: Record<string, unknown>): Promise<string> {
  if (!mcpPort) throw new Error('Call initMcpClient() first')
  const res = await fetch(`http://localhost:${String(mcpPort)}/mcp`, {
    method: 'POST',
    headers: authHeaders(),
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
    headers: authHeaders(),
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
  return mcpCall('await', { pane, condition: 'has_item', value: itemName, timeoutSeconds: timeoutS })
}

/** Waits for the pane path to contain a substring. */
export async function mcpAwaitPath(pane: 'left' | 'right', pathSubstring: string, timeoutS = 15): Promise<string> {
  return mcpCall('await', { pane, condition: 'path_contains', value: pathSubstring, timeoutSeconds: timeoutS })
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
