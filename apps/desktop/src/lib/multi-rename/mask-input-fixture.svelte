<!--
  Test-only host for `MaskInput`: owns the mask text, as the sheet does, and stands in for the
  sheet around the field by recording every keydown that bubbles out of it. `source` stands in for
  the sheet's state, which hands the range editor its examples.
-->
<script lang="ts">
    import MaskInput from './MaskInput.svelte'
    import { provideRangeSampleSource, type RangeSampleSource } from './range-sample-source'

    interface Props {
        initial: string
        onValue: (value: string) => void
        /** A key that reached the "sheet": one the field or its editor didn't keep. */
        onOuterKey: (key: string) => void
        source?: RangeSampleSource
    }

    const { initial, onValue, onOuterKey, source }: Props = $props()

    // The fixture only seeds from `initial` and `source`.
    // svelte-ignore state_referenced_locally
    let value = $state(initial)
    // svelte-ignore state_referenced_locally
    if (source) provideRangeSampleSource(source)
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
