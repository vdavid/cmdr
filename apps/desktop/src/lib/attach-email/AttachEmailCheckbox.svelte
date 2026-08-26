<script lang="ts">
    import { tick, type Snippet } from 'svelte'
    import Checkbox from '$lib/ui/Checkbox.svelte'
    import LinkButton from '$lib/ui/LinkButton.svelte'
    import TextInput from '$lib/ui/TextInput.svelte'
    import Trans from '$lib/intl/Trans.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import type { AttachEmail } from './attach-email.svelte'

    /**
     * The "Attach my email address … so you can follow up" opt-in, as shown by the
     * crash-report, error-report, and feedback dialogs. It always renders: with an address
     * on file the label names it and offers a "change" link into Settings, and without one
     * a tick reveals a field to type one into, so a user who skipped the onboarding beta
     * step can still leave a reply address.
     *
     * Pass the state from `createAttachEmail()`; the box and the field bind straight to it,
     * and the shape follows `analytics.email` while the dialog is up.
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
        // A field revealed by a LIVE settings change takes no focus: the user is in the
        // Settings window at that moment, and stealing the caret would yank them back.
        if (!checked || email.hasContactEmail) return
        await tick()
        inputElement?.focus()
    }

    /**
     * Keep a click on the inline link from also ticking the box.
     *
     * Ark's `Checkbox.Root` IS a `<label>`, and the link renders inside it. HTML says a
     * label doesn't activate its control for a click on interactive content inside it, but
     * that exclusion isn't worth betting the tick on: a canceled click can't activate a
     * label anywhere. It has to happen on the way DOWN, above the label, because the box
     * has already toggled by the time a handler on the link itself runs.
     */
    function handleChangeClickCapture(event: MouseEvent) {
        // The link is the only `<button>` in here; the box itself is an `<input>`.
        const source = event.target
        if (source instanceof Element && source.closest('button')) {
            event.preventDefault()
        }
    }

    async function openContactEmailSettings() {
        // Imported lazily for the reason `ShortcutChip` does it: this control renders in the
        // crash-report dialog, and a static import would drag the settings window's Tauri
        // surface into all three dialogs' module graphs at eval time.
        const { openSettingsWindow, settingAnchorId } = await import('$lib/settings/settings-window')
        // The section path is `analytics.email`'s registered home, asserted in
        // `settings-registry.test.ts`.
        await openSettingsWindow('attach-email', ['Updates & privacy'], settingAnchorId('analytics.email'))
    }
</script>

{#snippet changeLink(children: Snippet)}
    <LinkButton onclick={() => void openContactEmailSettings()}>{@render children()}</LinkButton>
{/snippet}

<div class="attach-email" style={containerStyle} onclickcapture={handleChangeClickCapture}>
    <Checkbox bind:checked={email.attach} onCheckedChange={(checked: boolean) => void handleCheckedChange(checked)}>
        {#if email.hasContactEmail}
            <Trans
                key="common.attachEmail"
                params={{ emailAddress: email.contactEmail }}
                snippets={{ change: changeLink }}
            />
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
