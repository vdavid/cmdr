<script lang="ts">
    import SettingsSection from '../components/SettingsSection.svelte'
    import SettingRow from '../components/SettingRow.svelte'
    import SettingToggleGroup from '../components/SettingToggleGroup.svelte'
    import { getSettingDefinition } from '$lib/settings'
    import { createShouldShow } from '$lib/settings/settings-search'

    interface Props {
        searchQuery: string
    }

    const { searchQuery }: Props = $props()

    const shouldShow = $derived(createShouldShow(searchQuery))

    const themeModeDef = getSettingDefinition('theme.mode') ?? { label: '', description: '' }

    // Theme application lives in `settings-applier.ts` — it runs at every window's
    // startup, so the persisted choice takes effect on cold launches and not just
    // when this section happens to mount. Toggling the radio fires `setSetting`,
    // which the applier subscribes to.
</script>

<SettingsSection title="Themes">
    {#if shouldShow('theme.mode')}
        <SettingRow id="theme.mode" label={themeModeDef.label} description={themeModeDef.description} {searchQuery}>
            <SettingToggleGroup id="theme.mode" />
        </SettingRow>
    {/if}

    {#if !searchQuery.trim()}
        <!-- Future: Preset themes -->
        <div class="coming-soon">
            <h3>Preset themes</h3>
            <p>Custom color themes are coming in a future update.</p>
        </div>

        <!-- Future: Custom theme editor -->
        <div class="coming-soon">
            <h3>Custom theme editor</h3>
            <p>Create and customize your own color schemes. Coming soon!</p>
        </div>
    {/if}
</SettingsSection>

<style>
    .coming-soon {
        padding: var(--spacing-lg);
        background: var(--color-bg-secondary);
        border-radius: var(--radius-md);
        margin-top: var(--spacing-lg);
    }

    .coming-soon h3 {
        font-size: var(--font-size-md);
        font-weight: 600;
        color: var(--color-text-secondary);
        margin: 0 0 var(--spacing-xs);
    }

    .coming-soon p {
        font-size: var(--font-size-sm);
        color: var(--color-text-tertiary);
        margin: 0;
    }
</style>
