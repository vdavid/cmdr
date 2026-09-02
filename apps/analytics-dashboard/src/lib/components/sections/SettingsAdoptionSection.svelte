<!--
  Product § Settings adoption: what installs actually run, once an absent config key has been
  resolved against the defaults that shipped in that install's app version.
-->
<script lang="ts">
    import type { SourceResult, DashboardSelection } from '$lib/server/types.js'
    import type { SettingsAdoption } from '$lib/server/settings-defaults.js'
    import { formatNumber } from '$lib/format.js'
    import { COLOR_GOLD, COLOR_GREEN, COLOR_CYAN } from '$lib/colors.js'
    import {
        formatShare,
        formatShareUnlike,
        formatValueShare,
        mostChanged,
        settingByKey,
        shareOnDefault,
        topOverride,
        unchangedNote,
    } from '$lib/settings-adoption.js'
    import ErrorState from '$lib/components/ErrorState.svelte'
    import MetricRow from '$lib/components/MetricRow.svelte'
    import Methodology from '$lib/components/Methodology.svelte'
    import SectionDescription from '$lib/components/SectionDescription.svelte'
    import BetaEmptyState from '$lib/components/BetaEmptyState.svelte'

    const {
        settingsAdoption,
        selection,
    }: {
        settingsAdoption: SourceResult<SettingsAdoption>
        selection: DashboardSelection
    } = $props()
</script>

<section class="rounded-xl border border-border bg-surface p-6">
    <div class="mb-1">
        <h2 class="text-lg font-semibold text-text-primary">Settings adoption</h2>
        <p class="text-sm text-text-tertiary">What do people actually turn on?</p>
    </div>
    <SectionDescription
        insight="Use this to answer 'how many people use X', which the raw heartbeat can't: the app saves only settings someone changed, so an absent key means either 'still on the default' or 'this build didn't have that setting'."
        caveat={'Each install counts once, at its latest heartbeat. Every number resolves against the defaults that ' +
            "shipped in that install's version, and an install whose build predates a setting is left out of that " +
            "setting's total, so a young setting shows a smaller total than an old one. That's the honest denominator, " +
            'not a gap.'}
    />

    {#if settingsAdoption.ok}
        {@const data = settingsAdoption.data}
        {@const readable = data.totalInstalls - data.unresolvedInstalls}
        {@const indexing = settingByKey(data.settings, 'indexing.enabled')}
        {@const mediaIndex = settingByKey(data.settings, 'mediaIndex.enabled')}
        {@const ai = settingByKey(data.settings, 'ai.provider')}
        {@const changed = mostChanged(data.settings)}

        {#if readable > 0}
            <MetricRow
                metrics={[
                    { label: 'Installs we can read', value: formatNumber(readable), color: COLOR_GOLD },
                    { label: 'Drive indexing on', value: formatValueShare(indexing, 'on'), color: COLOR_GREEN },
                    { label: 'Image search on', value: formatValueShare(mediaIndex, 'on') },
                    { label: 'AI switched on', value: formatShareUnlike(ai, 'off'), color: COLOR_CYAN },
                ]}
            />

            {#if data.unresolvedInstalls > 0}
                <p class="mt-3 text-xs text-text-tertiary">
                    {formatNumber(data.unresolvedInstalls)} more
                    {data.unresolvedInstalls === 1 ? 'install runs' : 'installs run'} a version older than the settings
                    history goes back, so nothing here counts them.
                </p>
            {/if}

            <div class="mt-6 border-t border-border-subtle pt-5">
                <h3 class="mb-2 text-sm font-medium text-text-secondary">Settings people change</h3>
                {#if changed.length > 0}
                    <div class="overflow-x-auto">
                        <table class="w-full text-left text-sm">
                            <thead>
                                <tr class="border-b border-border-subtle text-text-tertiary">
                                    <th class="pr-4 pb-2 font-medium">Setting</th>
                                    <th class="pr-4 pb-2 font-medium">Default</th>
                                    <th class="pr-4 pb-2 text-right font-medium">Installs</th>
                                    <th class="pr-4 pb-2 text-right font-medium">On default</th>
                                    <th class="pb-2 font-medium">Most common change</th>
                                </tr>
                            </thead>
                            <tbody>
                                {#each changed as setting (setting.key)}
                                    {@const share = shareOnDefault(setting)}
                                    {@const override = topOverride(setting)}
                                    <tr class="border-b border-border-subtle/50">
                                        <td class="py-1.5 pr-4 font-mono text-xs text-text-primary">{setting.key}</td>
                                        <td class="py-1.5 pr-4 text-text-secondary">{setting.defaultLabel ?? 'changed by version'}</td>
                                        <td class="py-1.5 pr-4 text-right text-text-secondary tabular-nums">
                                            {formatNumber(setting.eligible)}
                                        </td>
                                        <td class="py-1.5 pr-4 text-right text-text-secondary tabular-nums">
                                            {share === null ? '–' : formatShare(share)}
                                        </td>
                                        <td class="py-1.5 text-text-secondary">
                                            {#if override}
                                                {override.label}
                                                <span class="text-text-tertiary">({formatNumber(override.installs)})</span>
                                            {:else}
                                                –
                                            {/if}
                                        </td>
                                    </tr>
                                {/each}
                            </tbody>
                        </table>
                    </div>
                {:else}
                    <p class="text-sm text-text-tertiary">Nobody has moved a setting off its default yet.</p>
                {/if}
                <Methodology
                    text={'Only settings at least one install has moved appear, most-moved first; the other ' +
                        `${unchangedNote(data.settings.length - changed.length)}. ` +
                        'A value someone set back to the default counts as being on the default, since what matters ' +
                        'is what the app runs with. "Changed by version" means the default itself moved during the ' +
                        "window, so there's no single one to compare against."}
                />
            </div>
        {:else}
            <BetaEmptyState />
        {/if}
    {:else}
        <ErrorState error={settingsAdoption.error} {selection} />
    {/if}
</section>
