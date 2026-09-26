import { describe, expect, it } from 'vitest'
import { loadThirdPartyPackages } from './load-third-party-packages'

// The one place that loads the REAL generated list: every other licensing test stubs this
// module (see its header for why), so this pins the shape the dialog reads.
describe('loadThirdPartyPackages', () => {
  it('hands back all three attributed lists, each row named and licensed', async () => {
    const packages = await loadThirdPartyPackages()

    for (const list of [packages.vendored, packages.rust, packages.npm]) {
      expect(Array.isArray(list)).toBe(true)
      // `version` may be empty: vendored credits have none.
      for (const row of list) {
        expect(row.name).toBeTruthy()
        expect(row.license).toBeTruthy()
      }
    }
    expect(packages.rust.length).toBeGreaterThan(0)
    expect(packages.npm.length).toBeGreaterThan(0)
  })
})
