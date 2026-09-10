<!--
  Test-only host for `src/lib/ui/portal-target.test.ts`: a `Select` and a `Combobox`
  in a `ModalDialog`'s body, exactly where a real dialog carries them. The parent
  test asserts where each open menu mounts.
-->
<script lang="ts">
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import Select from '$lib/ui/Select.svelte'
    import Combobox from '$lib/ui/Combobox.svelte'
</script>

<ModalDialog titleId="portal-target-fixture-title">
    {#snippet title()}Fixture{/snippet}
    <Select
        items={[
            { value: 'a', label: 'A' },
            { value: 'b', label: 'B' },
        ]}
        value="a"
        onChange={() => {}}
        ariaLabel="Pick one"
    />
    <Combobox items={[{ value: 'x', label: 'X' }]} inputValue="" onInputValueChange={() => {}} ariaLabel="Type one" />
</ModalDialog>
