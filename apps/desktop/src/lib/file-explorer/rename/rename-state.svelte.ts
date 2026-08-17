/** Reactive state for inline rename. Must be .svelte.ts for $state(). */

import type { ValidationResult, ValidationSeverity } from '$lib/utils/filename-validation'

/**
 * Identifies one rename session: one activation of the inline editor, from the
 * moment it opens until the save it sent comes back.
 *
 * Everything that finishes asynchronously (the save, the permission check, the
 * sibling-name load, the editor's own blur) carries the id it started with, so
 * it can tell whether it is still speaking for the file on screen.
 */
export type RenameSessionId = number

export interface RenameTarget {
  /** Full path to the file being renamed */
  path: string
  /** Original filename */
  originalName: string
  /** Parent directory path */
  parentPath: string
  /** Whether the entry is a directory */
  isDirectory: boolean
}

export interface RenameState {
  /** Whether rename mode is active */
  active: boolean
  /** The file being renamed */
  target: RenameTarget | null
  /** Current value in the input */
  currentName: string
  /** Validation state */
  validation: ValidationResult
  /** Whether the shake animation should play (on Enter during error state) */
  shaking: boolean
  /** Incremented to re-focus the input after a dialog closes */
  focusTrigger: number
  /**
   * Id of the most recent activation. It outlives `cancel()`: ending an editing
   * session doesn't supersede it, only a NEW activation does.
   */
  sessionId: RenameSessionId
}

const initialValidation: ValidationResult = { severity: 'ok', message: '' }

function createInitialState(sessionId: RenameSessionId): RenameState {
  return {
    active: false,
    target: null,
    currentName: '',
    validation: initialValidation,
    shaking: false,
    focusTrigger: 0,
    sessionId,
  }
}

export function createRenameState() {
  let state = $state<RenameState>(createInitialState(0))

  return {
    get active() {
      return state.active
    },
    get target() {
      return state.target
    },
    get currentName() {
      return state.currentName
    },
    get validation() {
      return state.validation
    },
    get severity(): ValidationSeverity {
      return state.validation.severity
    },
    get shaking() {
      return state.shaking
    },
    get focusTrigger() {
      return state.focusTrigger
    },
    get sessionId(): RenameSessionId {
      return state.sessionId
    },

    /** Whether a newer session has activated since `sessionId` was handed out. */
    isSuperseded(sessionId: RenameSessionId): boolean {
      return sessionId !== state.sessionId
    },

    /** Activates rename mode for the given target, under a fresh session id. */
    activate(target: RenameTarget) {
      state = {
        ...createInitialState(state.sessionId + 1),
        active: true,
        target,
        currentName: target.originalName,
      }
    },

    /** Updates the current input value. */
    setCurrentName(name: string) {
      state.currentName = name
      // Clear shake on any input change
      state.shaking = false
    },

    /** Updates validation result. */
    setValidation(result: ValidationResult) {
      state.validation = result
    },

    /** Triggers shake animation. Auto-clears after the animation. */
    triggerShake() {
      state.shaking = true
    },

    /** Clears shake (called after animation ends). */
    clearShake() {
      state.shaking = false
    },

    /** Deactivates rename mode, resetting all state but the session id. */
    cancel() {
      state = createInitialState(state.sessionId)
    },

    /** Returns whether the current name (trimmed) differs from the original. */
    hasChanged(): boolean {
      if (!state.target) return false
      return state.currentName.trim() !== state.target.originalName
    },

    /** Returns the trimmed current name. */
    getTrimmedName(): string {
      return state.currentName.trim()
    },

    /** Requests the editor to re-focus and select (after a dialog closes). */
    requestRefocus() {
      state.focusTrigger++
    },
  }
}
