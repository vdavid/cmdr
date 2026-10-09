<script lang="ts">
    /**
     * A rename mask field whose editable placeholders (`MASK_TOKEN_KINDS`, the counter so far)
     * carry a small ▾ marker that opens an inline editor for that one token.
     *
     * - The markers are an overlay: an input can't hold widgets. Each sits at its token's end,
     *   measured with a canvas in the input's font, net of `scrollLeft`; one scrolled out of view
     *   isn't drawn. They take no focus (`tabindex="-1"`, mousedown prevented), so typing never
     *   loses the caret.
     * - ArrowDown with the caret inside or right after an editable token opens its editor and is
     *   claimed; anywhere else it's left alone.
     * - The editor lives in a house `Popover` under the token. Every edit rewrites the token in
     *   the text through `onValueChange`, so the mask stays the single source of truth. Enter,
     *   Escape, or ArrowUp from its first field return focus here with the caret after the token;
     *   a click elsewhere closes it and leaves focus where the click put it.
     */
    import { onMount, tick } from 'svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import Icon from '$lib/ui/Icon.svelte'
    import Popover from '$lib/ui/Popover.svelte'
    import TextInput from '$lib/ui/TextInput.svelte'
    import { claimKey } from '$lib/shortcuts/claim-key'
    import { MASK_TOKEN_KINDS } from './mask-token-kinds'
    import { findTokens, replaceToken, tokenAtCaret, type MaskToken } from './mask-tokens'

    type Token = MaskToken<(typeof MASK_TOKEN_KINDS)[number]>

    interface Props {
        value: string
        /** Fires with the new mask on every keystroke and on every edit made in a token editor. */
        onValueChange: (value: string) => void
        ariaLabel: string
        invalid?: boolean
        /** The text field, for imperative focus and caret moves. */
        inputElement?: HTMLInputElement
    }

    /* eslint-disable prefer-const -- $bindable() requires `let` destructuring */
    let { value, onValueChange, ariaLabel, invalid = false, inputElement = $bindable() }: Props = $props()
    /* eslint-enable prefer-const */

    const MARKER_WIDTH = 12
    /** The marker rides this far up from the text box's bottom, so it sits in the field's bottom padding. */
    const MARKER_RISE = 2

    let wrapperEl: HTMLDivElement | undefined = $state()
    let anchorEl: HTMLSpanElement | undefined = $state()

    const tokens = $derived(findTokens(value, MASK_TOKEN_KINDS))

    /** Where each token's marker sits, keyed by the token's start; `null` while scrolled out of view. */
    let markerLeft = $state<Partial<Record<number, number | null>>>({})
    let markerTop = $state(0)

    /** The token being edited, by its start (a rewrite changes only its end). */
    let editingFrom = $state<number | null>(null)
    const editing = $derived(tokens.find((token) => token.span.from === editingFrom))
    /** Where the caret goes once the editor closes through a key. */
    let returnCaret: number | null = null

    let measureContext: CanvasRenderingContext2D | null | undefined

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
        // An input not laid out (no width yet) can't say what's scrolled away, so every marker shows.
        const laidOut = input.clientWidth > 0
        const next: Record<number, number | null> = {}
        for (const token of current) {
            const width = measureContext?.measureText(value.slice(0, token.span.to)).width ?? 0
            const end = textLeft + width
            const visible = !laidOut || (end >= contentLeft && end <= contentLeft + input.clientWidth)
            next[token.span.from] = visible ? end : null
        }
        markerLeft = next
        markerTop = inputRect.bottom - wrapperRect.top
    }

    $effect(() => {
        // Re-measure after every render of a new mask (and its tokens).
        measure(tokens)
    })

    onMount(() => {
        const input = inputElement
        if (!input) return
        const remeasure = (): void => {
            measure(tokens)
        }
        const onSelectionChange = (): void => {
            if (document.activeElement === input) remeasure()
        }
        const resize = new ResizeObserver(remeasure)
        resize.observe(input)
        input.addEventListener('scroll', remeasure)
        document.addEventListener('selectionchange', onSelectionChange)
        return () => {
            resize.disconnect()
            input.removeEventListener('scroll', remeasure)
            document.removeEventListener('selectionchange', onSelectionChange)
        }
    })

    function open(token: Token): void {
        returnCaret = null
        editingFrom = token.span.from
    }

    function rewrite(token: Token, inner: string): void {
        onValueChange(replaceToken(value, token.span, inner).mask)
    }

    /** Closes the editor; focus comes back through the anchor (Escape) or `focusAfterEditing` (Enter, ArrowUp). */
    function close(): void {
        returnCaret = editing?.span.to ?? null
        editingFrom = null
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
        if (e.key !== 'ArrowDown' || e.isComposing || e.metaKey || e.ctrlKey || e.altKey || e.shiftKey) return
        const input = e.currentTarget as HTMLInputElement
        const token = tokenAtCaret(tokens, input.selectionEnd ?? -1)
        if (!token) return
        claimKey(e)
        open(token)
    }
</script>

<div class="mask-input" bind:this={wrapperEl}>
    <TextInput
        mono
        bind:inputElement
        {value}
        oninput={(e: Event) => { onValueChange((e.currentTarget as HTMLInputElement).value) }}
        onkeydown={handleKeydown}
        {ariaLabel}
        {invalid}
    />
    {#each tokens as token (token.span.from)}
        {@const left = markerLeft[token.span.from]}
        {#if left !== undefined && left !== null}
            <button
                type="button"
                class="token-marker"
                class:open={editingFrom === token.span.from}
                tabindex="-1"
                aria-label={tString(token.kind.markerLabel)}
                aria-haspopup="dialog"
                aria-expanded={editingFrom === token.span.from}
                style:left="{left - MARKER_WIDTH / 2}px"
                style:top="{markerTop - MARKER_RISE}px"
                onmousedown={(e: MouseEvent) => { e.preventDefault() }}
                onclick={() => { open(token) }}
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
        style:left="{(editing ? markerLeft[editing.span.from] : null) ?? 0}px"
        style:top="{markerTop}px"
        onfocus={focusAfterEditing}
    ></span>
</div>

{#if anchorEl}
    {@const token = editing}
    <Popover
        anchor={anchorEl}
        open={token !== undefined}
        onClose={close}
        ariaLabel={token ? tString(token.kind.editorLabel) : undefined}
    >
        {#if token}
            {@const Editor = token.kind.editor}
            <Editor
                value={token.value}
                onChange={(next) => { rewrite(token, token.kind.format(next)) }}
                onDone={done}
            />
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
