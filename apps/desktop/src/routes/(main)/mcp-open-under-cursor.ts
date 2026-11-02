/**
 * MCP `open_under_cursor`: the two round trips. One says what opening the cursor's entry would do, for
 * the tool's question (`tools/ask_plans.rs`). The other opens it, and is the backend's only honest
 * ack: a file handed to the OS default app produces no state push (`tools/actions/nav.rs`).
 */

import type { ToolOpenGate } from '#lib/file-explorer/pane/entry-activation.js'
import { noteToolNavigation } from '#lib/ui/tool-raised.js'
import type { McpListenerContext } from './mcp-listeners'

const navOpenUnderCursorCommand = 'nav.openUnderCursor'

function asRecord(payload: unknown): Record<string, unknown> {
  return payload && typeof payload === 'object' ? (payload as Record<string, unknown>) : {}
}

/**
 * What a tool's open must still be true of, off the `mcp-open-under-cursor` payload. No `gate` is a
 * client in direct mode. One this can't read launches nothing, since it isn't a yes.
 */
export function parseToolOpenGate(raw: Record<string, unknown>): ToolOpenGate | undefined {
  if (raw.gate === undefined) return undefined
  const { expectedPath } = asRecord(raw.gate)
  return typeof expectedPath === 'string' ? { expectedPath } : { mayLaunch: false }
}

export async function listenForOpenUnderCursor(ctx: McpListenerContext): Promise<void> {
  const { listenTauri, getExplorer, dispatch } = ctx

  await listenTauri('mcp-describe-open-under-cursor', (event) => {
    const raw = asRecord(event.payload)
    const requestId = typeof raw.requestId === 'string' ? raw.requestId : undefined
    if (requestId === undefined) return
    void (async () => {
      const { emit } = await import('@tauri-apps/api/event')
      const explorer = getExplorer()
      if (!explorer) {
        await emit('mcp-response', { requestId, ok: false, error: 'Explorer is not ready' })
        return
      }
      await emit('mcp-response', { requestId, ok: true, item: explorer.describeItemUnderCursor() })
    })()
  })

  await listenTauri('mcp-open-under-cursor', (event) => {
    const raw = asRecord(event.payload)
    const requestId = typeof raw.requestId === 'string' ? raw.requestId : undefined
    if (requestId === undefined) return
    noteToolNavigation()
    void (async () => {
      const { emit } = await import('@tauri-apps/api/event')
      const explorer = getExplorer()
      // HMR can land this with no explorer; reply ok:false rather than crashing.
      if (!explorer) {
        await emit('mcp-response', { requestId, ok: false, error: 'Explorer is not ready' })
        return
      }
      try {
        const gate = parseToolOpenGate(raw)
        if (gate === undefined) {
          await dispatch(navOpenUnderCursorCommand)
        } else {
          // Off the bus, which can't carry an answer back: this one says whether it opened at all.
          const answer = await explorer.openItemUnderCursorForTool(gate)
          if (answer === 'changedWhileAsking') {
            await emit('mcp-response', { requestId, ok: false, refusal: answer, error: 'It changed while asking' })
            return
          }
        }
        await emit('mcp-response', { requestId, ok: true })
      } catch (e) {
        const error = e instanceof Error ? e.message : String(e)
        await emit('mcp-response', { requestId, ok: false, error })
      }
    })()
  })
}
