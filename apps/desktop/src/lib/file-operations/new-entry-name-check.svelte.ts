/**
 * Validates the name typed into the New folder and New file dialogs.
 *
 * Sync checks first (disallowed characters, name length, full path length),
 * then an async clash lookup against the live listing. The lookup re-runs on a
 * debounce while the user types and whenever a `directory-diff` lands for the
 * listing, so a folder created underneath the dialog shows up as a clash. The
 * dialog reads `errorMessage` / `isChecking`, and writes `errorMessage` back
 * when the create itself comes back refused.
 */

import { findFileIndex, getFileAt, onDirectoryDiff, type UnlistenFn } from '$lib/tauri-commands'
import { validateDisallowedChars, validateNameLength, validatePathLength } from '$lib/utils/filename-validation'
import { tString } from '$lib/intl/messages.svelte'

/** Where the entry would land, and how to read the name being typed. */
export interface NewEntryNameCheckOptions {
  currentPath: string
  listingId: string
  showHiddenFiles: boolean
  /** Read when the debounce fires, so a re-validation sees the latest keystroke. */
  getName: () => string
}

/** Long enough to coalesce keystrokes and for the listing cache to settle after a diff. */
const DEBOUNCE_MS = 100

export class NewEntryNameCheck {
  /** Empty while the name is acceptable (or still empty). */
  errorMessage = $state('')
  /** True while the clash lookup is in flight; the dialog holds OK until it lands. */
  isChecking = $state(false)

  readonly #options: NewEntryNameCheckOptions
  #timer: ReturnType<typeof setTimeout> | undefined
  #unlistenDiff: UnlistenFn | undefined

  constructor(options: NewEntryNameCheckOptions) {
    this.#options = options
  }

  async validate(name: string): Promise<void> {
    const trimmed = name.trim()
    if (trimmed === '') {
      this.errorMessage = ''
      return
    }

    // Sync validators: chars, name length, full path length
    const charCheck = validateDisallowedChars(trimmed, true)
    if (charCheck.severity === 'error') {
      this.errorMessage = charCheck.message
      return
    }
    const nameLenCheck = validateNameLength(trimmed, true)
    if (nameLenCheck.severity === 'error') {
      this.errorMessage = nameLenCheck.message
      return
    }
    const pathLenCheck = validatePathLength(this.#options.currentPath, trimmed)
    if (pathLenCheck.severity === 'error') {
      this.errorMessage = pathLenCheck.message
      return
    }

    // Sync checks passed: clear any previous error, then run the async clash check
    this.errorMessage = ''

    this.isChecking = true
    try {
      const { listingId, showHiddenFiles } = this.#options
      const index = await findFileIndex(listingId, trimmed, showHiddenFiles)
      if (index !== null) {
        const entry = await getFileAt(listingId, index, showHiddenFiles)
        if (entry?.isDirectory) {
          this.errorMessage = tString('fileOperations.shared.conflictExistsFolder')
        } else {
          this.errorMessage = tString('fileOperations.shared.conflictExistsFile')
        }
      } else {
        this.errorMessage = ''
      }
    } catch {
      // If the lookup fails (listing gone), clear the error and let the backend decide
      this.errorMessage = ''
    } finally {
      this.isChecking = false
    }
  }

  /** Debounced `validate` of whatever the field holds when the timer fires. */
  schedule(): void {
    if (this.#timer) clearTimeout(this.#timer)
    this.#timer = setTimeout(() => {
      void this.validate(this.#options.getName())
    }, DEBOUNCE_MS)
  }

  /** Re-validates when the listing changes underneath the dialog. */
  async listen(): Promise<void> {
    this.#unlistenDiff = await onDirectoryDiff((payload) => {
      if (payload.listingId !== this.#options.listingId) return
      this.schedule()
    })
  }

  dispose(): void {
    if (this.#timer) clearTimeout(this.#timer)
    this.#unlistenDiff?.()
  }
}
