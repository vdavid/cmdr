<!--
  The `Indexing › Image indexing` subsection. On-device image-content (OCR) search reads the
  text inside your images so you can search it, running entirely on the Mac via Apple's Vision
  framework; semantic search adds "find a photo by describing it" via an on-device CLIP model.
  No cloud provider, no API key.

  Three cards (grouping is section-owned; the registry `section`/`cardKey` stay
  `['Indexing','Image indexing']`, so there's no new sidebar route):

    1. Enable indexing — the master `mediaIndex.enabled` toggle, the privacy note, the live
       per-drive progress summary (shown while a pass runs), and the `showFileStatusIcons`
       display toggle.
    2. Folders to index — everything answering "what gets indexed": the scope control (with
       its importance slider + reclaim), the chosen-folders list with per-folder coverage, and
       the per-network-volume opt-in.
    3. Semantic search — the CLIP model on/off toggle, download, and delete.

  Everything below the master toggle only means anything once indexing is on, so cards 2 and 3
  gate on the live master toggle (no restart, matching live-apply). Each card frame is wrapped
  in `anyVisible(shouldShow, ...)` over the SAME predicate that gates its rows, so a search that
  filters everything out leaves no empty card.
-->
<script lang="ts">
    import { onMount } from 'svelte'
    import SettingsSection from '../components/SettingsSection.svelte'
    import SettingRow from '../components/SettingRow.svelte'
    import SettingSwitch from '../components/SettingSwitch.svelte'
    import SettingSlider from '../components/SettingSlider.svelte'
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import StatusBadge from '$lib/ui/StatusBadge.svelte'
    import { getBadgeStatus } from '$lib/feature-status'
    import MediaIndexScope from './MediaIndexScope.svelte'
    import MediaIndexChosenFolders from './MediaIndexChosenFolders.svelte'
    import MediaIndexNetworkVolumes from './MediaIndexNetworkVolumes.svelte'
    import MediaIndexClipModel from './MediaIndexClipModel.svelte'
    import MediaIndexProgressSummary from './MediaIndexProgressSummary.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import { getSetting, getSettingDefinition, onSpecificSettingChange } from '$lib/settings'
    import { createShouldShow, anyVisible } from '$lib/settings/settings-search'
    import { getMediaIndexMaxParallelism } from '$lib/tauri-commands'

    interface Props {
        searchQuery: string
    }

    const { searchQuery }: Props = $props()

    const shouldShow = $derived(createShouldShow(searchQuery))

    const imageIndexDef = getSettingDefinition('mediaIndex.enabled') ?? { label: '', description: '' }
    const showFileStatusIconsDef = getSettingDefinition('mediaIndex.showFileStatusIcons') ?? {
        label: '',
        description: '',
    }
    const parallelismDef = getSettingDefinition('mediaIndex.parallelism') ?? { label: '', description: '' }
    const imageSearchBadge = getBadgeStatus('image-search')

    // Live master-toggle state, so cards 2 and 3 appear/disappear the moment the user flips
    // "Index image contents" (no restart, matching the live-apply rule).
    let imageIndexEnabled = $state(getSetting('mediaIndex.enabled'))

    // The parallelism slider's max is this machine's CPU count, known only at runtime. Seed
    // from the registry's static fallback, then fetch the real ceiling on mount. The backend
    // clamps independently, so a brief stale max can't over-provision.
    let maxParallelism = $state(getSettingDefinition('mediaIndex.parallelism')?.constraints?.max ?? 16)

    onMount(() => {
        const unsub = onSpecificSettingChange('mediaIndex.enabled', (_id, value) => (imageIndexEnabled = value))
        void getMediaIndexMaxParallelism()
            .then((n) => (maxParallelism = n))
            .catch(() => {
                /* keep the fallback max; the backend clamps regardless */
            })
        return unsub
    })

    // Card frames: each is the SAME `shouldShow` predicate that gates its rows.
    const showEnableCard = $derived(
        anyVisible(shouldShow, 'mediaIndex.enabled', 'mediaIndex.showFileStatusIcons', 'mediaIndex.parallelism'),
    )
    const showFoldersCard = $derived(
        imageIndexEnabled && anyVisible(shouldShow, 'mediaIndex.enabled', 'mediaIndex.scope', 'mediaIndex.importanceThreshold'),
    )
    const showSemanticCard = $derived(
        imageIndexEnabled && anyVisible(shouldShow, 'mediaIndex.enabled', 'mediaIndex.semanticSearch.enabled'),
    )
</script>

<SettingsSection title={tString('settings.section.imageIndexing')}>
    <!-- Card 1: Enable indexing -->
    {#if showEnableCard}
        <SectionCard label={tString('settings.mediaIndex.cards.enable')}>
            {#snippet badge()}
                {#if imageSearchBadge}<StatusBadge status={imageSearchBadge} />{/if}
            {/snippet}
            {#if shouldShow('mediaIndex.enabled')}
                <SettingRow
                    id="mediaIndex.enabled"
                    label={imageIndexDef.label}
                    description={imageIndexDef.description}
                    {searchQuery}
                >
                    <SettingSwitch id="mediaIndex.enabled" />
                </SettingRow>

                <!-- Privacy posture, spelled out because this feature touches no provider:
                     on-device via Apple's Vision framework, nothing leaves the machine. -->
                <p class="privacy-note">{tString('settings.mediaIndex.privacyNote')}</p>

                <!-- Live per-drive progress (reuses the top-right hourglass's row), shown
                     only while a pass is running. -->
                {#if imageIndexEnabled}
                    <MediaIndexProgressSummary />
                {/if}
            {/if}

            <!-- Whether the file list draws the small per-file image-index status badge.
                 Only meaningful once indexing is on, so gate on the live master toggle. -->
            {#if imageIndexEnabled && shouldShow('mediaIndex.showFileStatusIcons')}
                <SettingRow
                    id="mediaIndex.showFileStatusIcons"
                    label={showFileStatusIconsDef.label}
                    description={showFileStatusIconsDef.description}
                    {searchQuery}
                >
                    <SettingSwitch id="mediaIndex.showFileStatusIcons" />
                </SettingRow>
            {/if}

            <!-- How many parallel workers indexing runs. Only meaningful once indexing is on,
                 so gate on the live master toggle. The slider's max is this machine's CPU
                 count, fetched on mount; default 1 is today's single worker. -->
            {#if imageIndexEnabled && shouldShow('mediaIndex.parallelism')}
                <SettingRow
                    id="mediaIndex.parallelism"
                    label={parallelismDef.label}
                    description={parallelismDef.description}
                    {searchQuery}
                >
                    <SettingSlider id="mediaIndex.parallelism" maxOverride={maxParallelism} />
                </SettingRow>
            {/if}
        </SectionCard>
    {/if}

    <!-- Card 2: Folders to index — what gets indexed. -->
    {#if showFoldersCard}
        <SectionCard label={tString('settings.mediaIndex.cards.folders')}>
            <!-- Scope ("which folders?") hosts the importance slider (automatic scope only)
                 and the reclaim offer; the chosen-folders list shows per-folder coverage. -->
            <MediaIndexScope />
            <MediaIndexChosenFolders />
            <!-- Per-network-volume opt-in: about which SOURCES get indexed, so it lives here
                 (not under semantic search). -->
            <MediaIndexNetworkVolumes />
        </SectionCard>
    {/if}

    <!-- Card 3: Semantic search — the on-device CLIP model on/off, download, and delete. -->
    {#if showSemanticCard}
        <SectionCard label={tString('settings.mediaIndex.clip.title')}>
            <MediaIndexClipModel {searchQuery} />
        </SectionCard>
    {/if}
</SettingsSection>

<style>
    .privacy-note {
        margin: var(--spacing-xs) 0 var(--spacing-sm);
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
        line-height: 1.4;
    }
</style>
