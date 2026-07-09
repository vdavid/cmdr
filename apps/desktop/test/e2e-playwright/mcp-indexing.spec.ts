/**
 * E2E for the MCP indexing surface (M1): the cross-layer wiring an agent uses to
 * inspect and control drive indexing through the real MCP server.
 *
 *   1. cmdr://indexing renders per-volume (one block per known volume, with
 *      freshness + phase), and the local `root` volume shows up.
 *   2. The `indexing` tool rescans `root` and returns OK. Per the ordering
 *      contract, it returns only after the scan has left its pre-scan state, so
 *      a following `await index_status … fresh` can't match the pre-rescan
 *      state — it genuinely waits for the fresh rescan to complete.
 *   3. `await` with condition `index_status` (value `fresh`) resolves against
 *      the live freshness store once the scan finishes, and the resource re-read
 *      confirms fresh.
 *
 * The E2E `root` index covers only the small fixture tree, so a rescan finishes
 * in well under the await deadline — `fresh` is the deterministic end state. The
 * fresh / scanning / stale matching itself is covered exhaustively by the Rust
 * unit tests (`index_status_matches`, incl. an MTP colon-id case); this spec
 * pins the wiring: resource builder → tool → freshness store → await condition.
 */

import { test, expect } from './fixtures.js'
import { ensureAppReady } from './helpers.js'
import { ensureMcpClient, mcpCall, mcpReadResource } from '../e2e-shared/mcp-client.js'

test.describe('MCP indexing surface', () => {
  test('read per-volume status, rescan root, await index_status fresh', async ({ tauriPage }) => {
    test.setTimeout(60_000)
    await ensureAppReady(tauriPage)
    await ensureMcpClient(tauriPage)

    // 1. Per-volume resource: root should be a known indexed volume.
    const before = await mcpReadResource('cmdr://indexing')
    expect(before).toContain('root (local):')

    // 2. Rescan root. The ordering contract guarantees the tool returns only
    //    after freshness has left its pre-scan state, so the await below is
    //    sequenced after a real rescan (not the pre-rescan fresh state).
    const rescan = await mcpCall('indexing', { action: 'rescan', volumeId: 'root' })
    expect(rescan).toContain('OK')
    expect(rescan).toContain('root')

    // 3. await index_status resolves against the live freshness store once the
    //    (fixture-sized) rescan completes.
    const awaited = await mcpCall('await', {
      condition: 'index_status',
      volumeId: 'root',
      value: 'fresh',
      timeoutSeconds: 30,
    })
    expect(awaited).toContain('OK')
    expect(awaited).toContain('fresh')

    // The resource re-read reflects the same live status.
    const after = await mcpReadResource('cmdr://indexing')
    expect(after).toContain('root (local):')
    expect(after).toContain('status: fresh')
  })
})
