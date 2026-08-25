<script lang="ts">
    import { tick } from 'svelte'
    import Checkbox from '$lib/ui/Checkbox.svelte'
    import TextInput from '$lib/ui/TextInput.svelte'
    import { t, tString } from '$lib/intl/messages.svelte'
    import type { AttachEmail } from './attach-email.svelte'

    /**
     * The "Attach my email … so we can reply" opt-in, as shown by the crash-report,
     * error-report, and feedback dialogs. It always renders: with an address on file the
     * label names it, and without one a tick reveals a field to type one into, so a user
     * who skipped the onboarding beta step can still leave a reply address.
     *
     * Pass the state from `createAttachEmail()`; the box and the field bind straight to it.
     */
    interface Props {
        email: AttachEmail
        /** Inline style for the wrapper, for callers whose surrounding rhythm differs. */
        containerStyle?: string
    }

    const { email, containerStyle }: Props = $props()

    // Two dialogs can be on screen at once (a crash report over an error report), so the
    // describedby wiring needs ids that can't collide.
    const uid = $props.id()
    const messageId = `${uid}-message`

    let inputElement = $state<HTMLInputElement | undefined>()

    /** The field only exists once the user asks for a reply and has no address on file. */
    const collecting = $derived(email.attach && !email.hasContactEmail)

    async function handleCheckedChange(checked: boolean) {
        // Keyboard-first: a tick that reveals a field should land the caret in it, so the
        // user can answer the question they were just asked without reaching for Tab.
        if (!checked || email.hasContactEmail) return
        await tick()
        inputElement?.focus()
    }
</script>

<div class="attach-email" style={containerStyle}>
    <Checkbox bind:checked={email.attach} onCheckedChange={(checked: boolean) => void handleCheckedChange(checked)}>
        {#if email.hasContactEmail}
            {t('common.attachEmail', { email: email.contactEmail })}
        {:else}
            {tString('common.attachEmailPrompt')}
        {/if}
    </Checkbox>

    {#if collecting}
        <div class="attach-email-field">
            <TextInput
                type="email"
                bind:value={email.typedEmail}
                bind:inputElement
                ariaLabel={tString('common.attachEmailInputLabel')}
                placeholder={tString('common.attachEmailPlaceholder')}
                invalid={email.typedEmailInvalid}
                aria-describedby={email.typedEmailInvalid ? messageId : undefined}
                autocomplete="email"
                autocapitalize="off"
                spellcheck={false}
                containerStyle="max-width: 280px"
            />
            {#if email.typedEmailInvalid}
                <p class="attach-email-hint" id={messageId}>{tString('common.attachEmailInvalid')}</p>
            {/if}
        </div>
    {/if}
</div>

<style>
    .attach-email {
        margin-bottom: var(--spacing-md);
        color: var(--color-text-secondary);
    }

    /* Indented under the checkbox label so the field reads as its answer, not as the
       next control down. The inset is the 16px box plus the label gap. */
    .attach-email-field {
        margin-top: var(--spacing-sm);
        margin-left: calc(var(--spacing-lg) + var(--spacing-sm));
    }

    .attach-email-hint {
        margin: var(--spacing-xs) 0 0;
        font-size: var(--font-size-xs);
        color: var(--color-error);
    }
</style>
