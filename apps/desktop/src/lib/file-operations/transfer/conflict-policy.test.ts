/**
 * The MCP policy names, and what they mean to the transfer dialog.
 *
 * This map is half of a cross-language contract (the backend validates the name
 * in `mcp/executor/mod.rs::CONFLICT_POLICIES`), and the failure mode when the
 * halves drift is silent: an unknown name falls back to `skip`, so "ask me about
 * each file" becomes "skip every file" over somebody's data. It drifted once
 * already, in two private copies of this map that spelled the conditional
 * policies differently and were both unreachable.
 */

import { describe, it, expect } from 'vitest'
import { conflictPolicyFromMcpName } from './conflict-policy'

describe('conflictPolicyFromMcpName', () => {
  it('maps every name the backend accepts', () => {
    // Kept in step with `CONFLICT_POLICIES` in `mcp/executor/mod.rs`.
    expect(conflictPolicyFromMcpName('stop')).toBe('stop')
    expect(conflictPolicyFromMcpName('skip_all')).toBe('skip')
    expect(conflictPolicyFromMcpName('overwrite_all')).toBe('overwrite')
    expect(conflictPolicyFromMcpName('rename_all')).toBe('rename')
    expect(conflictPolicyFromMcpName('overwrite_smaller_all')).toBe('overwrite_smaller')
    expect(conflictPolicyFromMcpName('overwrite_older_all')).toBe('overwrite_older')
  })

  it('says it has never heard of a name rather than guessing one', () => {
    // The callers turn `undefined` into a deliberate default and log it. A map
    // that guessed would pick `skip` for a policy someone asked for by name.
    expect(conflictPolicyFromMcpName('overwrite_all_smaller')).toBeUndefined()
    expect(conflictPolicyFromMcpName('')).toBeUndefined()
    expect(conflictPolicyFromMcpName(undefined)).toBeUndefined()
  })
})
