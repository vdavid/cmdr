import { describe, expect, it } from 'vitest'
import { CONSENT_COPY_VERSION, SHOTS_THREAD, buildThreadSql } from './marketing-shots-thread.ts'

const AT = 1_754_000_000

describe('SHOTS_THREAD', () => {
  it('only uses roles the app knows how to render', () => {
    for (const message of SHOTS_THREAD.messages) {
      expect(['user', 'assistant', 'tool']).toContain(message.role)
    }
  })

  it('carries valid AgentPart JSON in every message', () => {
    for (const message of SHOTS_THREAD.messages) {
      const parts: unknown = JSON.parse(message.contentBlocks)
      expect(Array.isArray(parts)).toBe(true)
      for (const part of parts as Record<string, unknown>[]) {
        // Externally tagged, one tag per part: `{text}` / `{tool_call}` / `{tool_result}`.
        expect(Object.keys(part)).toHaveLength(1)
        expect(['text', 'tool_call', 'tool_result']).toContain(Object.keys(part)[0])
      }
    }
  })

  it('answers every tool call it makes, in order', () => {
    // A dangling call renders as a tool row that never resolves, which in a marketing
    // shot reads as the feature being broken.
    const calls: string[] = []
    const results: string[] = []
    for (const message of SHOTS_THREAD.messages) {
      for (const part of JSON.parse(message.contentBlocks) as Record<string, { call_id?: string }>[]) {
        if ('tool_call' in part) calls.push(part.tool_call.call_id ?? '')
        if ('tool_result' in part) results.push(part.tool_result.call_id ?? '')
      }
    }
    expect(calls.length).toBeGreaterThan(0)
    expect(results).toEqual(calls)
  })

  it('gives the search index the prose, not just the blocks', () => {
    // The FTS triggers copy `text_for_search` verbatim, so an empty one produces a
    // thread the app's own search cannot find. Invisible in the shot, wrong in the file.
    for (const message of SHOTS_THREAD.messages) {
      if (message.role === 'tool') continue
      expect(message.textForSearch.length).toBeGreaterThan(0)
    }
  })
})

describe('buildThreadSql', () => {
  const sql = buildThreadSql(AT)

  it('accepts the consent the rail checks before it renders anything', () => {
    expect(sql).toContain(`'ask_cmdr_consent_version','${String(CONSENT_COPY_VERSION)}'`)
    expect(sql).toContain("'ask_cmdr_consent_at'")
  })

  it('numbers messages from zero with no gaps, which the unique index demands', () => {
    const seqs = [...sql.matchAll(/INSERT INTO messages \(conversation_id, seq,/g)]
    expect(seqs).toHaveLength(SHOTS_THREAD.messages.length)
    for (const [index] of SHOTS_THREAD.messages.entries()) {
      expect(sql).toContain(`, ${String(index)}, '`)
    }
  })

  it('replaces its own previous thread rather than stacking a new one every run', () => {
    // Idempotence is what lets the seed run on every launch: without the delete, a
    // week of runs leaves a sidebar full of identical conversations.
    expect(sql).toContain('DELETE FROM conversations')
    expect(sql).toContain(SHOTS_THREAD.title)
  })

  it('escapes quotes in the copy instead of ending the string early', () => {
    const quoted = buildThreadSql(AT, { title: "David's stuff", messages: SHOTS_THREAD.messages })
    expect(quoted).toContain("David''s stuff")
  })

  it('leaves the thread newest, so the rail bootstraps onto it', () => {
    expect(sql).toContain(String(AT))
    expect(sql).toContain('archived')
  })
})
