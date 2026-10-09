<!--
  Test-only host for `MaskInput`: owns the mask text, as the sheet does, and stands in for the
  sheet around the field by recording every keydown that bubbles out of it.
-->
<script lang="ts">
    import MaskInput from './MaskInput.svelte'

    interface Props {
        initial: string
        onValue: (value: string) => void
        /** A key that reached the "sheet": one the field or its editor didn't keep. */
        onOuterKey: (key: string) => void
    }

    const { initial, onValue, onOuterKey }: Props = $props()

    // The fixture only seeds from `initial`.
    // svelte-ignore state_referenced_locally
    let value = $state(initial)
</script>

<svelte:window
    onkeydown={(e: KeyboardEvent) => {
        onOuterKey(e.key)
    }}
/>

<MaskInput
    {value}
    onValueChange={(next: string) => {
        value = next
        onValue(next)
    }}
    ariaLabel="Name"
/>
