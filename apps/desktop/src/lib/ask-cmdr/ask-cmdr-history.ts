/**
 * Folding a loaded conversation into rail items.
 *
 * A pure transform over the wire types, kept out of the state store beside it: nothing here
 * reads or writes `askCmdrState`, so history rendering is unit-testable on its own.
 */

import type { ConversationDetailView, MessageView } from '$lib/tauri-commands'
import type { RailMessage, RailToolCall } from './ask-cmdr-messages'

/** Fold a loaded conversation's messages into rail items: tool results are attached to the
 * assistant tool call they answer (by `callId`), so the thread shows one line per call. */
export function buildRailMessages(detail: ConversationDetailView): RailMessage[] {
  // A plain lookup (not a reactive SvelteMap): purely local to this pure transform.
  const resultOk: Record<string, boolean> = {}
  for (const message of detail.messages) {
    for (const block of message.blocks) {
      if (block.type === 'toolResult') resultOk[block.callId] = block.ok
    }
  }
  const out: RailMessage[] = []
  for (const message of detail.messages) {
    if (message.role === 'user') {
      // A wake opens its thread with a structured digest rather than typed prose, so the
      // user-role row can carry either. The digest wins when it's there: a wake never also
      // types something.
      const digest = message.blocks.find((b) => b.type === 'wakeDigest')
      if (digest) {
        out.push({ kind: 'wakeDigest', id: message.id, folders: digest.folders, rollups: digest.rollups })
      } else {
        out.push({ kind: 'user', id: message.id, text: joinText(message), attachments: [] })
      }
    } else if (message.role === 'assistant') {
      out.push({
        kind: 'assistant',
        id: message.id,
        text: joinText(message),
        tools: toolCallsOf(message, resultOk),
        thinking: false,
        streaming: false,
      })
    } else if (message.role === 'event') {
      for (const block of message.blocks) {
        if (block.type === 'modelChanged') out.push({ kind: 'modelChange', model: block.model })
      }
    }
    // `tool`-role messages carry only results, already folded into the tool lines above.
  }
  return out
}

function joinText(message: MessageView): string {
  return message.blocks
    .filter((b): b is Extract<typeof b, { type: 'text' }> => b.type === 'text')
    .map((b) => b.text)
    .join('')
}

function toolCallsOf(message: MessageView, resultOk: Record<string, boolean>): RailToolCall[] {
  return message.blocks
    .filter((b): b is Extract<typeof b, { type: 'toolCall' }> => b.type === 'toolCall')
    .map((b) => ({
      callId: b.callId,
      tool: b.tool,
      running: false,
      ok: resultOk[b.callId] ?? true,
      path: pathFromArguments(b.arguments),
    }))
}

/** Pull a `path` field out of a tool call's JSON arguments for the "looked at X" label. */
export function pathFromArguments(argumentsJson: string): string | null {
  try {
    const parsed = JSON.parse(argumentsJson) as unknown
    if (parsed && typeof parsed === 'object' && 'path' in parsed) {
      const path = parsed.path
      if (typeof path === 'string' && path.length > 0) return path
    }
  } catch {
    // Malformed arguments just yield no path suffix.
  }
  return null
}
