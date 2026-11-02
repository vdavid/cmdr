import { describe, it, expect, vi, afterEach } from 'vitest'
import { mount, tick } from 'svelte'
import {
  getSettingDefinition,
  getDefaultValue,
  validateSettingValue,
  getSettingsInSection,
} from '#lib/settings/settings-registry.js'
import { SOFT_DIALOG_REGISTRY } from '#lib/ui/dialog-registry.js'
import RenameConflictDialog from './RenameConflictDialog.svelte'
import ExtensionChangeDialog from './ExtensionChangeDialog.svelte'
import type { RenameConflictResolution } from './rename-operations'

vi.mock('#lib/tauri-commands/index.js', async (importOriginal) => ({
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
        renamedFile: { name: 'report.md', size: 2048, modifiedAt: undefined, isDirectory: false },
        existingFile: { name: 'report.md', size: 1024, modifiedAt: undefined, isDirectory: false },
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

/**
 * A rename the agent started picks the name, so what that name runs into is its doing too. The Enter
 * that commits the editor mustn't also answer the question the commit raises.
 */
describe('a rename someone else started', () => {
  const mounted: HTMLElement[] = []

  afterEach(() => {
    for (const target of mounted.splice(0)) target.remove()
  })

  function targetInBody(): HTMLElement {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mounted.push(target)
    return target
  }

  function pressEnter(target: HTMLElement) {
    target
      .querySelector('[role="alertdialog"]')
      ?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }))
  }

  it('keeps a stray Enter off the conflict question until it has armed', async () => {
    const target = targetInBody()
    const resolutions: RenameConflictResolution[] = []
    mount(RenameConflictDialog, {
      target,
      props: {
        renamedFile: { name: 'report.md', size: 2048, modifiedAt: undefined, isDirectory: false },
        existingFile: { name: 'report.md', size: 1024, modifiedAt: undefined, isDirectory: false },
        supportsTrash: true,
        startedBy: 'agent',
        onResolve: (resolution: RenameConflictResolution) => resolutions.push(resolution),
      },
    })
    await tick()

    expect(target.querySelector('[data-armed]')?.getAttribute('data-armed')).toBe('false')
    pressEnter(target)
    expect(resolutions).toEqual([])
  })

  it('keeps a stray Enter off the extension question until it has armed', async () => {
    const target = targetInBody()
    const onUseNew = vi.fn()
    mount(ExtensionChangeDialog, {
      target,
      props: { oldExtension: 'txt', newExtension: 'command', startedBy: 'agent', onKeepOld: vi.fn(), onUseNew },
    })
    await tick()

    expect(target.querySelector('[data-armed]')?.getAttribute('data-armed')).toBe('false')
    pressEnter(target)
    expect(onUseNew).not.toHaveBeenCalled()
  })

  it('leaves a person’s own rename unguarded', async () => {
    const target = targetInBody()
    mount(ExtensionChangeDialog, {
      target,
      props: { oldExtension: 'txt', newExtension: 'md', onKeepOld: vi.fn(), onUseNew: vi.fn() },
    })
    await tick()

    expect(target.querySelector('[data-armed]')).toBeNull()
  })
})

/**
 * A name a FOLDER holds is a different question: replacing it takes everything inside.
 * Every backend refuses a confirmed replace of a folder that holds something
 * (`Volume::rename`), so the permanent overwrite is never offered for one, and Enter
 * never means it. The trash, where there is one, takes the whole folder recoverably.
 */
describe('RenameConflictDialog for a folder', () => {
  const mounted: HTMLElement[] = []

  afterEach(() => {
    for (const target of mounted.splice(0)) target.remove()
  })

  async function mountDialog(supportsTrash: boolean) {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mounted.push(target)
    const resolutions: RenameConflictResolution[] = []
    mount(RenameConflictDialog, {
      target,
      props: {
        renamedFile: { name: 'report.md', size: 2048, modifiedAt: undefined, isDirectory: false },
        existingFile: { name: 'report.md', size: 0, modifiedAt: undefined, isDirectory: true },
        supportsTrash,
        onResolve: (resolution: RenameConflictResolution) => resolutions.push(resolution),
      },
    })
    await tick()
    return { target, resolutions }
  }

  const labelsIn = (target: HTMLElement) => [...target.querySelectorAll('button')].map((b) => b.textContent.trim())

  function pressEnter(target: HTMLElement) {
    target
      .querySelector('[role="alertdialog"]')
      ?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }))
  }

  it('says it is a folder, not a file', async () => {
    const { target } = await mountDialog(false)

    expect(target.textContent).toContain('A folder with this name already exists')
    expect(target.textContent).not.toContain('A file with this name already exists')
  })

  it('never offers to delete the folder, and Enter goes back to editing the name', async () => {
    const { target, resolutions } = await mountDialog(false)

    expect(labelsIn(target).some((label) => label.startsWith('Overwrite and delete'))).toBe(false)
    pressEnter(target)
    expect(resolutions).toEqual(['continue'])
  })

  it('offers the trash for the whole folder where there is one', async () => {
    const { target, resolutions } = await mountDialog(true)

    expect(labelsIn(target)).toContain('Overwrite and trash old folder')
    pressEnter(target)
    expect(resolutions).toEqual(['overwrite-trash'])
  })
})

/**
 * Every button sits in `ModalDialog`'s footer, which keeps a row on one line in every
 * language by widening the panel: the ways out on the left, the overwrites on the right
 * with Enter's last. A row hand-built in the body has no such floor.
 */
describe('RenameConflictDialog footer', () => {
  const mounted: HTMLElement[] = []

  afterEach(() => {
    for (const target of mounted.splice(0)) target.remove()
  })

  async function mountDialog(supportsTrash: boolean) {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mounted.push(target)
    const resolutions: RenameConflictResolution[] = []
    mount(RenameConflictDialog, {
      target,
      props: {
        renamedFile: { name: 'report.md', size: 2048, modifiedAt: undefined, isDirectory: false },
        existingFile: { name: 'report.md', size: 1024, modifiedAt: undefined, isDirectory: false },
        supportsTrash,
        onResolve: (resolution: RenameConflictResolution) => resolutions.push(resolution),
      },
    })
    await tick()
    return { target, resolutions }
  }

  const labels = (target: HTMLElement, selector: string) =>
    [...target.querySelectorAll(`${selector} button`)].map((b) => b.textContent.trim())

  it('puts the ways out in the leading slot and the overwrites in the row, Enter’s last', async () => {
    const { target } = await mountDialog(true)

    expect(labels(target, '.modal-footer-leading')).toEqual(['Cancel', 'Continue renaming'])
    expect(labels(target, '.modal-footer-actions')).toEqual([
      'Overwrite and delete old file',
      'Overwrite and trash old file',
    ])
    expect(target.querySelectorAll('.modal-body button')).toHaveLength(0)
  })

  it('answers each button with its own resolution', async () => {
    const { target, resolutions } = await mountDialog(true)

    for (const button of target.querySelectorAll<HTMLButtonElement>('.modal-footer button')) button.click()

    expect(resolutions).toEqual(['cancel', 'continue', 'overwrite-delete', 'overwrite-trash'])
  })
})

/**
 * Two files a few bytes apart both read "1.18 MiB", and the choice feeds an
 * overwrite: the prompt must show they differ, or say they don't (#782).
 */
describe('RenameConflictDialog sizes that look the same', () => {
  const mounted: HTMLElement[] = []

  afterEach(() => {
    for (const target of mounted.splice(0)) target.remove()
  })

  async function mountSizes(renamedSize: number, existingSize: number, existingIsDirectory = false) {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mounted.push(target)
    mount(RenameConflictDialog, {
      target,
      props: {
        renamedFile: { name: 'report.md', size: renamedSize, modifiedAt: undefined, isDirectory: false },
        existingFile: {
          name: 'report.md',
          size: existingSize,
          modifiedAt: undefined,
          isDirectory: existingIsDirectory,
        },
        supportsTrash: true,
        onResolve: () => {},
      },
    })
    await tick()
    return target
  }

  const sizesIn = (target: HTMLElement) =>
    [...target.querySelectorAll('.size-value')].map((el) => el.textContent.trim())

  it('shows exact bytes on both sides when the friendly sizes match but the bytes differ', async () => {
    const target = await mountSizes(1_234_570, 1_234_567)
    expect(sizesIn(target)).toEqual(['1,234,570 bytes', '1,234,567 bytes'])
    expect(target.querySelector('.same-size')).toBeNull()
  })

  it('says "same size" when the bytes match', async () => {
    const target = await mountSizes(1_234_567, 1_234_567)
    expect(sizesIn(target)).toEqual(['1.23 MB', '1.23 MB'])
    expect(target.querySelector('.same-size')?.textContent.trim()).toBe('(same size)')
  })

  it('keeps friendly sizes that already differ', async () => {
    const target = await mountSizes(2048, 1024)
    expect(sizesIn(target)).toEqual(['2.05 kB', '1.02 kB'])
  })

  it('makes no size comparison against a folder', async () => {
    const target = await mountSizes(0, 0, true)
    expect(target.querySelector('.same-size')).toBeNull()
  })
})

/**
 * The extension-change dialog words each of the three changes on its own:
 * swapping one extension for another, removing it, and adding one where there
 * was none. A dialog that only knew the swap read "Use ." and "to “.”".
 */
describe('ExtensionChangeDialog wording', () => {
  const mounted: HTMLElement[] = []

  afterEach(() => {
    for (const target of mounted.splice(0)) target.remove()
  })

  async function mountDialog(oldExtension: string, newExtension: string, isDirectory = false) {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mounted.push(target)
    mount(ExtensionChangeDialog, {
      target,
      props: { oldExtension, newExtension, isDirectory, onKeepOld: () => {}, onUseNew: () => {} },
    })
    await tick()
    const description = target.querySelector('#extension-change-description')?.textContent.trim() ?? ''
    const buttons = [...target.querySelectorAll('.modal-footer button')].map((b) => b.textContent.trim())
    return { description, buttons }
  }

  it('words a swap with both extensions', async () => {
    const { description, buttons } = await mountDialog('txt', 'md')

    expect(description).toContain('from “.txt” to “.md”')
    expect(buttons).toEqual(['Keep .txt', 'Use .md'])
  })

  it('words a removal without an empty extension', async () => {
    const { description, buttons } = await mountDialog('txt', '')

    expect(description).toContain('remove the “.txt” extension')
    expect(description).not.toContain('“.”')
    expect(buttons).toEqual(['Keep .txt', 'Remove .txt'])
  })

  it('says what removing a package folder’s extension does to it', async () => {
    const { description } = await mountDialog('app', '', true)

    expect(description).toContain('remove the “.app” extension')
    expect(description).toContain('folder')
  })

  it('words an addition without an empty extension', async () => {
    const { description, buttons } = await mountDialog('', 'md')

    expect(description).toContain('add the “.md” extension')
    expect(description).not.toContain('“.”')
    expect(buttons).toEqual(['Keep no extension', 'Use .md'])
  })
})
