/**
 * Accessibility audit for the Cmdr dual-pane explorer using axe-core.
 *
 * Proof-of-concept: injects axe-core into the real Tauri webview via
 * tauriPage.evaluate(), runs a WCAG audit, and fails on critical violations.
 */

import fs from 'fs'
import path from 'path'
import { test, expect } from './fixtures.js'
import { ensureAppReady } from './helpers.js'

/** Minimal type for the axe-core result shape we care about. */
interface AxeViolation {
    id: string
    impact: 'minor' | 'moderate' | 'serious' | 'critical'
    description: string
    helpUrl: string
    nodes: { html: string; failureSummary: string }[]
}
interface AxeResults {
    violations: AxeViolation[]
}

test('main explorer view has no critical accessibility violations', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)

    // Read and inject axe-core into the webview
    const axeSource = fs.readFileSync(
        path.resolve(__dirname, '../../node_modules/axe-core/axe.min.js'),
        'utf-8',
    )
    await tauriPage.evaluate(axeSource)

    // Run the accessibility audit
    const results = await tauriPage.evaluate<AxeResults>('axe.run()')

    // Separate violations by severity
    const critical = results.violations.filter((v) => v.impact === 'critical')
    const serious = results.violations.filter((v) => v.impact === 'serious')

    // Log serious and critical violations for visibility
    for (const v of [...critical, ...serious]) {
        console.log(
            `[axe/${v.impact}] ${v.id}: ${v.description}\n` +
                `  Help: ${v.helpUrl}\n` +
                v.nodes.map((n) => `  - ${n.html}\n    ${n.failureSummary}`).join('\n'),
        )
    }

    if (serious.length > 0) {
        console.log(`\n⚠ ${serious.length} serious violation(s) found (not failing the test)`)
    }

    // Fail only on critical violations
    expect(critical, `Found ${critical.length} critical accessibility violation(s)`).toHaveLength(0)
})
