import { describe, it, expect } from 'vitest'
import {
    getSettingDefinition,
    getDefaultValue,
    validateSettingValue,
    getSettingsInSection,
} from '$lib/settings/settings-registry'
import { SOFT_DIALOG_REGISTRY } from '$lib/ui/dialog-registry'

describe('allowFileExtensionChanges setting', () => {
    it('should be registered in settings registry', () => {
        const def = getSettingDefinition('fileOperations.allowFileExtensionChanges')
        expect(def).toBeDefined()
        expect(def?.id).toBe('fileOperations.allowFileExtensionChanges')
    })

    it('should default to "ask"', () => {
        const value = getDefaultValue('fileOperations.allowFileExtensionChanges')
        expect(value).toBe('ask')
    })

    it('should be an enum type with three options', () => {
        const def = getSettingDefinition('fileOperations.allowFileExtensionChanges')
        expect(def?.type).toBe('enum')
        expect(def?.constraints?.options).toHaveLength(3)
        const values = def?.constraints?.options?.map((o) => o.value)
        expect(values).toContain('yes')
        expect(values).toContain('no')
        expect(values).toContain('ask')
    })

    it('should be in the File operations section', () => {
        const def = getSettingDefinition('fileOperations.allowFileExtensionChanges')
        expect(def?.section).toEqual(['General', 'File operations'])
    })

    it('should appear in File operations section listing', () => {
        const settings = getSettingsInSection(['General', 'File operations'])
        const ids = settings.map((s) => s.id)
        expect(ids).toContain('fileOperations.allowFileExtensionChanges')
    })

    it('should validate valid enum values', () => {
        expect(() => {
            validateSettingValue('fileOperations.allowFileExtensionChanges', 'yes')
        }).not.toThrow()
        expect(() => {
            validateSettingValue('fileOperations.allowFileExtensionChanges', 'no')
        }).not.toThrow()
        expect(() => {
            validateSettingValue('fileOperations.allowFileExtensionChanges', 'ask')
        }).not.toThrow()
    })

    it('should reject invalid enum values', () => {
        expect(() => {
            validateSettingValue('fileOperations.allowFileExtensionChanges', 'maybe')
        }).toThrow()
        expect(() => {
            validateSettingValue('fileOperations.allowFileExtensionChanges', true)
        }).toThrow()
        expect(() => {
            validateSettingValue('fileOperations.allowFileExtensionChanges', 42)
        }).toThrow()
    })

    it('should use radio component', () => {
        const def = getSettingDefinition('fileOperations.allowFileExtensionChanges')
        expect(def?.component).toBe('radio')
    })
})

describe('rename dialog registry', () => {
    it('should include rename-conflict dialog', () => {
        const ids = SOFT_DIALOG_REGISTRY.map((d) => d.id)
        expect(ids).toContain('rename-conflict')
    })

    it('should include extension-change dialog', () => {
        const ids = SOFT_DIALOG_REGISTRY.map((d) => d.id)
        expect(ids).toContain('extension-change')
    })

    it('should have descriptions for rename dialogs', () => {
        const renameConflict = SOFT_DIALOG_REGISTRY.find((d) => d.id === 'rename-conflict')
        const extensionChange = SOFT_DIALOG_REGISTRY.find((d) => d.id === 'extension-change')
        expect(renameConflict?.description).toBeTruthy()
        expect(extensionChange?.description).toBeTruthy()
    })
})
