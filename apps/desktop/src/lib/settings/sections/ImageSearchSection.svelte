<!--
  The `AI › Image search` subsection (third card under the AI card-menu parent). On-device
  image-content (OCR) search: it reads the text inside your images so you can search it, running
  entirely on the Mac via Apple's Vision framework. It lives under AI but is provider-independent
  — no cloud, no API key — so it carries an explicit privacy note, since the rest of the AI
  section otherwise implies the configured provider.

  Composes the three self-contained media-index components: the master `mediaIndex.enabled`
  toggle (its own card, titled by `cardKey`), the importance-threshold slider
  (`MediaIndexImportanceSlider`, which itself hosts `MediaIndexReclaim`), and the per-network-
  volume opt-in list (`MediaIndexNetworkVolumes`). The slider and network list only mean anything
  once indexing is on, so both gate on the live master toggle (no restart — matches live-apply).
-->
<script lang="ts">
    import { onMount } from 'svelte'
    import SettingsSection from '../components/SettingsSection.svelte'
    import SettingRow from '../components/SettingRow.svelte'
    import SettingSwitch from '../components/SettingSwitch.svelte'
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import StatusBadge from '$lib/ui/StatusBadge.svelte'
    import { getBadgeStatus } from '$lib/feature-status'
    import MediaIndexImportanceSlider from './MediaIndexImportanceSlider.svelte'
    import MediaIndexNetworkVolumes from './MediaIndexNetworkVolumes.svelte'
    import MediaIndexClipModel from './MediaIndexClipModel.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import { getSetting, getSettingDefinition, onSpecificSettingChange } from '$lib/settings'
    import { createShouldShow } from '$lib/settings/settings-search'

    interface Props {
        searchQuery: string
    }

    const { searchQuery }: Props = $props()

    const shouldShow = $derived(createShouldShow(searchQuery))

    const imageIndexDef = getSettingDefinition('mediaIndex.enabled') ?? { label: '', description: '' }
    const imageSearchBadge = getBadgeStatus('image-search')

    // Live master-toggle state, so the slider and per-network-volume controls appear/disappear the
    // moment the user flips "Index image contents" (no restart, matches the live-apply rule).
    let imageIndexEnabled = $state(getSetting('mediaIndex.enabled'))

    onMount(() => {
        // Track the master toggle so the refining controls reveal live (the toggle applies in
        // this same window before this section re-reads it).
        const unsubImageIndex = onSpecificSettingChange('mediaIndex.enabled', (_id, value) => {
            imageIndexEnabled = value
        })
        return () => {
            unsubImageIndex()
        }
    })
</script>

<SettingsSection title={tString('settings.section.imageSearch')}>
    {#if shouldShow('mediaIndex.enabled')}
        <SectionCard label={tString('settings.mediaIndex.card')}>
            {#snippet badge()}
                {#if imageSearchBadge}<StatusBadge status={imageSearchBadge} />{/if}
            {/snippet}
            <SettingRow
                id="mediaIndex.enabled"
                label={imageIndexDef.label}
                description={imageIndexDef.description}
                {searchQuery}
            >
                <SettingSwitch id="mediaIndex.enabled" />
            </SettingRow>

            <!-- Privacy posture, spelled out because this feature sits under AI yet touches no
                 provider: on-device via Apple's Vision framework, nothing leaves the machine. -->
            <p class="privacy-note">{tString('settings.mediaIndex.privacyNote')}</p>

            <!-- The importance-threshold slider ("how deep do I index?") + its per-volume
                 progress and reclaim line. Refines the master toggle, so it's only shown when
                 indexing is on. -->
            {#if imageIndexEnabled}
                <MediaIndexImportanceSlider />
            {/if}

            <!-- The on-device CLIP model for natural-language semantic search (plan M3).
                 Self-gates on Apple Silicon + shows its own download state. -->
            {#if imageIndexEnabled}
                <MediaIndexClipModel />
            {/if}

            <!-- Per-network-volume opt-in + "always index" overrides (network enrichment). Only
                 meaningful once image indexing is on, so gate on the live master toggle. -->
            {#if imageIndexEnabled}
                <MediaIndexNetworkVolumes />
            {/if}
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
