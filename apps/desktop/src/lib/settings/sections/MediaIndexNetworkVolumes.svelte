<script lang="ts">
    /**
     * Per-network-volume image-enrichment controls (plan M1.5), rendered inside the
     * "Image search" card in `FileSystemWatchingSection.svelte` (only when the master
     * `mediaIndex.enabled` toggle is on). For each mounted network (SMB) volume:
     *
     *   - an opt-in Switch ("Index photos on this drive") — off by default, because
     *     turning on image indexing does NOT auto-enrich network drives (they're read
     *     over the wire, so we stay conservative). Honest copy: reads photos only when
     *     the app is idle, throttled, and pauses when the drive disconnects.
     *   - once opted in, an "Always index this drive" Switch for the NAS-archive case
     *     (a rarely-browsed drive scores low on navigation importance, so without this
     *     its photos would defer forever), plus a live status line (indexing now /
     *     paused because disconnected / how many photos are indexed).
     *
     * The opt-in / always-index switches are driven by the persisted settings (the
     * source of truth, live-synced cross-window); the status line is driven by a polled
     * `mediaIndexVolumeState` snapshot. Local volumes never appear here — they enrich by
     * default when the master toggle is on.
     */
    import { onMount } from 'svelte'
    import { SvelteMap } from 'svelte/reactivity'
    import { Switch } from '@ark-ui/svelte/switch'
    import { getVolumes } from '$lib/stores/volume-store.svelte'
    import type { VolumeInfo } from '$lib/file-explorer/types'
    import { tString } from '$lib/intl/messages.svelte'
    import { formatInteger } from '$lib/intl/number-format'
    import Spinner from '$lib/ui/Spinner.svelte'
    import { getAppLogger } from '$lib/logging/logger'
    import { onSpecificSettingChange } from '$lib/settings'
    import { mediaIndexVolumeState, type MediaIndexVolumeState } from '$lib/tauri-commands'
    import {
        isNetworkVolumeOptedIn,
        isVolumeAlwaysIndexed,
        setNetworkVolumeOptedIn,
        setVolumeAlwaysIndexed,
    } from '$lib/media-index/network-volume-prefs'

    const log = getAppLogger('media-index')

    // The mounted network (SMB) volumes, reactive off the shared volume store.
    const networkVolumes = $derived(getVolumes().filter((v: VolumeInfo) => v.category === 'network'))

    // Opt-in / always-index reflect the persisted settings (source of truth). Reactive maps
    // seeded on mount and updated on cross-window setting changes; mutated in place.
    const optIn = new SvelteMap<string, boolean>()
    const always = new SvelteMap<string, boolean>()
    // Per-volume backend snapshot (indexing / paused / enriched count), polled while open.
    // A map so a missing entry reads as `undefined` (honest "no snapshot yet").
    const states = new SvelteMap<string, MediaIndexVolumeState>()

    function reseedFromSettings(): void {
        for (const v of networkVolumes) {
            optIn.set(v.id, isNetworkVolumeOptedIn(v.id))
            always.set(v.id, isVolumeAlwaysIndexed(v.id))
        }
    }

    async function refreshStates(): Promise<void> {
        const results = await Promise.all(
            networkVolumes.map(async (v) => {
                try {
                    return [v.id, await mediaIndexVolumeState(v.id)] as const
                } catch {
                    return [v.id, null] as const
                }
            }),
        )
        for (const [id, state] of results) {
            if (state !== null) states.set(id, state)
        }
    }

    async function handleOptInChange(volumeId: string, enabled: boolean): Promise<void> {
        optIn.set(volumeId, enabled)
        try {
            await setNetworkVolumeOptedIn(volumeId, enabled)
        } catch {
            optIn.set(volumeId, !enabled)
        }
        await refreshStates()
    }

    async function handleAlwaysChange(volumeId: string, next: boolean): Promise<void> {
        always.set(volumeId, next)
        try {
            await setVolumeAlwaysIndexed(volumeId, next)
        } catch {
            always.set(volumeId, !next)
        }
        await refreshStates()
    }

    onMount(() => {
        reseedFromSettings()
        void refreshStates()
        // Keep the switches in sync when the same settings change in another window.
        const unsubOptIn = onSpecificSettingChange('mediaIndex.networkVolumes', () => { reseedFromSettings(); })
        const unsubAlways = onSpecificSettingChange('mediaIndex.alwaysIndexVolumes', () => { reseedFromSettings(); })
        // Light poll so "indexing" / "paused" / count stay honest while Settings is open.
        const timer = setInterval(() => void refreshStates(), 3000)
        return () => {
            unsubOptIn()
            unsubAlways()
            clearInterval(timer)
        }
    })

    // Re-seed the switch maps whenever the mounted network-volume set changes.
    $effect(() => {
        if (networkVolumes.length > 0 && optIn.size === 0) {
            reseedFromSettings()
            void refreshStates()
        }
    })

    function statusLine(volumeId: string): string | null {
        const state = states.get(volumeId)
        if (!state) return null
        if (state.paused) return tString('settings.mediaIndex.networkVolumes.paused')
        // Honest "N of M" once the drive index knows the qualifying total; while that total is
        // still unknown (scanning / offline) fall back to a plain count or "counting…".
        if (state.qualifyingCount === null) {
            if (state.indexing && state.enrichedCount === 0) {
                return tString('settings.mediaIndex.progress.counting')
            }
            if (state.enrichedCount > 0) {
                return tString('settings.mediaIndex.networkVolumes.indexed', {
                    countText: formatInteger(state.enrichedCount),
                    count: state.enrichedCount,
                })
            }
            return tString('settings.mediaIndex.networkVolumes.notIndexedYet')
        }
        if (state.qualifyingCount === 0) return tString('settings.mediaIndex.networkVolumes.notIndexedYet')
        if (state.enrichedCount >= state.qualifyingCount) {
            return tString('settings.mediaIndex.progress.done', {
                total: state.qualifyingCount,
                totalText: formatInteger(state.qualifyingCount),
            })
        }
        return tString('settings.mediaIndex.progress.ofTotal', {
            total: state.qualifyingCount,
            enrichedText: formatInteger(state.enrichedCount),
            totalText: formatInteger(state.qualifyingCount),
        })
    }
</script>

<div class="net-vols">
    <p class="net-intro">{tString('settings.mediaIndex.networkVolumes.intro')}</p>

    {#if networkVolumes.length === 0}
        <p class="net-empty">{tString('settings.mediaIndex.networkVolumes.none')}</p>
    {:else}
        <ul class="net-list" role="list">
            {#each networkVolumes as volume (volume.id)}
                {@const isOn = optIn.get(volume.id) ?? false}
                {@const state = states.get(volume.id)}
                <li class="net-item">
                    <div class="net-row">
                        <div class="net-labels">
                            <span class="net-name">{volume.name}</span>
                            {#if isOn}
                                {@const line = statusLine(volume.id)}
                                {#if line}
                                    <span class="net-status" class:net-status-paused={state?.paused}>
                                        {#if state?.indexing && !state.paused}
                                            <Spinner size="sm" />
                                        {/if}
                                        {line}
                                    </span>
                                {/if}
                            {/if}
                        </div>
                        <Switch.Root
                            checked={isOn}
                            onCheckedChange={(d) => {
                                void handleOptInChange(volume.id, d.checked).catch((err: unknown) => {
                                    log.warn('opt-in toggle failed: {err}', { err: String(err) })
                                })
                            }}
                            aria-label={tString('settings.mediaIndex.networkVolumes.optInLabel', {
                                name: volume.name,
                            })}
                        >
                            <Switch.Control class="mi-switch-control">
                                <Switch.Thumb class="mi-switch-thumb" />
                            </Switch.Control>
                            <Switch.HiddenInput data-test="media-net-optin" data-volume-id={volume.id} />
                        </Switch.Root>
                    </div>

                    {#if isOn}
                        <p class="net-help">{tString('settings.mediaIndex.networkVolumes.optInHelp')}</p>
                        <div class="net-row net-subrow">
                            <div class="net-labels">
                                <span class="net-sublabel"
                                    >{tString('settings.mediaIndex.networkVolumes.alwaysLabel')}</span
                                >
                                <span class="net-help net-help-inline"
                                    >{tString('settings.mediaIndex.networkVolumes.alwaysHelp')}</span
                                >
                            </div>
                            <Switch.Root
                                checked={always.get(volume.id) ?? false}
                                onCheckedChange={(d) => {
                                    void handleAlwaysChange(volume.id, d.checked).catch((err: unknown) => {
                                        log.warn('always-index toggle failed: {err}', { err: String(err) })
                                    })
                                }}
                                aria-label={tString('settings.mediaIndex.networkVolumes.alwaysAria', {
                                    name: volume.name,
                                })}
                            >
                                <Switch.Control class="mi-switch-control">
                                    <Switch.Thumb class="mi-switch-thumb" />
                                </Switch.Control>
                                <Switch.HiddenInput data-test="media-net-always" data-volume-id={volume.id} />
                            </Switch.Root>
                        </div>
                    {/if}
                </li>
            {/each}
        </ul>
    {/if}
</div>

<style>
    .net-vols {
        padding: var(--spacing-sm) 0 var(--spacing-xs);
    }

    .net-intro,
    .net-empty {
        margin: 0 0 var(--spacing-sm);
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
        line-height: 1.4;
    }

    .net-empty {
        color: var(--color-text-tertiary);
    }

    .net-list {
        list-style: none;
        margin: 0;
        padding: 0;
        display: flex;
        flex-direction: column;
        gap: var(--spacing-sm);
    }

    .net-item {
        padding: var(--spacing-sm);
        border: 1px solid var(--color-border-subtle);
        border-radius: var(--radius-md);
        background: var(--color-bg-secondary);
    }

    .net-row {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: var(--spacing-md);
    }

    .net-subrow {
        margin-top: var(--spacing-sm);
        padding-top: var(--spacing-sm);
        border-top: 1px solid var(--color-border-subtle);
    }

    .net-labels {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xxs);
        min-width: 0;
    }

    .net-name {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-xs);
        font-weight: 500;
        color: var(--color-text-primary);
    }

    .net-sublabel {
        font-weight: 500;
        color: var(--color-text-primary);
        font-size: var(--font-size-sm);
    }

    .net-status {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-xs);
        font-size: var(--font-size-sm);
        color: var(--color-text-tertiary);
    }

    .net-status-paused {
        color: var(--color-warning-text);
    }

    .net-help {
        margin: var(--spacing-xs) 0 0;
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
        line-height: 1.4;
    }

    .net-help-inline {
        margin: 0;
        color: var(--color-text-tertiary);
    }

    /* Switch styling mirrors `SettingSwitch.svelte`; scoped class names keep the
       rules local to this dynamic per-volume list (it can't use the registry
       `SettingSwitch`, which is keyed to a single setting id). */
    :global(.mi-switch-control) {
        display: inline-flex;
        align-items: center;
        width: 36px;
        height: 20px;
        background: var(--color-bg-tertiary);
        border-radius: var(--radius-full);
        padding: var(--spacing-xxs);
        cursor: default;
        transition: background-color var(--transition-base);
        flex-shrink: 0;
    }

    :global(.mi-switch-control[data-state='checked']) {
        background: var(--color-accent);
    }

    :global(.mi-switch-control[data-state='checked']:hover) {
        background: var(--color-accent-hover);
    }

    :global(.mi-switch-thumb) {
        width: 16px;
        height: 16px;
        background: white;
        border-radius: var(--radius-full);
        transition: transform var(--transition-base);
        box-shadow: var(--shadow-sm);
    }

    :global(.mi-switch-control[data-state='checked'] .mi-switch-thumb) {
        transform: translateX(16px);
    }

    :global(.mi-switch-control[data-focus]) {
        outline: 2px solid var(--color-accent);
        outline-offset: 2px;
        box-shadow: var(--shadow-focus);
    }
</style>
