<script lang="ts">
    /**
     * "Send feedback" dialog for the open beta.
     *
     * Mounted from `(main)/+layout.svelte` and driven by the reactive `feedbackFlow`
     * store, mirroring `ErrorReportDialog`. Ships the text via the `send_feedback`
     * IPC command; no log bundle rides along (that's the error reporter's job).
     */
    import { onMount, tick } from 'svelte'
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import Button from '$lib/ui/Button.svelte'
    import AttachEmailCheckbox from '$lib/attach-email/AttachEmailCheckbox.svelte'
    import { createAttachEmail } from '$lib/attach-email/attach-email.svelte'
    import TextArea from '$lib/ui/TextArea.svelte'
    import LinkButton from '$lib/ui/LinkButton.svelte'
    import { addToast } from '$lib/ui/toast'
    import { sendFeedback, openExternalUrl } from '$lib/tauri-commands'
    import { formatInteger } from '$lib/intl/number-format'
    import { t, tString } from '$lib/intl/messages.svelte'
    import Trans from '$lib/intl/Trans.svelte'
    import { closeFeedbackDialog } from './feedback-flow.svelte'
    import { getAppLogger } from '$lib/logging/logger'
    import { GITHUB_ISSUES_URL, BOOK_A_CALL_URL } from '$lib/beta-links'

    const log = getAppLogger('feedbackDialog')

    // Same caps as the error reporter's note textarea (and the backend + server validators).
    const MAX_FEEDBACK_CHARS = 100_000
    const SOFT_WARN_AT = 50_000

    let feedbackText = $state('')
    let textareaRef: HTMLTextAreaElement | undefined
    const attachEmail = createAttachEmail()
    let sending = $state(false)
    let sendFailedMessage = $state<string | null>(null)

    // Count by Unicode code points so the frontend cap matches the Rust validator's
    // `.chars().count()` and the server's `[...text].length`. `feedbackText.length`
    // (UTF-16 code units) diverges for surrogate-pair characters (most emoji).
    const textLength = $derived(Array.from(feedbackText).length)
    const overLimit = $derived(textLength > MAX_FEEDBACK_CHARS)
    const showCounter = $derived(textLength > SOFT_WARN_AT)
    const isEmpty = $derived(feedbackText.trim().length === 0)

    const canSend = $derived(!sending && !isEmpty && !overLimit && !attachEmail.blocksSend)

    async function handleSend() {
        if (!canSend) return
        sending = true
        sendFailedMessage = null
        try {
            const result = await sendFeedback(feedbackText, attachEmail.emailToAttach)
            if (result.kind === 'sent') {
                // Sticky choice and a newly typed address are remembered only now: a
                // half-typed one shouldn't become the reply channel for every report.
                attachEmail.persist()
                addToast(tString('feedback.sentToast'), { level: 'success' })
                feedbackText = ''
                closeFeedbackDialog()
            } else if (result.kind === 'invalid') {
                // Both empty and over-cap are blocked above, so this is a backstop.
                sendFailedMessage = tString('feedback.dialog.invalid')
            } else {
                sendFailedMessage = tString('feedback.dialog.softFailure')
            }
        } finally {
            sending = false
        }
    }

    async function handleOpenLink(url: string) {
        try {
            await openExternalUrl(url)
        } catch (e) {
            log.warn("Couldn't open external link: {error}", { error: String(e) })
        }
    }

    function handleClose() {
        closeFeedbackDialog()
    }

    onMount(async () => {
        // Focus the textarea so the user can type immediately (keyboard-first). After a tick
        // so it wins over ModalDialog's overlay focus, which runs in the child's onMount.
        await tick()
        textareaRef?.focus()
    })

    /**
     * Exactly ⌘/⌃Enter, no extra modifiers: ⌥⌘Enter and ⇧⌘Enter are different combos
     * and must not send feedback on their way somewhere else.
     */
    function isSendCombo(event: KeyboardEvent): boolean {
        return (event.metaKey || event.ctrlKey) && !event.altKey && !event.shiftKey && event.key === 'Enter'
    }

    function handleKeydown(event: KeyboardEvent) {
        // Cmd/Ctrl+Enter sends. Plain Enter is consumed by the textarea.
        if (isSendCombo(event)) {
            event.preventDefault()
            void handleSend()
        }
    }
</script>

{#snippet githubLink(children: import('svelte').Snippet)}
    <LinkButton
        href={GITHUB_ISSUES_URL}
        onclick={(e: MouseEvent) => {
            e.preventDefault()
            void handleOpenLink(GITHUB_ISSUES_URL)
        }}>{@render children()}</LinkButton
    >
{/snippet}
{#snippet callLink(children: import('svelte').Snippet)}
    <LinkButton
        href={BOOK_A_CALL_URL}
        onclick={(e: MouseEvent) => {
            e.preventDefault()
            void handleOpenLink(BOOK_A_CALL_URL)
        }}>{@render children()}</LinkButton
    >
{/snippet}

<ModalDialog
    titleId="feedback-dialog-title"
    onkeydown={handleKeydown}
    dialogId="feedback"
    role="dialog"
    onclose={handleClose}
    ariaDescribedby="feedback-dialog-body"
    containerStyle="width: 480px"
>
    {#snippet title()}{tString('feedback.dialog.title')}{/snippet}

    <div>
        <p id="feedback-dialog-body" class="description">
            {tString('feedback.dialog.description')}
        </p>

        <label class="feedback-label" for="feedback-text">
            <span>{tString('feedback.dialog.label')}</span>
            {#if showCounter}
                <span class="counter" class:over={overLimit}>
                    {t('feedback.dialog.counter', {
                        currentText: formatInteger(textLength),
                        maxText: formatInteger(MAX_FEEDBACK_CHARS),
                    })}
                </span>
            {/if}
        </label>
        <TextArea
            id="feedback-text"
            bind:textareaElement={textareaRef}
            bind:value={feedbackText}
            invalid={overLimit}
            radius="md"
            rows={5}
            containerStyle="margin-bottom: var(--spacing-md)"
            placeholder={tString('feedback.dialog.placeholder')}
        />
        {#if overLimit}
            <p class="helper-text">
                {t('feedback.dialog.tooLong', { maxText: formatInteger(MAX_FEEDBACK_CHARS) })}
            </p>
        {/if}

        <AttachEmailCheckbox email={attachEmail} />

        <p class="more-ways">
            <Trans key="feedback.dialog.moreWays" snippets={{ github: githubLink, call: callLink }} />
        </p>

        {#if sendFailedMessage}
            <p class="status status-error" role="alert">{sendFailedMessage}</p>
        {/if}
    </div>

    {#snippet footer()}
        <Button variant="secondary" onclick={handleClose} disabled={sending}>{tString('feedback.dialog.cancel')}</Button>
        <Button variant="primary" onclick={() => void handleSend()} disabled={!canSend}>
            {sending ? tString('feedback.dialog.sending') : tString('feedback.dialog.send')}
        </Button>
    {/snippet}
</ModalDialog>

<style>
    .description {
        margin: 0 0 var(--spacing-md);
        font-size: var(--font-size-md);
        color: var(--color-text-secondary);
    }

    .feedback-label {
        display: flex;
        justify-content: space-between;
        align-items: baseline;
        margin-bottom: var(--spacing-xs);
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }

    .counter {
        font-family: var(--font-mono);
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
    }

    .counter.over {
        color: var(--color-error);
    }

    .helper-text {
        margin: calc(var(--spacing-md) * -1) 0 var(--spacing-md);
        font-size: var(--font-size-xs);
        color: var(--color-error);
    }

    .more-ways {
        margin: 0 0 var(--spacing-md);
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
    }

    .status {
        margin: 0 0 var(--spacing-md);
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }

    .status-error {
        color: var(--color-error);
    }
</style>
