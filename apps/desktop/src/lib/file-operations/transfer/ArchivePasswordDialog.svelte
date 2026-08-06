<script lang="ts">
    import { onMount, tick } from 'svelte'
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import TextInput from '$lib/ui/TextInput.svelte'
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
            <TextInput
                bind:inputElement={passwordInputRef}
                bind:value={password}
                type="password"
                ariaLabel={tString('fileOperations.archivePassword.inputAria')}
                spellcheck={false}
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
    }

    .archive-name {
        color: var(--color-text-primary);
        font-weight: 500;
        word-break: break-all;
    }

    .input-group {
        margin-bottom: var(--spacing-lg);
    }
</style>
