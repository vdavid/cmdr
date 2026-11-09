<script lang="ts">
    import { onMount, tick, untrack } from 'svelte'
    import { eventMatchesCommand } from '#lib/shortcuts/index.js'
    import { dependOn } from '#lib/utils/reactivity.js'
    import type { RenameSessionId } from './rename-state.svelte'
    import { renameStepDirection, type RenameStepDirection } from './rename-step'
    import { nextRenameSelection, renameSelectionStages, stageDirection, wordStopSelection } from './rename-selection'
    import type { NameKind } from '#lib/utils/file-extension.js'
    import {
        attachStartedElsewhereGuard,
        startedElsewhereLine,
        type StartedElsewhereGuard,
        type ToolInitiator,
    } from '#lib/ui/started-elsewhere-guard.js'

    interface Props {
        /** Current filename value (two-way bindable via onInput) */
        value: string
        /** File, folder, or package (`nameKindOf`): a folder's dot is part of its name unless it's a package */
        nameKind?: NameKind
        /** The rename session this editor was opened for; handed back with `onCancel` */
        sessionId: RenameSessionId
        /** Validation severity for border coloring */
        severity: 'ok' | 'error' | 'warning'
        /** Whether the shake animation is active */
        shaking: boolean
        /** Accessible label */
        ariaLabel: string
        /** Whether there's a validation error (for aria-invalid) */
        ariaInvalid?: boolean
        /** Validation message announced to screen readers via aria-live region */
        validationMessage?: string
        /** Increment to re-focus the input and restore selection (after dialog closes) */
        focusTrigger?: number
        /**
         * Who started this, when a tool did and a stray keystroke mustn't touch it: it waits for the
         * keyboard to go quiet, takes text only after a click or Tab, and a click away cancels instead
         * of saving a name nobody chose (`started-elsewhere-guard.ts`). Read once, as it opens.
         */
        startedBy?: ToolInitiator
        /** Called when the value changes */
        onInput: (value: string) => void
        /** Called when Enter is pressed */
        onSubmit: () => void
        /** Called when Escape is pressed or focus leaves, naming the session that asked */
        onCancel: (sessionId: RenameSessionId) => void
        /** Called when the user presses the mouse anywhere outside the input */
        onClickAway: () => void
        /** Called on a bare arrow: chain the rename to the row above or below, for the session that asked */
        onStep?: (direction: RenameStepDirection, sessionId: RenameSessionId) => void
        /** Called when shake animation ends */
        onShakeEnd: () => void
    }

    const {
        value,
        nameKind = 'file',
        sessionId,
        severity,
        shaking,
        ariaLabel,
        ariaInvalid = false,
        validationMessage = '',
        focusTrigger = 0,
        startedBy,
        onInput,
        onSubmit,
        onCancel,
        onClickAway,
        onStep,
        onShakeEnd,
    }: Props = $props()

    let inputElement: HTMLInputElement | undefined = $state()

    // Read once, on purpose: this editor answers for the session it was OPENED
    // for. When the next file's editor takes over, this one blurs on its way out
    // and cancels — naming the session that is live by then would discard the
    // edit the user has already started there.
    const openedForSession = sessionId

    // Read once too: a rename that's over can't turn this editor into another kind.
    const guardedFor = untrack(() => startedBy)
    let guard: StartedElsewhereGuard | null = null
    let armed = $state(false)

    /** Focuses the input and selects the filename excluding the extension (the cycle's first stage). */
    function focusAndSelect() {
        if (!inputElement) return
        inputElement.focus()
        const first = renameSelectionStages(value, nameKind).at(0) ?? { start: 0, end: 0 }
        inputElement.setSelectionRange(first.start, first.end, stageDirection(first))
    }

    /** `file.rename` pressed again: step the selection through name, whole name, extension. */
    function cycleSelection(input: HTMLInputElement) {
        const current = { start: input.selectionStart ?? 0, end: input.selectionEnd ?? 0 }
        const next = nextRenameSelection(input.value, nameKind, current)
        input.setSelectionRange(next.start, next.end, stageDirection(next))
    }

    /**
     * ⌥← / ⌥→ (and with ⇧) get one extra stop at the extension's dot. Answered
     * only where the system's word move would certainly skip it; everything else,
     * IME composition included, is left to the system (`rename-selection.ts`).
     * True when it took the move.
     */
    function applyWordStop(e: KeyboardEvent, input: HTMLInputElement): boolean {
        if (e.isComposing || !e.altKey || e.metaKey || e.ctrlKey) return false
        if (e.key !== 'ArrowLeft' && e.key !== 'ArrowRight') return false
        const stop = wordStopSelection({
            value: input.value,
            nameKind,
            selectionStart: input.selectionStart ?? 0,
            selectionEnd: input.selectionEnd ?? 0,
            selectionDirection: input.selectionDirection ?? 'none',
            direction: e.key === 'ArrowLeft' ? 'left' : 'right',
            extend: e.shiftKey,
        })
        if (!stop) return false
        input.setSelectionRange(stop.start, stop.end, stop.direction)
        return true
    }

    function getSeverityColor(): string {
        if (severity === 'error') return 'var(--color-error)'
        if (severity === 'warning') return 'var(--color-warning)'
        return 'var(--color-allow)'
    }

    /** Inline style: border color + box-shadow (avoids stylelint custom property restrictions) */
    const inputStyle = $derived.by(() => {
        const color = getSeverityColor()
        return `border-color: ${color}; box-shadow: 0 0 6px 1px ${color};`
    })

    function handleKeyDown(e: KeyboardEvent) {
        if (e.key === 'Enter') {
            e.preventDefault()
            e.stopPropagation()
            onSubmit()
            return
        }
        if (e.key === 'Escape') {
            e.preventDefault()
            e.stopPropagation()
            onCancel(openedForSession)
            return
        }
        if (e.key === 'Tab') {
            e.preventDefault()
            e.stopPropagation()
            onCancel(openedForSession)
            return
        }
        // A bare arrow chains the rename to the next file. It's answered here and
        // goes no further: the pane behind the editor would otherwise move the
        // cursor a second time.
        //
        // A guarded editor doesn't chain: a step saves the name it's leaving, with no question asked,
        // and that name is the model's.
        const stepDirection = renameStepDirection(e)
        if (stepDirection) {
            e.preventDefault()
            e.stopPropagation()
            if (!guardedFor) onStep?.(stepDirection, openedForSession)
            return
        }
        // The Rename key again (F2 / ⇧F6, or whatever it's bound to) moves the
        // selection on instead of doing nothing.
        if (inputElement && eventMatchesCommand(e, 'file.rename')) {
            e.preventDefault()
            e.stopPropagation()
            cycleSelection(inputElement)
            return
        }
        if (inputElement && applyWordStop(e, inputElement)) {
            e.preventDefault()
            e.stopPropagation()
            return
        }
        // Stop propagation of all keys to prevent app shortcuts
        // except standard text editing keys (Cmd+C/A/Z/X/V handled natively by the input)
        e.stopPropagation()
    }

    function handleInput(e: Event) {
        const target = e.target as HTMLInputElement
        // An IME's composition can't be refused ahead of time, so undo what it wrote.
        if (guard && !guard.textAllowed()) {
            target.value = value
            return
        }
        onInput(target.value)
    }

    function handleBlur() {
        onCancel(openedForSession)
    }

    /**
     * A mouse press outside the input means "I'm done here" → save.
     *
     * Keyed off the press, not off blur: blur also fires when the row scrolls out
     * of the virtual window and the input unmounts, and that must keep discarding
     * (a scroll is not a decision to rename). Capture phase, so it lands before
     * the row and pane handlers move focus and blur us.
     *
     * A guarded editor cancels instead: the name in it is the model's, and pressing the rail or Stop
     * is no decision to keep it.
     */
    function handleDocumentMouseDown(e: MouseEvent) {
        const target = e.target
        if (target instanceof Element && target.closest('.rename-input')) return
        if (guardedFor) onCancel(openedForSession)
        else onClickAway()
    }

    function handleAnimationEnd(e: AnimationEvent) {
        if (e.animationName === 'rename-shake') {
            onShakeEnd()
        }
    }

    // Ahead of the focus below, so no key slips in first.
    onMount(() => {
        if (!guardedFor) return
        guard = attachStartedElsewhereGuard({
            scope: () => inputElement,
            tabMovesFocus: false,
            onArmedChange: (next) => {
                armed = next
            },
        })
        return () => {
            guard?.detach()
        }
    })

    onMount(async () => {
        await tick()
        focusAndSelect()
    })

    // Separate from the async onMount above: an async one can't return a cleanup.
    onMount(() => {
        document.addEventListener('mousedown', handleDocumentMouseDown, true)
        return () => {
            document.removeEventListener('mousedown', handleDocumentMouseDown, true)
        }
    })

    // Re-focus when focusTrigger increments (after a dialog closes and returns to editing)
    $effect(() => {
        dependOn(focusTrigger)
        if (focusTrigger > 0) {
            void tick().then(() => {
                focusAndSelect()
            })
        }
    })
</script>

<!-- eslint-disable-next-line cmdr/prefer-ui-primitive -- The inline rename field can't be a `TextInput`: it sizes to the file row (height 100%, `font: inherit`, 2px padding), draws a severity-colored 2px border with a live glow and a shake animation on the element itself, and its `.rename-input` class is a load-bearing hit-test hook (`FilePane`, both views, and the E2E suite all query `.closest('.rename-input')`). The framed primitive's padding alone would break row height. -->
<input
    bind:this={inputElement}
    type="text"
    class="rename-input"
    class:shaking
    {value}
    aria-label={ariaLabel}
    aria-invalid={ariaInvalid}
    aria-describedby="rename-validation-live"
    data-armed={guardedFor ? armed : undefined}
    style={inputStyle}
    oninput={handleInput}
    onkeydown={handleKeyDown}
    onblur={handleBlur}
    onanimationend={handleAnimationEnd}
/>
<span id="rename-validation-live" class="sr-only" aria-live="assertive">{validationMessage}</span>
{#if guardedFor}
    <span class="rename-started-elsewhere" role="status">{startedElsewhereLine(guardedFor, armed)}</span>
{/if}

<style>
    .sr-only {
        position: absolute;
        width: 1px;
        height: 1px;
        padding: 0;
        margin: -1px;
        overflow: hidden;
        clip-path: inset(50%);
        white-space: nowrap;
        border: 0;
    }

    .rename-input {
        width: 100%;
        height: 100%;
        padding: 0 var(--spacing-xxs);
        font: inherit;
        font-size: inherit;
        line-height: inherit;
        color: var(--color-text-primary);
        background: var(--color-bg-primary);
        border: 2px solid;
        border-radius: var(--radius-sm);
        outline: none;
        box-sizing: border-box;
        user-select: text;
        -webkit-user-select: text;
        cursor: text;
    }

    .rename-input[data-armed='false'] {
        border-style: dashed;
    }

    /* Beside the field's row, in the overlay that holds it: it reads as part of the editor and steps
       over no row's name. */
    .rename-started-elsewhere {
        position: absolute;
        top: 100%;
        left: 0;
        padding: var(--spacing-xxs) var(--spacing-xs);
        font-size: var(--font-size-sm);
        white-space: nowrap;
        color: var(--color-text-primary);
        background: var(--color-bg-tertiary);
        border-radius: var(--radius-sm);
        pointer-events: none;
    }

    /* Both animations move the field, so Reduce motion drops them. The severity border and the
       live validation message still say what's wrong. With no animation, `animationend` never
       fires, so `shaking` stays set until the next keystroke clears it: harmless, since a
       re-trigger has nothing to replay. */
    @media (prefers-reduced-motion: no-preference) {
        .rename-input {
            animation: rename-glow 300ms ease-out;
        }

        .rename-input.shaking {
            animation: rename-shake 300ms ease-in-out;
        }
    }

    /* Glow animation uses only transform (box-shadow set via inline style) */
    @keyframes rename-glow {
        0% {
            transform: scale(1.02);
        }

        100% {
            transform: scale(1);
        }
    }

    @keyframes rename-shake {
        0%,
        100% {
            transform: translateX(0);
        }

        20% {
            transform: translateX(-4px);
        }

        40% {
            transform: translateX(4px);
        }

        60% {
            transform: translateX(-3px);
        }

        80% {
            transform: translateX(3px);
        }
    }
</style>
