<script lang="ts">
    import { onMount, onDestroy } from 'svelte'
    import SettingsSection from '../components/SettingsSection.svelte'
    import SettingRow from '../components/SettingRow.svelte'
    import SettingToggleGroup from '../components/SettingToggleGroup.svelte'
    import { getSettingDefinition, onSpecificSettingChange, getSetting } from '$lib/settings'
    import { createShouldShow } from '$lib/settings/settings-search'
    import { getAppLogger } from '$lib/logging/logger'

    const log = getAppLogger('settings')

    interface Props {
        searchQuery: string
    }

    const { searchQuery }: Props = $props()

    const shouldShow = $derived(createShouldShow(searchQuery))

    const themeModeDef = getSettingDefinition('theme.mode') ?? { label: '', description: '' }

    let unsubscribe: (() => void) | undefined

    async function applyTheme(mode: string) {
        log.debug('Applying theme: {mode}', { mode })
        try {
            const { setTheme } = await import('@tauri-apps/api/app')
            if (mode === 'system') {
                // Setting null lets Tauri follow system preference
                await setTheme(null)
            } else {
                await setTheme(mode as 'light' | 'dark')
            }
            log.info('Theme applied: {mode}', { mode })
        } catch (error) {
            log.error('Failed to apply theme: {error}', { error })
        }
    }

    onMount(() => {
        // Apply current theme on mount (in case it changed while settings were closed)
        const currentTheme = getSetting('theme.mode')
        void applyTheme(currentTheme)

        // Listen for theme changes
        unsubscribe = onSpecificSettingChange('theme.mode', (_id, value) => {
            void applyTheme(value)
        })
    })

    onDestroy(() => {
        unsubscribe?.()
    })
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
