<!--
  Test-only host for `AttachEmailCheckbox`: builds the state with `createAttachEmail()`
  during component init, exactly as the three report dialogs do, so the `$effect` that
  follows `analytics.email` is owned by a real component and torn down on unmount.
-->
<script lang="ts">
    import AttachEmailCheckbox from './AttachEmailCheckbox.svelte'
    import { createAttachEmail, type AttachEmail } from './attach-email.svelte'

    interface Props {
        /** Handed the state so a test can read the tick and the typed address back. */
        onState?: (email: AttachEmail) => void
    }

    const { onState }: Props = $props()

    const email = createAttachEmail()
    onState?.(email)
</script>

<AttachEmailCheckbox {email} />
