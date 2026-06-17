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
import { searchSettings, clearSearchIndex } from './settings-search'

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
  it('should return the settings whose single home is Advanced', () => {
    const advanced = getAdvancedSettings()
    expect(advanced.length).toBeGreaterThan(0)
    for (const setting of advanced) {
      expect(setting.section[0]).toBe('Advanced')
      expect(setting.hidden).not.toBe(true)
    }
  })

  it('should include the repointed former mirrors and exclude them from their old feature pages', () => {
    const advancedIds = new Set(getAdvancedSettings().map((s) => s.id))
    // The three former mirrors now live only in Advanced.
    expect(advancedIds.has('network.smbConcurrency')).toBe(true)
    expect(advancedIds.has('fileOperations.maxConflictsToShow')).toBe(true)
    expect(advancedIds.has('fileOperations.progressUpdateInterval')).toBe(true)
    // And the two Recent* caps.
    expect(advancedIds.has('search.recentSearches.maxCount')).toBe(true)
    expect(advancedIds.has('selection.recentSelections.maxCount')).toBe(true)

    // None of them is reachable through the nav tree (Advanced auto-renders instead).
    expect(getSettingDefinition('network.smbConcurrency')?.section).toEqual(['Advanced'])
    expect(getSettingDefinition('fileOperations.maxConflictsToShow')?.section).toEqual(['Advanced'])
    expect(getSettingDefinition('fileOperations.progressUpdateInterval')?.section).toEqual(['Advanced'])
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

describe('search.autoApply', () => {
  it('registers a boolean setting in Behavior > Search, defaulting to on', () => {
    const def = getSettingDefinition('search.autoApply')
    expect(def).toBeDefined()
    expect(def?.type).toBe('boolean')
    expect(def?.default).toBe(true)
    expect(def?.section).toEqual(['Behavior', 'Search'])
    expect(def?.component).toBe('switch')
  })

  it('is reachable via canonical search keywords', () => {
    const def = getSettingDefinition('search.autoApply')
    const keywords = def?.keywords ?? []
    // The plan's keyword set covers the natural lookup terms (search, auto, apply, debounce,
    // filename, regex). One assertion per term so a typo failure points right at the gap.
    for (const term of ['search', 'auto', 'apply', 'debounce', 'filename', 'regex']) {
      expect(keywords).toContain(term)
    }
  })

  it('validates booleans only', () => {
    expect(() => {
      validateSettingValue('search.autoApply', true)
    }).not.toThrow()
    expect(() => {
      validateSettingValue('search.autoApply', false)
    }).not.toThrow()
    expect(() => {
      validateSettingValue('search.autoApply', 'yes')
    }).toThrow()
  })

  it('lives under Behavior > Search in the section tree', () => {
    const tree = buildSectionTree()
    const behavior = tree.find((s) => s.name === 'Behavior')
    expect(behavior).toBeDefined()
    const search = behavior?.subsections.find((s) => s.name === 'Search')
    expect(search).toBeDefined()
    const ids = (search?.settings ?? []).map((s) => s.id)
    expect(ids).toContain('search.autoApply')
  })
})

describe('search.recentSearches.maxCount', () => {
  it('registers the Advanced entry with the documented defaults and bounds', () => {
    const def = getSettingDefinition('search.recentSearches.maxCount')
    expect(def).toBeDefined()
    expect(def?.type).toBe('number')
    expect(def?.default).toBe(1000)
    expect(def?.section).toEqual(['Advanced'])
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

describe('appearance.showFunctionKeyBar', () => {
  it('registers a boolean switch in Appearance > Listing, defaulting to on (current behavior)', () => {
    const def = getSettingDefinition('appearance.showFunctionKeyBar')
    expect(def).toBeDefined()
    expect(def?.type).toBe('boolean')
    expect(def?.component).toBe('switch')
    expect(getDefaultValue('appearance.showFunctionKeyBar')).toBe(true)
    expect(def?.section).toEqual(['Appearance', 'Listing'])
  })

  it('is reachable via natural lookup keywords', () => {
    const keywords = getSettingDefinition('appearance.showFunctionKeyBar')?.keywords ?? []
    for (const term of ['function', 'key', 'bar', 'f-key', 'shortcut']) {
      expect(keywords).toContain(term)
    }
  })

  it('validates booleans only', () => {
    expect(() => {
      validateSettingValue('appearance.showFunctionKeyBar', false)
    }).not.toThrow()
    expect(() => {
      validateSettingValue('appearance.showFunctionKeyBar', 'yes')
    }).toThrow()
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

  it('includes Advanced as a top-level node holding its settings (no longer skipped)', () => {
    const tree = buildSectionTree()
    const advanced = tree.find((s) => s.name === 'Advanced')
    expect(advanced).toBeDefined()
    expect(advanced?.path).toEqual(['Advanced'])
    expect(advanced?.subsections).toHaveLength(0)
    const ids = advanced?.settings.map((s) => s.id) ?? []
    expect(ids).toContain('network.smbConcurrency')
    expect(ids).toContain('advanced.prefetchBufferSize')
  })

  it('gives every setting exactly one home: Advanced auto-render XOR a non-Advanced tree node', () => {
    // The M8 invariant. `getAdvancedSettings()` is what AdvancedSection auto-renders;
    // the tree (sans Advanced) is what feature pages hand-render. No id may be in both,
    // and together (plus hidden) they account for the whole registry.
    const advancedIds = new Set(getAdvancedSettings().map((s) => s.id))

    function collect(nodes: ReturnType<typeof buildSectionTree>, acc: Set<string>): void {
      for (const node of nodes) {
        for (const s of node.settings) acc.add(s.id)
        collect(node.subsections, acc)
      }
    }
    const treeIds = new Set<string>()
    collect(buildSectionTree(), treeIds)

    // The Advanced tree node's settings ARE the Advanced auto-rendered set; remove them
    // so `treeIds` is the feature-page (hand-rendered) home set.
    for (const id of advancedIds) treeIds.delete(id)

    // No setting is both Advanced-homed and feature-page-homed.
    for (const id of advancedIds) {
      expect(treeIds.has(id), `${id} is in both Advanced and a feature page`).toBe(false)
    }

    // Every non-hidden setting has exactly one home (Advanced or a feature page).
    for (const s of settingsRegistry) {
      if (s.hidden) continue
      const homes = (advancedIds.has(s.id) ? 1 : 0) + (treeIds.has(s.id) ? 1 : 0)
      expect(homes, `${s.id} should have exactly one home`).toBe(1)
    }
  })
})

describe('updates.attachEmailToReports', () => {
  it('registers a boolean defaulting to false (not pre-ticked) in Advanced', () => {
    const def = getSettingDefinition('updates.attachEmailToReports')
    expect(def).toBeDefined()
    expect(def?.type).toBe('boolean')
    expect(def?.component).toBe('switch')
    expect(getDefaultValue('updates.attachEmailToReports')).toBe(false)
    expect(def?.section).toEqual(['Advanced'])
  })

  it('surfaces in the Advanced settings list', () => {
    const ids = getAdvancedSettings().map((s) => s.id)
    expect(ids).toContain('updates.attachEmailToReports')
  })

  it('validates booleans only', () => {
    expect(() => {
      validateSettingValue('updates.attachEmailToReports', true)
    }).not.toThrow()
    expect(() => {
      validateSettingValue('updates.attachEmailToReports', 'yes')
    }).toThrow()
  })
})

describe('analytics settings', () => {
  it('analytics.enabled is a boolean switch defaulting to on (opt-out for the beta)', () => {
    const def = getSettingDefinition('analytics.enabled')
    expect(def?.type).toBe('boolean')
    expect(def?.component).toBe('switch')
    expect(getDefaultValue('analytics.enabled')).toBe(true)
  })

  it('analytics.email is a string text input defaulting to empty', () => {
    const def = getSettingDefinition('analytics.email')
    expect(def?.type).toBe('string')
    expect(def?.component).toBe('text-input')
    expect(getDefaultValue('analytics.email')).toBe('')
  })

  it('both analytics settings live under the renamed "Updates & privacy" section', () => {
    expect(getSettingDefinition('analytics.enabled')?.section).toEqual(['Updates & privacy'])
    expect(getSettingDefinition('analytics.email')?.section).toEqual(['Updates & privacy'])
    // The pre-existing Updates settings moved into the renamed section too.
    expect(getSettingDefinition('updates.autoCheck')?.section).toEqual(['Updates & privacy'])
  })
})

describe('cardKey resolution (resolveDefinition)', () => {
  it('resolves a setting`s cardKey to a `card` getter (the in-page SectionCard title)', () => {
    // The downloads setting carries cardKey `settings.fileSystemWatching.cardDownloads`.
    const def = getSettingDefinition('behavior.fileSystemWatching.downloadsNotifications')
    expect(def?.card).toBe('Downloads')
  })

  it('leaves `card` undefined when a setting has no cardKey', () => {
    // A setting on a single-card page (one unlabeled card), so it carries no `cardKey`.
    const def = getSettingDefinition('appearance.uiDensity')
    expect(def?.card).toBeUndefined()
  })
})

describe('indexing.indexSize hidden search anchor', () => {
  it('is a fully-modeled, hidden boolean under the File system watching page', () => {
    const def = getSettingDefinition('indexing.indexSize')
    expect(def).toBeDefined()
    expect(def?.hidden).toBe(true)
    expect(def?.type).toBe('boolean')
    expect(getDefaultValue('indexing.indexSize')).toBe(false)
    // Guardrail: section MUST equal the hosting page`s, or the blank-page fix breaks.
    expect(def?.section).toEqual(['Behavior', 'File system watching'])
    expect(def?.component).toBeUndefined()
  })

  it('is excluded from the nav section tree (it is hidden)', () => {
    const tree = buildSectionTree()
    const behavior = tree.find((s) => s.name === 'Behavior')
    const fsw = behavior?.subsections.find((s) => s.name === 'File system watching')
    expect(fsw).toBeDefined()
    expect(fsw?.settings.some((s) => s.id === 'indexing.indexSize')).toBe(false)
  })

  it('is included in the search index (the whole registry is indexed; hidden is searchable)', () => {
    clearSearchIndex()
    const ids = searchSettings('').map((r) => r.setting.id)
    expect(ids).toContain('indexing.indexSize')
  })
})

describe('appearance.language (the UI language picker)', () => {
  it('is a persisted enum that defaults to System default', () => {
    const def = getSettingDefinition('appearance.language')
    expect(def?.type).toBe('enum')
    expect(def?.component).toBe('select')
    expect(getDefaultValue('appearance.language')).toBe('system')
    expect(def?.section).toEqual(['Appearance', 'Colors and formats'])
  })

  it('offers a System default option plus every available locale (en at least)', () => {
    const options = getSettingDefinition('appearance.language')?.constraints?.options ?? []
    const values = options.map((o) => o.value)
    // The System default sentinel comes first.
    expect(values[0]).toBe('system')
    // The base locale is always loaded, so it must be selectable.
    expect(values).toContain('en')
    // The non-locale screenshots dir must never leak in as an option.
    expect(values).not.toContain('screenshots')
  })

  it('labels the `en` option with the locale endonym (English), not the raw tag', () => {
    const options = getSettingDefinition('appearance.language')?.constraints?.options ?? []
    const en = options.find((o) => o.value === 'en')
    expect(en?.label).toBe('English')
  })

  it('validates: accepts `system` and a loaded locale, rejects an unknown tag', () => {
    expect(() => {
      validateSettingValue('appearance.language', 'system')
    }).not.toThrow()
    expect(() => {
      validateSettingValue('appearance.language', 'en')
    }).not.toThrow()
    expect(() => {
      validateSettingValue('appearance.language', 'zz-NOPE')
    }).toThrow()
  })
})
