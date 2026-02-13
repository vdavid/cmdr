/** Reactive state for inline rename. Must be .svelte.ts for $state(). */

import type { ValidationResult, ValidationSeverity } from '$lib/utils/filename-validation'

export interface RenameTarget {
    /** Full path to the file being renamed */
    path: string
    /** Original filename */
    originalName: string
    /** Parent directory path */
    parentPath: string
    /** Index in the file list (frontend index, includes ".." offset) */
    index: number
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
}

const initialValidation: ValidationResult = { severity: 'ok', message: '' }

function createInitialState(): RenameState {
    return {
        active: false,
        target: null,
        currentName: '',
        validation: initialValidation,
        shaking: false,
        focusTrigger: 0,
    }
}

export function createRenameState() {
    let state = $state<RenameState>(createInitialState())

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

        /** Activates rename mode for the given target. */
        activate(target: RenameTarget) {
            state = {
                active: true,
                target,
                currentName: target.originalName,
                validation: initialValidation,
                shaking: false,
                focusTrigger: 0,
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

        /** Deactivates rename mode, resetting all state. */
        cancel() {
            state = createInitialState()
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
