<script lang="ts">
    import SettingRow from '../components/SettingRow.svelte'
    import SettingSwitch from '../components/SettingSwitch.svelte'
    import { getSettingDefinition } from '$lib/settings'

    interface Props {
        searchQuery: string
    }

    const { searchQuery }: Props = $props()

    const autoCheckDef = getSettingDefinition('updates.autoCheck')!
</script>

<div class="section">
    <h2 class="section-title">Updates</h2>

    <SettingRow id="updates.autoCheck" label={autoCheckDef.label} description={autoCheckDef.description}>
        <SettingSwitch id="updates.autoCheck" />
    </SettingRow>
</div>

<style>
    .section {
        margin-bottom: var(--spacing-md);
    }

    .section-title {
        font-size: 16px;
        font-weight: 600;
        color: var(--color-text-primary);
        margin: 0 0 var(--spacing-sm);
        padding-bottom: var(--spacing-xs);
        border-bottom: 1px solid var(--color-border);
    }
</style>
