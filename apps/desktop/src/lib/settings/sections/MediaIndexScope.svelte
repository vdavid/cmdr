<script lang="ts">
    /**
     * The image-indexing SCOPE control: which folders indexing may cover. Rendered inside
     * the "Image indexing" card in `ImageIndexingSection.svelte` once the master
     * `mediaIndex.enabled` toggle is on, above the chosen-folders list.
     *
     * Two modes, and the default is the narrow one: index only the folders you name. The
     * automatic mode (rank folders by importance and index everything above the slider) is
     * the opt-in, and the slider only exists inside it — so the panel never shows a control
     * that doesn't apply.
     *
     * Switching to the narrow mode indexes less from now on but deletes nothing: the rows
     * already indexed stay searchable and turn up as the reclaim offer. That offer normally
     * rides inside the slider, so this component hosts its own `MediaIndexReclaim` in the
     * narrow mode — narrowing is exactly when there's something to reclaim, and losing the
     * offer along with the slider would strand the disk space with no way to free it.
     * Exactly one instance renders either way.
     *
     * The persisted setting live-applies to the backend scheduler via `settings-applier.ts`
     * (the `mediaIndex.enabled` precedent), so the control and the enrichment gate read the
     * SAME scope and can't drift.
     */
    import { onMount } from 'svelte'
    import SettingRadioGroup from '../components/SettingRadioGroup.svelte'
    import { getSetting, onSpecificSettingChange } from '$lib/settings'
    import { tString } from '$lib/intl/messages.svelte'
    import MediaIndexImportanceSlider from './MediaIndexImportanceSlider.svelte'
    import MediaIndexReclaim from './MediaIndexReclaim.svelte'

    // Live scope, so the slider appears/disappears the moment the mode changes (no restart,
    // matching the live-apply rule) and stays in sync across windows.
    let scope = $state(getSetting('mediaIndex.scope'))
    // The persisted threshold, for the narrow mode's reclaim call. The backend ignores it
    // there (the partition is override-only), but the command signature still takes it, and
    // passing the real persisted value keeps it honest if the user switches back.
    let threshold = $state(getSetting('mediaIndex.importanceThreshold'))

    onMount(() => {
        const unsubScope = onSpecificSettingChange('mediaIndex.scope', (_id, value) => (scope = value))
        const unsubThreshold = onSpecificSettingChange(
            'mediaIndex.importanceThreshold',
            (_id, value) => (threshold = value),
        )
        return () => {
            unsubScope()
            unsubThreshold()
        }
    })
</script>

<div class="mi-scope">
    <h4 class="mi-scope-title">{tString('settings.mediaIndex.scope.label')}</h4>
    <p class="mi-scope-help">{tString('settings.mediaIndex.scope.description')}</p>
    <SettingRadioGroup id="mediaIndex.scope" />

    <!-- The importance slider belongs to the automatic scope alone: in the narrow one it
         has no effect at all, so showing it would promise a control that does nothing. -->
    {#if scope === 'importance'}
        <MediaIndexImportanceSlider />
    {:else}
        <!-- The slider hosts the reclaim offer in the automatic mode; here it has no slider
             to ride, and narrowing is precisely when there are leftover rows to free. Not
             `blocked`: the narrow partition needs no importance, so the component's own
             `pending` guard is the only wait that applies. -->
        <MediaIndexReclaim {threshold} blocked={false} />
    {/if}
</div>

<style>
    .mi-scope {
        padding: var(--spacing-sm) 0 var(--spacing-xs);
    }

    .mi-scope-title {
        margin: 0;
        font-size: var(--font-size-md);
        font-weight: 600;
        color: var(--color-text-primary);
    }

    .mi-scope-help {
        margin: var(--spacing-xxs) 0 var(--spacing-sm);
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
        line-height: 1.4;
    }
</style>
