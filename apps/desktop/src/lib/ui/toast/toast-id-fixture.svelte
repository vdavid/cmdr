<!--
  Test-only toast content: shows the `toastId` the toast frame handed it, plus a
  close button wired the way every self-closing toast body is, so a test can see
  whether a component toast raised without `props` still knows its own id.
-->
<script lang="ts">
    import { dismissToast } from './toast-store.svelte'

    interface Props {
        toastId: string
        label?: string
    }

    const { toastId, label = '' }: Props = $props()
</script>

<p data-test="toast-id">{toastId}</p>
<p data-test="toast-label">{label}</p>
<button
    type="button"
    onclick={() => {
        dismissToast(toastId)
    }}>Close</button
>
