<script lang="ts">
    import { onMount, tick } from 'svelte'
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import Button from '$lib/ui/Button.svelte'
    import Trans from '$lib/intl/Trans.svelte'
    import { tString } from '$lib/intl/messages.svelte'

    interface Props {
        /** Display name of the archive being unlocked (e.g. "photos.zip"). */
        archiveName: string
        /** True when a stored password was just rejected: re-prompt with distinct copy. */
        wrongAttempt: boolean
        /** Called with the entered password when the user confirms. */
        onSubmit: (password: string) => void
        /** Called when the user cancels (Esc, ×, or the Cancel button). */
        onCancel: () => void
    }

    const { archiveName, wrongAttempt, onSubmit, onCancel }: Props = $props()

    let password = $state('')
    let passwordInputRef: HTMLInputElement | undefined = $state()

    const titleKey = $derived(
        wrongAttempt ? 'fileOperations.archivePassword.retryTitle' : 'fileOperations.archivePassword.title',
    )
    const messageKey = $derived(
        wrongAttempt ? 'fileOperations.archivePassword.retryMessage' : 'fileOperations.archivePassword.message',
    )
    const canSubmit = $derived(password.length > 0)

    onMount(async () => {
        await tick()
        passwordInputRef?.focus()
    })

    function handleSubmit() {
        if (!canSubmit) return
        onSubmit(password)
    }

    function handleInputKeydown(event: KeyboardEvent) {
        if (event.key === 'Enter') {
            event.preventDefault()
            event.stopPropagation()
            handleSubmit()
        }
    }
</script>

<ModalDialog
    titleId="archive-password-title"
    ariaDescribedby="archive-password-message"
    dialogId="archive-password"
    onclose={onCancel}
    containerStyle="width: 400px"
>
    {#snippet title()}{tString(titleKey)}{/snippet}

    <div class="dialog-body">
        <p id="archive-password-message" class="subtitle">
            <Trans key={messageKey} params={{ name: archiveName }} snippets={{ archive }} />
        </p>

        <div class="input-group">
            <input
                bind:this={passwordInputRef}
                bind:value={password}
                type="password"
                class="password-input"
                aria-label={tString('fileOperations.archivePassword.inputAria')}
                spellcheck="false"
                autocomplete="off"
                autocapitalize="off"
                autocorrect="off"
                placeholder={tString('fileOperations.archivePassword.placeholder')}
                onkeydown={handleInputKeydown}
            />
        </div>
    </div>

    {#snippet footer()}
        <Button variant="secondary" onclick={onCancel}>{tString('fileOperations.button.cancel')}</Button>
        <Button variant="primary" onclick={handleSubmit} disabled={!canSubmit}
            >{tString('fileOperations.archivePassword.unlock')}</Button
        >
    {/snippet}
</ModalDialog>

{#snippet archive(children: import('svelte').Snippet)}<span class="archive-name">{@render children()}</span>{/snippet}

<style>
    .subtitle {
        margin: 0 0 var(--spacing-lg);
        font-size: var(--font-size-md);
        color: var(--color-text-secondary);
        line-height: 1.4;
    }

    .archive-name {
        color: var(--color-text-primary);
        font-weight: 500;
        word-break: break-all;
    }

    .input-group {
        margin-bottom: var(--spacing-lg);
    }

    .password-input {
        width: 100%;
        padding: var(--spacing-md) var(--spacing-md);
        font-size: var(--font-size-md);
        font-family: var(--font-system) sans-serif;
        background: var(--color-bg-primary);
        border: 2px solid var(--color-accent);
        border-radius: var(--radius-md);
        color: var(--color-text-primary);
        box-sizing: border-box;
    }

    .password-input::placeholder {
        color: var(--color-text-tertiary);
    }

    .password-input:focus {
        outline: none;
        box-shadow: var(--shadow-focus);
    }
</style>
