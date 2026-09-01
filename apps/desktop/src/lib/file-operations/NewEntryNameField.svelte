<script lang="ts">
    import { onDestroy, onMount, tick } from 'svelte'
    import TextInput from '$lib/ui/TextInput.svelte'
    import Trans from '$lib/intl/Trans.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import type { NamedKind } from './mutation-error-messages'
    import type { NewEntryNameCheck } from './new-entry-name-check.svelte'

    /**
     * The "Create <kind> in <dir>" subtitle plus the name field the New folder and
     * New file dialogs share. Owns the field's lifecycle: focus and select on mount,
     * validate a pre-filled name, re-validate on every keystroke and directory diff.
     */
    interface Props {
        /** Picks the copy (subtitle, aria label, placeholder) and the error element's id. */
        kind: NamedKind
        /** The directory the entry lands in; its last segment names it in the subtitle. */
        currentPath: string
        /** The validation state this field shows and drives. */
        check: NewEntryNameCheck
        /** The name being typed. */
        value: string
        /** The underlying input, for a parent that needs to hand focus back to it. */
        inputElement?: HTMLInputElement
        /** Enter pressed in the field. */
        onSubmit: () => void
    }

    /* eslint-disable prefer-const -- the two `$bindable()` props need `let` */
    let { kind, currentPath, check, value = $bindable(), inputElement = $bindable(), onSubmit }: Props = $props()
    /* eslint-enable prefer-const */

    const COPY = {
        folder: {
            nameAria: 'fileOperations.mkdir.nameAria',
            placeholder: 'fileOperations.mkdir.placeholder',
            errorId: 'new-folder-error',
        },
        file: {
            nameAria: 'fileOperations.mkfile.nameAria',
            placeholder: 'fileOperations.mkfile.placeholder',
            errorId: 'new-file-error',
        },
    } as const

    const copy = $derived(COPY[kind])
    const currentDirName = $derived(currentPath.split('/').pop() || currentPath)

    onMount(async () => {
        await tick()
        inputElement?.focus()
        inputElement?.select()

        // Validate the initial name if pre-filled
        if (value.trim()) {
            void check.validate(value)
        }

        await check.listen()
    })

    onDestroy(() => {
        check.dispose()
    })

    function handleKeydown(event: KeyboardEvent) {
        if (event.key === 'Enter') {
            event.preventDefault()
            event.stopPropagation()
            onSubmit()
        }
    }
</script>

<p class="subtitle">
    {#if kind === 'folder'}
        <Trans key="fileOperations.mkdir.createIn" params={{ name: currentDirName }} snippets={{ dir }} />
    {:else}
        <Trans key="fileOperations.mkfile.createIn" params={{ name: currentDirName }} snippets={{ dir }} />
    {/if}
</p>

<div class="input-group">
    <TextInput
        bind:inputElement
        bind:value
        invalid={!!check.errorMessage}
        ariaLabel={tString(copy.nameAria)}
        aria-describedby={check.errorMessage ? copy.errorId : undefined}
        spellcheck={false}
        autocomplete="off"
        placeholder={tString(copy.placeholder)}
        onkeydown={handleKeydown}
        oninput={() => {
            check.schedule()
        }}
    />
    {#if check.errorMessage}
        <p id={copy.errorId} class="error-message" role="alert">{check.errorMessage}</p>
    {/if}
</div>

{#snippet dir(children: import('svelte').Snippet)}<span class="dir-name">{@render children()}</span>{/snippet}

<style>
    .subtitle {
        margin: 0 0 var(--spacing-lg);
        font-size: var(--font-size-md);
        color: var(--color-text-secondary);
    }

    .dir-name {
        color: var(--color-text-primary);
        font-weight: 500;
    }

    .input-group {
        margin-bottom: var(--spacing-lg);
    }

    .error-message {
        margin: var(--spacing-sm) 0 0;
        font-size: var(--font-size-sm);
        color: var(--color-error);
    }
</style>
