<script lang="ts">
    /**
     * A rename mask field whose editable placeholders (`MASK_TOKEN_KINDS`, the counter so far)
     * carry a small ▾ marker that opens an inline editor for that one token.
     *
     * - The markers are an overlay: an input can't hold widgets. Each sits centered under its
     *   token, measured with a canvas in the input's font, net of `scrollLeft`; one scrolled out of
     *   view isn't drawn. They take no focus (`tabindex="-1"`, mousedown prevented), so typing
     *   never loses the caret.
     * - The editor opens on its own (`token-editor-rules.ts`): while the caret rests inside a token,
     *   with focus staying in the field, and while the pointer rests on a token or the editor. It
     *   opens after a short delay, so a caret or pointer passing through doesn't flash it.
     * - ArrowDown with the caret inside or right after a token goes into its editor (opening it
     *   first if needed) and is claimed; anywhere else it's left alone. Escape in the field closes
     *   an open editor, and is claimed so the sheet stays.
     * - The editor lives in a house `Popover` under the token. Every edit rewrites the token in
     *   the text through `onValueChange`, so the mask stays the single source of truth. Enter,
     *   Escape, or ArrowUp from its first field return focus here with the caret after the token;
     *   a click elsewhere closes it and leaves focus where the click put it.
     */
    import { flushSync, onDestroy, onMount, tick, type Snippet } from 'svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import Icon from '$lib/ui/Icon.svelte'
    import Popover from '$lib/ui/Popover.svelte'
    import TextInput from '$lib/ui/TextInput.svelte'
    import { claimKey } from '$lib/shortcuts/claim-key'
    import { MASK_TOKEN_KINDS } from './mask-token-kinds'
    import { findTokens, replaceToken, tokenAtCaret, type MaskToken } from './mask-tokens'
    import { nextEditor, tokenInside, type EditorEvent, type EditorOpen } from './token-editor-rules'

    type Token = MaskToken<(typeof MASK_TOKEN_KINDS)[number]>

    interface Props {
        value: string
        /** Fires with the new mask on every keystroke and on every edit made in a token editor. */
        onValueChange: (value: string) => void
        ariaLabel: string
        invalid?: boolean
        /** The text field, for imperative focus and caret moves. */
        inputElement?: HTMLInputElement
        /** Controls at the field's end (the sheet's history chevron), in `TextInput`'s trailing slot. */
        trailing?: Snippet
    }

    /* eslint-disable prefer-const -- $bindable() requires `let` destructuring */
    let { value, onValueChange, ariaLabel, invalid = false, inputElement = $bindable(), trailing }: Props = $props()
    /* eslint-enable prefer-const */

    const MARKER_WIDTH = 12
    /** The marker rides this far up from the text box's bottom, so it sits in the field's bottom padding. */
    const MARKER_RISE = 2
    /** How long the caret rests inside a token before its editor opens. */
    const CARET_OPEN_MS = 250
    /** How long the pointer rests on a token before its editor opens. */
    const HOVER_OPEN_MS = 300
    /** How long after the pointer left the token and the editor the editor closes. */
    const HOVER_CLOSE_MS = 300

    let wrapperEl: HTMLDivElement | undefined = $state()
    let anchorEl: HTMLSpanElement | undefined = $state()
    let editorEl: HTMLDivElement | undefined = $state()

    const tokens = $derived(findTokens(value, MASK_TOKEN_KINDS))

    /** Each token's horizontal extent in the wrapper, keyed by its start; `null` while its middle is scrolled out of view. */
    let tokenBox = $state<Partial<Record<number, { left: number; right: number } | null>>>({})
    let markerTop = $state(0)

    /** The open editor: its token, by start (a rewrite changes only its end), and why it's open. */
    let editor = $state<EditorOpen | null>(null)
    const editing = $derived(tokens.find((token) => token.span.from === editor?.from))
    /** Where the caret goes once the editor closes through a key. */
    let returnCaret: number | null = null

    let caretTimer: ReturnType<typeof setTimeout> | undefined
    let hoverTimer: ReturnType<typeof setTimeout> | undefined
    /** The token the pointer rests on now, so a move within it doesn't restart its delay. */
    let hoveredFrom: number | null = null

    let measureContext: CanvasRenderingContext2D | null | undefined

    function middleOf(from: number): number | null {
        const box = tokenBox[from]
        return box ? (box.left + box.right) / 2 : null
    }

    function measure(current: readonly Token[]): void {
        const input = inputElement
        if (!input || !wrapperEl) return
        measureContext ??= document.createElement('canvas').getContext('2d')
        const style = getComputedStyle(input)
        if (measureContext) {
            measureContext.font = `${style.fontStyle} ${style.fontWeight} ${style.fontSize} ${style.fontFamily}`
        }
        const inputRect = input.getBoundingClientRect()
        const wrapperRect = wrapperEl.getBoundingClientRect()
        const contentLeft = inputRect.left - wrapperRect.left + input.clientLeft
        const textLeft = contentLeft + (parseFloat(style.paddingLeft) || 0) - input.scrollLeft
        const xAt = (offset: number): number => textLeft + (measureContext?.measureText(value.slice(0, offset)).width ?? 0)
        // An input not laid out (no width yet) can't say what's scrolled away, so every marker shows.
        const laidOut = input.clientWidth > 0
        const next: Record<number, { left: number; right: number } | null> = {}
        for (const token of current) {
            const left = xAt(token.span.from)
            const right = xAt(token.span.to)
            const middle = (left + right) / 2
            const visible = !laidOut || (middle >= contentLeft && middle <= contentLeft + input.clientWidth)
            next[token.span.from] = visible ? { left, right } : null
        }
        tokenBox = next
        markerTop = inputRect.bottom - wrapperRect.top
    }

    $effect(() => {
        // Re-measure after every render of a new mask (and its tokens).
        measure(tokens)
    })

    $effect(() => {
        // A token that's gone (edited into something else, deleted) takes its editor with it.
        const froms = tokens.map((token) => token.span.from)
        const current = $state.snapshot(editor)
        const next = nextEditor(current, { type: 'tokens', froms })
        if (next !== current) editor = next
    })

    function apply(event: EditorEvent): void {
        if (event.type === 'request' || event.type === 'engage' || event.type === 'dismiss') {
            clearTimeout(caretTimer)
            clearTimeout(hoverTimer)
        }
        editor = nextEditor(editor, event)
    }

    /** The token the caret is inside of now, while the field has focus and nothing is selected. */
    function caretToken(): number | null {
        const input = inputElement
        if (!input || document.activeElement !== input) return null
        if (input.selectionStart !== input.selectionEnd) return null
        return tokenInside(tokens, input.selectionEnd ?? -1)?.span.from ?? null
    }

    /** Follows the caret: a new token opens after `CARET_OPEN_MS`, anything else applies now. */
    function syncCaret(): void {
        const input = inputElement
        if (!input || document.activeElement !== input) return
        clearTimeout(caretTimer)
        const from = caretToken()
        if (from !== null && from !== editor?.from) {
            caretTimer = setTimeout(() => {
                if (document.activeElement === inputElement) apply({ type: 'caret', from: caretToken() })
            }, CARET_OPEN_MS)
            return
        }
        apply({ type: 'caret', from })
    }

    /** Follows the pointer: settling on a token opens it after a delay, leaving closes it after one. */
    function hover(from: number | null): void {
        if (from === hoveredFrom) return
        hoveredFrom = from
        clearTimeout(hoverTimer)
        if (from === null && editor?.reason !== 'hover') return
        if (from !== null && editor?.from === from) return
        hoverTimer = setTimeout(
            () => {
                apply({ type: 'hover', from })
            },
            from === null ? HOVER_CLOSE_MS : HOVER_OPEN_MS,
        )
    }

    function handlePointerMove(e: MouseEvent): void {
        if (!wrapperEl) return
        const x = e.clientX - wrapperEl.getBoundingClientRect().left
        const over = tokens.find((token) => {
            const box = tokenBox[token.span.from]
            return box && x >= box.left && x <= box.right
        })
        hover(over?.span.from ?? null)
    }

    onMount(() => {
        const input = inputElement
        if (!input) return
        const remeasure = (): void => {
            measure(tokens)
        }
        const onSelectionChange = (): void => {
            if (document.activeElement !== input) return
            remeasure()
            syncCaret()
        }
        const resize = new ResizeObserver(remeasure)
        resize.observe(input)
        input.addEventListener('scroll', remeasure)
        document.addEventListener('selectionchange', onSelectionChange)
        // Pointer-only (the keyboard's road is the caret), so these sit on the wrapper as listeners
        // rather than as handlers on a role-less element.
        const wrapper = wrapperEl
        const onPointerLeave = (): void => {
            hover(null)
        }
        wrapper?.addEventListener('mousemove', handlePointerMove)
        wrapper?.addEventListener('mouseleave', onPointerLeave)
        wrapper?.addEventListener('mousedown', handleFieldMouseDown)
        return () => {
            wrapper?.removeEventListener('mousemove', handlePointerMove)
            wrapper?.removeEventListener('mouseleave', onPointerLeave)
            wrapper?.removeEventListener('mousedown', handleFieldMouseDown)
            resize.disconnect()
            input.removeEventListener('scroll', remeasure)
            document.removeEventListener('selectionchange', onSelectionChange)
        }
    })

    onDestroy(() => {
        clearTimeout(caretTimer)
        clearTimeout(hoverTimer)
    })

    // The popover's own surface keeps a hover-opened editor open while the pointer is on it.
    $effect(() => {
        const surface = editorEl?.closest<HTMLElement>('.ui-popover')
        if (!surface) return
        const enter = (): void => {
            hover(editor?.from ?? null)
        }
        const leave = (): void => {
            hover(null)
        }
        surface.addEventListener('mouseenter', enter)
        surface.addEventListener('mouseleave', leave)
        return () => {
            surface.removeEventListener('mouseenter', enter)
            surface.removeEventListener('mouseleave', leave)
        }
    })

    function rewrite(token: Token, inner: string): void {
        onValueChange(replaceToken(value, token.span, inner).mask)
    }

    /** Closes the editor; focus comes back through the anchor (Escape) or `focusAfterEditing` (Enter, ArrowUp). */
    function close(): void {
        returnCaret = editing?.span.to ?? null
        apply({ type: 'dismiss' })
    }

    function focusAfterEditing(): void {
        const caret = returnCaret
        returnCaret = null
        const input = inputElement
        if (!input) return
        input.focus()
        if (caret !== null) input.setSelectionRange(caret, caret)
    }

    function done(): void {
        close()
        void tick().then(focusAfterEditing)
    }

    function handleKeydown(e: KeyboardEvent): void {
        if (e.isComposing || e.metaKey || e.ctrlKey || e.altKey || e.shiftKey) return
        if (e.key === 'Escape' && editor !== null) {
            claimKey(e)
            apply({ type: 'dismiss' })
            return
        }
        if (e.key !== 'ArrowDown') return
        const input = e.currentTarget as HTMLInputElement
        const token = editing ?? tokenAtCaret(tokens, input.selectionEnd ?? -1)
        if (!token) return
        claimKey(e)
        // A passive editor stops being one, which sends focus to its first field.
        apply({ type: 'request', from: token.span.from })
    }

    function handleBlur(e: FocusEvent): void {
        // Tabbing (or clicking) away from the field takes a caret-opened editor with it; going
        // into the editor itself doesn't.
        if (editor?.reason !== 'caret') return
        const next = e.relatedTarget
        if (next instanceof Node && editorEl?.closest('.ui-popover')?.contains(next)) return
        apply({ type: 'dismiss' })
    }

    function handleFieldMouseDown(e: MouseEvent): void {
        // Back from the editor into the field: hand the editor to the caret BEFORE focus moves,
        // so its focus trap is gone and doesn't pull focus back into it. A marker takes no focus.
        if (editor?.reason !== 'focus') return
        if (e.target instanceof Element && e.target.closest('.token-marker')) return
        editor = { ...editor, reason: 'caret' }
        flushSync()
    }
</script>

<div class="mask-input" bind:this={wrapperEl}>
    <TextInput
        mono
        bind:inputElement
        {value}
        oninput={(e: Event) => {
            onValueChange((e.currentTarget as HTMLInputElement).value)
            syncCaret()
        }}
        onkeydown={handleKeydown}
        onkeyup={syncCaret}
        onmouseup={syncCaret}
        onfocus={syncCaret}
        onblur={handleBlur}
        {ariaLabel}
        {invalid}
        {trailing}
    />
    {#each tokens as token (token.span.from)}
        {@const middle = middleOf(token.span.from)}
        {#if middle !== null}
            <button
                type="button"
                class="token-marker"
                class:open={editor?.from === token.span.from}
                tabindex="-1"
                aria-label={tString(token.kind.markerLabel)}
                aria-haspopup="dialog"
                aria-expanded={editor?.from === token.span.from}
                style:left="{middle - MARKER_WIDTH / 2}px"
                style:top="{markerTop - MARKER_RISE}px"
                onmousedown={(e: MouseEvent) => { e.preventDefault() }}
                onmouseenter={() => { hover(token.span.from) }}
                onclick={() => { apply({ type: 'request', from: token.span.from }) }}
            >
                <Icon name="chevron-down" size={10} aria-hidden="true" />
            </button>
        {/if}
    {/each}
    <!-- The popover's anchor and its focus-return target: Escape focuses it, and it hands focus
         on to the field with the caret after the token. -->
    <span
        class="editor-anchor"
        bind:this={anchorEl}
        tabindex="-1"
        style:left="{(editing ? middleOf(editing.span.from) : null) ?? 0}px"
        style:top="{markerTop}px"
        onfocus={focusAfterEditing}
    ></span>
</div>

{#if anchorEl}
    {@const token = editing}
    <Popover
        anchor={anchorEl}
        open={token !== undefined}
        passive={editor?.reason !== 'focus'}
        alsoInside={wrapperEl}
        surface="solid"
        onClose={close}
        ariaLabel={token ? tString(token.kind.editorLabel) : undefined}
    >
        {#if token}
            {@const Editor = token.kind.editor}
            <!-- Focus arriving here (a click, ArrowDown) means the user is working in the editor. -->
            <div
                bind:this={editorEl}
                onfocusin={() => { apply({ type: 'engage' }) }}
            >
                <Editor
                    value={token.value}
                    onChange={(next) => { rewrite(token, token.kind.format(next)) }}
                    onDone={done}
                />
            </div>
        {/if}
    </Popover>
{/if}

<style>
    .mask-input {
        position: relative;
        display: flex;
        min-width: 0;
    }

    .token-marker {
        position: absolute;
        width: 12px;
        height: 10px;
        padding: 0;
        display: flex;
        align-items: center;
        justify-content: center;
        border: none;
        border-radius: var(--radius-sm);
        background: transparent;
        color: var(--color-text-tertiary);
        cursor: default;
    }

    .token-marker:hover,
    .token-marker.open {
        color: var(--color-accent-text);
    }

    .editor-anchor {
        position: absolute;
        width: 0;
        height: 0;
        outline: none;
    }
</style>
