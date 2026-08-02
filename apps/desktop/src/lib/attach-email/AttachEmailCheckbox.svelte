<script lang="ts">
    import Checkbox from '$lib/ui/Checkbox.svelte'
    import { t } from '$lib/intl/messages.svelte'
    import type { AttachEmail } from './attach-email.svelte'

    /**
     * The "Attach my email (…) so we can reply" opt-in, as shown by the crash-report,
     * error-report, and feedback dialogs. Renders nothing when no contact email is on
     * file, so a caller can drop it in unconditionally.
     *
     * Pass the state from `createAttachEmail()`; the box binds straight to its `attach`.
     */
    interface Props {
        email: AttachEmail
        /** Inline style for the wrapper, for callers whose surrounding rhythm differs. */
        containerStyle?: string
    }

    const { email, containerStyle }: Props = $props()
</script>

{#if email.available}
    <div class="attach-email" style={containerStyle}>
        <Checkbox bind:checked={email.attach}>{t('common.attachEmail', { email: email.contactEmail })}</Checkbox>
    </div>
{/if}

<style>
    .attach-email {
        margin-bottom: var(--spacing-md);
        color: var(--color-text-secondary);
    }
</style>
