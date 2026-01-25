import { describe, it, expect, beforeEach } from 'vitest'
import {
    settingsRegistry,
    getSettingDefinition,
    getDefaultValue,
    getSettingsInSection,
    getAdvancedSettings,
    validateSettingValue,
    buildSectionTree,
    type SettingsSection,
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
        // @ts-expect-error - testing invalid input
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
        const value = getDefaultValue('fileOperations.confirmBeforeDelete')
        expect(value).toBe(true)
    })

    it('should return correct defaults for number settings', () => {
        const value = getDefaultValue('fileOperations.progressUpdateInterval')
        expect(typeof value).toBe('number')
    })
})

describe('getSettingsInSection', () => {
    it('should return settings in General section', () => {
        const settings = getSettingsInSection(['General'])
        expect(settings.length).toBeGreaterThan(0)
        for (const setting of settings) {
            expect(setting.section[0]).toBe('General')
        }
    })

    it('should return settings in nested section', () => {
        const settings = getSettingsInSection(['General', 'Appearance'])
        expect(settings.length).toBeGreaterThan(0)
        for (const setting of settings) {
            expect(setting.section).toEqual(['General', 'Appearance'])
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
        expect(() => validateSettingValue('appearance.uiDensity', 'compact')).not.toThrow()
        expect(() => validateSettingValue('appearance.uiDensity', 'comfortable')).not.toThrow()
        expect(() => validateSettingValue('appearance.uiDensity', 'spacious')).not.toThrow()

        // Invalid
        expect(() => validateSettingValue('appearance.uiDensity', 'invalid')).toThrow()
    })

    it('should validate boolean values', () => {
        // Valid
        expect(() => validateSettingValue('fileOperations.confirmBeforeDelete', true)).not.toThrow()
        expect(() => validateSettingValue('fileOperations.confirmBeforeDelete', false)).not.toThrow()

        // Invalid
        expect(() => validateSettingValue('fileOperations.confirmBeforeDelete', 'yes')).toThrow()
    })

    it('should validate number values with constraints', () => {
        // Valid
        expect(() => validateSettingValue('fileOperations.progressUpdateInterval', 100)).not.toThrow()

        // Invalid - below min
        expect(() => validateSettingValue('fileOperations.progressUpdateInterval', 0)).toThrow()
    })
})

describe('buildSectionTree', () => {
    it('should build a tree from settings', () => {
        const tree = buildSectionTree()
        expect(Array.isArray(tree)).toBe(true)
        expect(tree.length).toBeGreaterThan(0)
    })

    it('should have General section at top level', () => {
        const tree = buildSectionTree()
        const general = tree.find((s) => s.name === 'General')
        expect(general).toBeDefined()
    })

    it('should have nested Appearance subsection under General', () => {
        const tree = buildSectionTree()
        const general = tree.find((s) => s.name === 'General')
        expect(general?.subsections.some((s) => s.name === 'Appearance')).toBe(true)
    })

    it('should have path arrays matching section hierarchy', () => {
        const tree = buildSectionTree()
        for (const section of tree) {
            expect(section.path).toEqual([section.name])
        }
    })
})
