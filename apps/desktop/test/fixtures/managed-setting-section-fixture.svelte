<!--
  Test-only host for `src/lib/settings/components/managed-lock.test.ts`: one switch row inside a
  settings section, exactly how a section composes them. The parent test asserts how the row and
  the section render a setting the organization manages.
-->
<script lang="ts">
    import SettingsSection from '$lib/settings/components/SettingsSection.svelte'
    import SettingRow from '$lib/settings/components/SettingRow.svelte'
    import SettingSwitch from '$lib/settings/components/SettingSwitch.svelte'
    import type { SettingId } from '$lib/settings'

    interface Props {
        id: SettingId
        disabled?: boolean
        disabledNote?: string
        disabledReason?: string
    }

    const { id, disabled = false, disabledNote, disabledReason }: Props = $props()
</script>

<SettingsSection title="Updates & privacy">
    <SettingRow {id} label="Share usage stats" description="Helps make Cmdr better." {disabled} {disabledNote} {disabledReason}>
        <SettingSwitch {id} {disabled} />
    </SettingRow>
</SettingsSection>
