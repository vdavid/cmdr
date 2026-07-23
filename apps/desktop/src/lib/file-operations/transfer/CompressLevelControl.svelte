<script lang="ts">
    /**
     * Compression-level control for the Compress mode of the transfer dialog.
     *
     * A thin frame around the shared `SettingSlider`, bound to the SAME
     * `behavior.archiveCompressionLevel` setting the Settings › Behavior ›
     * Archives row uses. Because `SettingSlider` reads and writes purely by id,
     * moving it here persists the value immediately and Settings reflects it
     * live (and vice versa) — there is NO dialog-local level state.
     *
     * Kept out of `TransferDialog.svelte` so that file's line budget stays put.
     */
    import SettingSlider from '$lib/settings/components/SettingSlider.svelte'
    import { tString } from '$lib/intl/messages.svelte'

    const SETTING_ID = 'behavior.archiveCompressionLevel'
</script>

<div class="compress-level">
    <span class="level-title">{tString('settings.archives.compressionLevel.label')}</span>
    <SettingSlider
        id={SETTING_ID}
        endLabels={[
            tString('settings.archives.compressionLevel.faster'),
            tString('settings.archives.compressionLevel.smaller'),
        ]}
    />
    <p class="level-caption">{tString('fileOperations.transferDialog.compressLevelCaption')}</p>
</div>

<style>
    .compress-level {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
    }

    .level-title {
        font-weight: 500;
        color: var(--color-text-primary);
    }

    .level-caption {
        margin: 0;
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
        line-height: 1.4;
    }
</style>
