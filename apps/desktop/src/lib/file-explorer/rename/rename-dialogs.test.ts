import { describe, it, expect, vi, afterEach } from 'vitest'
import { mount, tick } from 'svelte'
import {
  getSettingDefinition,
  getDefaultValue,
  validateSettingValue,
  getSettingsInSection,
} from '$lib/settings/settings-registry'
import { SOFT_DIALOG_REGISTRY } from '$lib/ui/dialog-registry'
import RenameConflictDialog from './RenameConflictDialog.svelte'
import type { RenameConflictResolution } from './rename-operations'

vi.mock('$lib/tauri-commands', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
}))

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

  it('should be in the Navigation & file ops section', () => {
    const def = getSettingDefinition('fileOperations.allowFileExtensionChanges')
    expect(def?.section).toEqual(['Behavior', 'Navigation & file ops'])
  })

  it('should appear in Navigation & file ops section listing', () => {
    const settings = getSettingsInSection(['Behavior', 'Navigation & file ops'])
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

  it('should use toggle-group component', () => {
    const def = getSettingDefinition('fileOperations.allowFileExtensionChanges')
    expect(def?.component).toBe('toggle-group')
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

/**
 * The conflict dialog's trash option is offered only where a trash exists.
 *
 * Its "overwrite and trash" path calls `moveToTrash`, which is macOS's
 * `NSFileManager` on a real local path: on a volume serving its own I/O, and
 * inside an archive, the call is refused and the chained rename that was waiting
 * on it never runs, so the person watches the dialog close on nothing.
 */
describe('RenameConflictDialog trash gate', () => {
  const mounted: HTMLElement[] = []

  afterEach(() => {
    for (const target of mounted.splice(0)) target.remove()
  })

  function mountDialog(supportsTrash: boolean) {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mounted.push(target)
    const resolutions: RenameConflictResolution[] = []
    mount(RenameConflictDialog, {
      target,
      props: {
        renamedFile: { name: 'report.md', size: 2048, modifiedAt: undefined },
        existingFile: { name: 'report.md', size: 1024, modifiedAt: undefined },
        supportsTrash,
        onResolve: (resolution: RenameConflictResolution) => resolutions.push(resolution),
      },
    })
    return { target, resolutions }
  }

  const labelsIn = (target: HTMLElement) => [...target.querySelectorAll('button')].map((b) => b.textContent.trim())

  function pressEnter(target: HTMLElement) {
    const dialog = target.querySelector('[role="alertdialog"]')
    expect(dialog).not.toBeNull()
    dialog?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }))
  }

  it('offers the trash where there is one, and makes it the Enter default', async () => {
    const { target, resolutions } = mountDialog(true)
    await tick()

    expect(labelsIn(target)).toContain('Overwrite and trash old file')
    pressEnter(target)
    expect(resolutions).toEqual(['overwrite-trash'])
  })

  it('drops the trash button on a volume without one', async () => {
    const { target } = mountDialog(false)
    await tick()

    const labels = labelsIn(target)
    expect(labels).not.toContain('Overwrite and trash old file')
    expect(labels).toContain('Overwrite and delete old file')
  })

  it('sends Enter to the permanent overwrite when there is no trash', async () => {
    const { target, resolutions } = mountDialog(false)
    await tick()

    pressEnter(target)

    // Never `overwrite-trash`: that call would be refused, and the rename behind
    // it would be dropped silently.
    expect(resolutions).toEqual(['overwrite-delete'])
  })
})
