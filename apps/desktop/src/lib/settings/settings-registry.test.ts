import { describe, it, expect } from 'vitest'
import {
  settingsRegistry,
  getSettingDefinition,
  getDefaultValue,
  getSettingsInSection,
  getAdvancedSettings,
  validateSettingValue,
  buildSectionTree,
} from './settings-registry'

describe('settingsRegistry', () => {
  it('should have at least one setting defined', () => {
    expect(settingsRegistry.length).toBeGreaterThan(0)
  })

  it('should have unique IDs for all settings', () => {
    const ids = settingsRegistry.map((s) => s.id)
    const uniqueIds = new Set(ids)
    expect(uniqueIds.size).toBe(ids.length)
  })

  it('should have non-empty sections for all settings', () => {
    for (const setting of settingsRegistry) {
      expect(setting.section.length).toBeGreaterThan(0)
      expect(setting.section[0]).toBeTruthy()
    }
  })
})

describe('getSettingDefinition', () => {
  it('should return definition for existing setting', () => {
    const def = getSettingDefinition('appearance.uiDensity')
    expect(def).toBeDefined()
    expect(def?.id).toBe('appearance.uiDensity')
    expect(def?.type).toBe('enum')
  })

  it('should return undefined for non-existent setting', () => {
    const def = getSettingDefinition('nonexistent.setting')
    expect(def).toBeUndefined()
  })
})

describe('getDefaultValue', () => {
  it('should return default value for settings', () => {
    const value = getDefaultValue('appearance.uiDensity')
    expect(value).toBe('comfortable')
  })

  it('should return correct defaults for boolean settings', () => {
    const value = getDefaultValue('updates.autoCheck')
    expect(value).toBe(true)
  })

  it('should return correct defaults for number settings', () => {
    const value = getDefaultValue('fileOperations.progressUpdateInterval')
    expect(typeof value).toBe('number')
  })
})

describe('getSettingsInSection', () => {
  it('should return settings in Appearance section', () => {
    const settings = getSettingsInSection(['Appearance'])
    expect(settings.length).toBeGreaterThan(0)
    for (const setting of settings) {
      expect(setting.section[0]).toBe('Appearance')
    }
  })

  it('should return settings in nested section', () => {
    const settings = getSettingsInSection(['Appearance', 'Colors and formats'])
    expect(settings.length).toBeGreaterThan(0)
    for (const setting of settings) {
      expect(setting.section).toEqual(['Appearance', 'Colors and formats'])
    }
  })

  it('should return empty array for non-existent section', () => {
    const settings = getSettingsInSection(['NonExistent'])
    expect(settings).toEqual([])
  })
})

describe('getAdvancedSettings', () => {
  it('should return settings marked as showInAdvanced', () => {
    const advanced = getAdvancedSettings()
    expect(advanced.length).toBeGreaterThan(0)
    for (const setting of advanced) {
      expect(setting.showInAdvanced).toBe(true)
    }
  })
})

describe('validateSettingValue', () => {
  it('should validate enum values', () => {
    // Valid
    expect(() => {
      validateSettingValue('appearance.uiDensity', 'compact')
    }).not.toThrow()
    expect(() => {
      validateSettingValue('appearance.uiDensity', 'comfortable')
    }).not.toThrow()
    expect(() => {
      validateSettingValue('appearance.uiDensity', 'spacious')
    }).not.toThrow()

    // Invalid
    expect(() => {
      validateSettingValue('appearance.uiDensity', 'invalid')
    }).toThrow()
  })

  it('should validate boolean values', () => {
    // Valid
    expect(() => {
      validateSettingValue('updates.autoCheck', true)
    }).not.toThrow()
    expect(() => {
      validateSettingValue('updates.autoCheck', false)
    }).not.toThrow()

    // Invalid
    expect(() => {
      validateSettingValue('updates.autoCheck', 'yes')
    }).toThrow()
  })

  it('should validate number values with constraints', () => {
    // Valid
    expect(() => {
      validateSettingValue('fileOperations.progressUpdateInterval', 100)
    }).not.toThrow()

    // Invalid - below min
    expect(() => {
      validateSettingValue('fileOperations.progressUpdateInterval', 0)
    }).toThrow()
  })
})

describe('search.recentSearches.maxCount', () => {
  it('registers the Advanced entry with the documented defaults and bounds', () => {
    const def = getSettingDefinition('search.recentSearches.maxCount')
    expect(def).toBeDefined()
    expect(def?.type).toBe('number')
    expect(def?.default).toBe(1000)
    expect(def?.showInAdvanced).toBe(true)
    expect(def?.component).toBe('number-input')
    expect(def?.constraints?.min).toBe(0)
    expect(def?.constraints?.max).toBe(10000)
  })

  it('mentions that 0 disables history so users find it via search', () => {
    const def = getSettingDefinition('search.recentSearches.maxCount')
    expect(def?.description).toContain('0 disables history')
  })

  it('validates the documented range', () => {
    expect(() => {
      validateSettingValue('search.recentSearches.maxCount', 0)
    }).not.toThrow()
    expect(() => {
      validateSettingValue('search.recentSearches.maxCount', 1000)
    }).not.toThrow()
    expect(() => {
      validateSettingValue('search.recentSearches.maxCount', 10_000)
    }).not.toThrow()
    expect(() => {
      validateSettingValue('search.recentSearches.maxCount', -1)
    }).toThrow()
    expect(() => {
      validateSettingValue('search.recentSearches.maxCount', 10_001)
    }).toThrow()
  })

  it('surfaces in the Advanced settings list', () => {
    const advanced = getAdvancedSettings()
    const ids = advanced.map((s) => s.id)
    expect(ids).toContain('search.recentSearches.maxCount')
  })
})

describe('buildSectionTree', () => {
  it('should build a tree from settings', () => {
    const tree = buildSectionTree()
    expect(Array.isArray(tree)).toBe(true)
    expect(tree.length).toBeGreaterThan(0)
  })

  it('should have Appearance section at top level', () => {
    const tree = buildSectionTree()
    const appearance = tree.find((s) => s.name === 'Appearance')
    expect(appearance).toBeDefined()
  })

  it('should have nested "Colors and formats" subsection under Appearance', () => {
    const tree = buildSectionTree()
    const appearance = tree.find((s) => s.name === 'Appearance')
    expect(appearance?.subsections.some((s) => s.name === 'Colors and formats')).toBe(true)
  })

  it('should have path arrays matching section hierarchy', () => {
    const tree = buildSectionTree()
    for (const section of tree) {
      expect(section.path).toEqual([section.name])
    }
  })
})
