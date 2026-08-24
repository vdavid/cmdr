import { describe, expect, it } from 'vitest'
import {
  decodeValue,
  defaultsForVersion,
  labelValue,
  resolveAdoption,
  type ConfigShapeInstallRow,
  type ConfigShapeValueRow,
  type SettingValue,
} from './settings-defaults.js'

/**
 * A stand-in manifest: `askCmdr.proactive` arrives in 0.40.0, and `indexing.enabled` flips its
 * default there. Both are the real shapes this resolution exists to get right.
 */
const versions: Record<string, Record<string, SettingValue>> = {
  '0.39.0': { 'indexing.enabled': true, 'theme.mode': 'system' },
  '0.40.0': { 'indexing.enabled': false, 'theme.mode': 'system', 'askCmdr.proactive': true },
}

function installs(rows: Record<string, number>): ConfigShapeInstallRow[] {
  return Object.entries(rows).map(([appVersion, count]) => ({ appVersion, installs: count }))
}

function value(appVersion: string, key: string, raw: SettingValue, count: number): ConfigShapeValueRow {
  const type = raw === true ? 'true' : raw === false ? 'false' : typeof raw === 'number' ? 'integer' : 'text'
  return { appVersion, key, type, value: raw === true ? 1 : raw === false ? 0 : raw, installs: count }
}

function adoptionFor(key: string, result: ReturnType<typeof resolveAdoption>) {
  const setting = result.settings.find((s) => s.key === key)
  if (setting === undefined) throw new Error(`no adoption for ${key}`)
  return setting
}

describe('defaultsForVersion', () => {
  it('takes the newest entry at or below the version', () => {
    expect(defaultsForVersion('0.39.5', versions)).toBe(versions['0.39.0'])
    expect(defaultsForVersion('0.41.0', versions)).toBe(versions['0.40.0'])
  })

  it('answers null below the first entry, rather than guessing with the oldest one', () => {
    expect(defaultsForVersion('0.30.0', versions)).toBeNull()
  })
})

describe('decodeValue', () => {
  it('rebuilds a boolean D1 flattened to 1 / 0', () => {
    expect(decodeValue({ type: 'true', value: 1 })).toBe(true)
    expect(decodeValue({ type: 'false', value: 0 })).toBe(false)
  })

  it('keeps numbers numeric and text textual', () => {
    expect(decodeValue({ type: 'integer', value: 125 })).toBe(125)
    expect(decodeValue({ type: 'real', value: 0.5 })).toBe(0.5)
    expect(decodeValue({ type: 'text', value: 'dark' })).toBe('dark')
  })

  it('refuses a shape the config shape can never carry', () => {
    expect(decodeValue({ type: 'object', value: '{}' })).toBeNull()
  })
})

describe('resolveAdoption', () => {
  it('counts an install that reported nothing as running the default', () => {
    const result = resolveAdoption(installs({ '0.39.0': 10 }), [], versions)
    const indexing = adoptionFor('indexing.enabled', result)
    expect(indexing.eligible).toBe(10)
    expect(indexing.onDefault).toBe(10)
    expect(indexing.values).toEqual([{ label: 'on', installs: 10, isDefault: true }])
  })

  it('resolves effective value as override over default', () => {
    const result = resolveAdoption(
      installs({ '0.39.0': 10 }),
      [value('0.39.0', 'indexing.enabled', false, 3)],
      versions,
    )
    const indexing = adoptionFor('indexing.enabled', result)
    expect(indexing.values).toEqual([
      { label: 'on', installs: 7, isDefault: true },
      { label: 'off', installs: 3, isDefault: false },
    ])
    expect(indexing.onDefault).toBe(7)
  })

  it('drops installs whose build did not have the setting yet', () => {
    // This is the whole point. `askCmdr.proactive` arrives in 0.40.0, so the 100 installs on 0.39.0
    // have no opinion about it and must not be scored as "on the default".
    const result = resolveAdoption(installs({ '0.39.0': 100, '0.40.0': 10 }), [], versions)
    const askCmdr = adoptionFor('askCmdr.proactive', result)
    expect(askCmdr.eligible).toBe(10)
    expect(askCmdr.values).toEqual([{ label: 'on', installs: 10, isDefault: true }])
    // Meanwhile a setting that existed all along keeps the whole fleet.
    expect(adoptionFor('theme.mode', result).eligible).toBe(110)
  })

  it('ignores a stale value for a setting the build no longer has', () => {
    // An old key can linger in someone's settings.json long after the release that read it. It's
    // inert in that build, so counting it would invent a user of a setting that isn't there.
    const result = resolveAdoption(
      installs({ '0.40.0': 10 }),
      [value('0.40.0', 'listing.humanFriendlySizeUnits', true, 4)],
      versions,
    )
    expect(result.settings.some((s) => s.key === 'listing.humanFriendlySizeUnits')).toBe(false)
  })

  it('scores each version against its OWN default when the default moved', () => {
    // `indexing.enabled` defaults on in 0.39.0 and off in 0.40.0. Nobody set anything, so all ten
    // installs are on their own version's default and none is a deviation. Deriving that from the
    // value buckets instead would rank this setting as "everybody changed it".
    const result = resolveAdoption(installs({ '0.39.0': 6, '0.40.0': 4 }), [], versions)
    const indexing = adoptionFor('indexing.enabled', result)
    expect(indexing.onDefault).toBe(10)
    // No single default to show, so the breakdown says so rather than picking one release's answer.
    expect(indexing.defaultLabel).toBeNull()
    expect(indexing.values).toEqual([
      { label: 'on', installs: 6, isDefault: false },
      { label: 'off', installs: 4, isDefault: false },
    ])
  })

  it('counts an override that matches a moved default as being on the default', () => {
    const result = resolveAdoption(
      installs({ '0.39.0': 6, '0.40.0': 4 }),
      [value('0.40.0', 'indexing.enabled', false, 2)],
      versions,
    )
    // All four 0.40.0 installs run `off`, which IS 0.40.0's default, whether they set it or not.
    expect(adoptionFor('indexing.enabled', result).onDefault).toBe(10)
  })

  it('reports installs it cannot resolve instead of folding them into a denominator', () => {
    const result = resolveAdoption(installs({ '0.30.0': 25, '0.40.0': 5 }), [], versions)
    expect(result.totalInstalls).toBe(30)
    expect(result.unresolvedInstalls).toBe(25)
    expect(adoptionFor('theme.mode', result).eligible).toBe(5)
  })

  it('merges the same effective value across versions', () => {
    const result = resolveAdoption(
      installs({ '0.39.0': 5, '0.40.0': 5 }),
      [value('0.40.0', 'theme.mode', 'dark', 2)],
      versions,
    )
    expect(adoptionFor('theme.mode', result).values).toEqual([
      { label: 'system', installs: 8, isDefault: true },
      { label: 'dark', installs: 2, isDefault: false },
    ])
  })

  it('counts a deliberate choice that equals the default as being on the default', () => {
    // Persistence is structural, so someone who toggled a setting back reports it explicitly. They
    // are running the default, and the effective value is what adoption asks about.
    const result = resolveAdoption(installs({ '0.39.0': 10 }), [value('0.39.0', 'indexing.enabled', true, 4)], versions)
    expect(adoptionFor('indexing.enabled', result).onDefault).toBe(10)
  })

  it('never produces a negative count when value rows outrun their version total', () => {
    const result = resolveAdoption(installs({ '0.39.0': 2 }), [value('0.39.0', 'theme.mode', 'dark', 5)], versions)
    expect(adoptionFor('theme.mode', result).values).toEqual([{ label: 'dark', installs: 5, isDefault: false }])
  })
})

describe('labelValue', () => {
  it('reads booleans as on and off', () => {
    expect(labelValue(true)).toBe('on')
    expect(labelValue(false)).toBe('off')
    expect(labelValue(125)).toBe('125')
    expect(labelValue('dark')).toBe('dark')
  })
})
