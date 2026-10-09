<script lang="ts">
    /**
     * The counter token's editor: Start, Step, and Digits, and a line saying what they count.
     * Every edit goes straight to `onChange`, which rewrites the `[C…]` token in the mask; an
     * emptied field means that part's default, and a half-typed number (a lone `-`) waits.
     *
     * Keys: ArrowDown / ArrowUp walk the fields (ArrowUp from the first one, and Enter, leave
     * through `onDone`). Both are claimed, so Enter never reaches the sheet's Enter-starts rule.
     * Escape belongs to the `Popover` around this.
     */
    import { tString } from '$lib/intl/messages.svelte'
    import TextInput from '$lib/ui/TextInput.svelte'
    import { claimKey } from '$lib/shortcuts/claim-key'
    import { COUNTER_DEFAULTS, clampCounter, counterSamples, type Counter } from './counter-token'
    import type { TokenEditorProps } from './mask-token-kinds'

    const { value, onChange, onDone }: TokenEditorProps<Counter> = $props()

    type Part = keyof Counter
    const PARTS: { part: Part; label: string }[] = [
        { part: 'start', label: tString('multiRename.counterEditor.start') },
        { part: 'step', label: tString('multiRename.counterEditor.step') },
        { part: 'digits', label: tString('multiRename.counterEditor.digits') },
    ]

    const fields: HTMLInputElement[] = $state([])
    /** What the user typed in a field, while it differs from the value (`-`, or empty). */
    let drafts = $state<Partial<Record<Part, string>>>({})

    // The numbers as they land in the names, padding included.
    const sampleLine = $derived.by(() => {
        const [first, second, third] = counterSamples(value)
        return tString('multiRename.counterEditor.counts', { first, second, third })
    })
    const uid = $props.id()
    const sampleId = `counter-editor-sample-${uid}`

    function handleInput(part: Part, text: string): void {
        drafts = { ...drafts, [part]: text }
        const trimmed = text.trim()
        if (trimmed !== '' && !/^[+-]?\d+$/.test(trimmed)) return
        const number = trimmed === '' ? COUNTER_DEFAULTS[part] : Number(trimmed)
        onChange(clampCounter({ ...value, [part]: number }))
    }

    function handleBlur(part: Part): void {
        // The field shows the value it wrote (clamped, or the default for an empty field).
        const { [part]: _dropped, ...rest } = drafts
        drafts = rest
    }

    function handleKeydown(e: KeyboardEvent, index: number): void {
        if (e.isComposing || e.metaKey || e.ctrlKey || e.altKey || e.shiftKey) return
        if (e.key === 'Enter') {
            claimKey(e)
            onDone()
        } else if (e.key === 'ArrowUp') {
            claimKey(e)
            if (index === 0) onDone()
            else fields[index - 1]?.focus()
        } else if (e.key === 'ArrowDown') {
            claimKey(e)
            fields[index + 1]?.focus()
        }
    }
</script>

<div class="counter-editor">
    <div class="fields">
        {#each PARTS as { part, label }, index (part)}
            <label class="field">
                <span class="label">{label}</span>
                <TextInput
                    bind:inputElement={fields[index]}
                    value={drafts[part] ?? String(value[part])}
                    inputmode="numeric"
                    mono
                    radius="md"
                    containerStyle="width: 72px"
                    ariaLabel={label}
                    aria-describedby={sampleId}
                    oninput={(e: Event) => { handleInput(part, (e.currentTarget as HTMLInputElement).value) }}
                    onfocus={(e: FocusEvent) => { (e.currentTarget as HTMLInputElement).select() }}
                    onblur={() => { handleBlur(part) }}
                    onkeydown={(e: KeyboardEvent) => { handleKeydown(e, index) }}
                />
            </label>
        {/each}
    </div>
    <p class="sample" id={sampleId}>{sampleLine}</p>
</div>

<style>
    .counter-editor {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-sm);
    }

    .fields {
        display: flex;
        gap: var(--spacing-sm);
    }

    .field {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xxs);
    }

    .label {
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
    }

    .sample {
        margin: 0;
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
    }
</style>
