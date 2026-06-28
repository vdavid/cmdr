<script lang="ts">
    // Dev gallery: the drive-indexing status checklist (`IndexingStatusBody`) in
    // every meaningful state, plus the collapsed multi-drive summary row. The
    // body is purely presentational (explicit `activity` / `aggregation` / `phase`
    // / `isNetwork` props), so each state is just a fixture — no live indexing
    // needed. Each state has a stable anchor id so the i18n screenshot-capture
    // driver can target it; the fixed `NOW` keeps every elapsed clock and ETA
    // deterministic across captures.
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import IndexingStatusBody from '$lib/indexing/IndexingStatusBody.svelte'
    import IndexingDriveSummary from '$lib/indexing/IndexingDriveSummary.svelte'
    import type { VolumeIndexActivity, AggregationActivity } from '$lib/indexing/index-state.svelte'
    import type { ActivityPhase } from '$lib/ipc/bindings'

    // A fixed "now" so elapsed clocks ("· 5:23") and ETAs render identically every
    // capture. All `scanStartedAt` / `startedAt` fixtures are offsets from this.
    const NOW = 1_700_000_000_000

    function scan(overrides: Partial<VolumeIndexActivity> = {}): VolumeIndexActivity {
        return {
            volumeId: 'root',
            phase: 'scanning',
            entriesScanned: 0,
            dirsFound: 0,
            bytesScanned: 0,
            scanStartedAt: 0,
            priorTotalEntries: null,
            priorScanDurationMs: null,
            volumeUsedBytes: null,
            replayEventsProcessed: 0,
            replayEstimatedTotal: 0,
            replayStartedAt: 0,
            ...overrides,
        }
    }

    function agg(phase: string, current: number, total: number, secondsAgo = 45): AggregationActivity {
        return { phase, current, total, startedAt: NOW - secondsAgo * 1000 }
    }

    interface State {
        id: string
        caption: string
        activity: VolumeIndexActivity
        aggregation: AggregationActivity | undefined
        phase: ActivityPhase | undefined
        isNetwork: boolean
        windowedEta: string | null
        driveName: string
    }

    // Each state names the drive it's plausibly about, so the heading reads true.
    const STATES: State[] = [
        {
            id: 'find-files-first',
            caption: 'Local · find files (first scan, no calibration → count + elapsed, no bar)',
            activity: scan({
                entriesScanned: 171_607,
                dirsFound: 16_101,
                bytesScanned: 180_000_000_000,
                scanStartedAt: NOW - 323_000,
                volumeUsedBytes: 420_000_000_000,
            }),
            aggregation: undefined,
            phase: 'scanning',
            isNetwork: false,
            windowedEta: null,
            driveName: 'Macintosh HD',
        },
        {
            id: 'find-files-calibrated',
            caption: 'Local · find files (calibrated rescan → bar + ETA)',
            activity: scan({
                entriesScanned: 672_000,
                dirsFound: 58_300,
                bytesScanned: 120_000_000_000,
                scanStartedAt: NOW - 84_000,
                priorTotalEntries: 1_400_000,
                priorScanDurationMs: 175_000,
            }),
            aggregation: undefined,
            phase: 'scanning',
            isNetwork: false,
            windowedEta: '1m 20s left',
            driveName: 'Macintosh HD',
        },
        {
            id: 'save-file-list',
            caption: 'Local · save the file list (saving entries → bar)',
            activity: scan({ entriesScanned: 1_400_000, dirsFound: 96_400 }),
            aggregation: agg('saving_entries', 480_000, 1_400_000, 60),
            phase: 'aggregating',
            isNetwork: false,
            windowedEta: null,
            driveName: 'Macintosh HD',
        },
        {
            id: 'compute-loading',
            caption: 'Local · compute folder sizes (loading → indeterminate, spinner + sub-line)',
            activity: scan({ entriesScanned: 1_400_000, dirsFound: 96_400 }),
            aggregation: agg('loading', 0, 0, 8),
            phase: 'aggregating',
            isNetwork: false,
            windowedEta: null,
            driveName: 'Macintosh HD',
        },
        {
            id: 'compute-computing',
            caption: 'Local · compute folder sizes (computing → bar + ETA)',
            activity: scan({ entriesScanned: 1_400_000, dirsFound: 96_400 }),
            aggregation: agg('computing', 8_200, 16_101, 40),
            phase: 'aggregating',
            isNetwork: false,
            windowedEta: null,
            driveName: 'Macintosh HD',
        },
        {
            id: 'compute-sorting',
            caption: 'Local · compute folder sizes (sorting → indeterminate, spinner + sub-line)',
            activity: scan({ entriesScanned: 1_400_000, dirsFound: 96_400 }),
            aggregation: agg('sorting', 11_200, 16_101, 18),
            phase: 'aggregating',
            isNetwork: false,
            windowedEta: null,
            driveName: 'Macintosh HD',
        },
        {
            id: 'compute-writing',
            caption: 'Local · compute folder sizes (saving sizes → bar + ETA)',
            activity: scan({ entriesScanned: 1_400_000, dirsFound: 96_400 }),
            aggregation: agg('writing', 14_500, 16_101, 30),
            phase: 'aggregating',
            isNetwork: false,
            windowedEta: null,
            driveName: 'Macintosh HD',
        },
        {
            // Near-complete computing run: the elapsed-extrapolated ETA drops under the
            // 2s "Almost done" threshold (`formatEta`), so this exercises the
            // `indexing.eta.almostDone` string the other tiles never reach.
            id: 'compute-almost-done',
            caption: 'Local · compute folder sizes (almost done → "Almost done" ETA)',
            activity: scan({ entriesScanned: 1_400_000, dirsFound: 96_400 }),
            aggregation: agg('computing', 15_950, 16_101, 40),
            phase: 'aggregating',
            isNetwork: false,
            windowedEta: null,
            driveName: 'Macintosh HD',
        },
        {
            id: 'catch-up',
            caption: 'Local · catch up on recent changes (reconcile → indeterminate, spinner only)',
            activity: scan({ entriesScanned: 1_400_000, dirsFound: 96_400 }),
            aggregation: undefined,
            phase: 'reconciling',
            isNetwork: false,
            windowedEta: null,
            driveName: 'Macintosh HD',
        },
        {
            id: 'network-find-files',
            caption: 'Network · find files (no Save / Catch-up steps; count + elapsed)',
            activity: scan({
                volumeId: 'smb-naspi',
                entriesScanned: 65_311,
                dirsFound: 4_820,
                bytesScanned: 90_000_000_000,
                scanStartedAt: NOW - 52_000,
                volumeUsedBytes: 1_800_000_000_000,
            }),
            aggregation: undefined,
            phase: 'scanning',
            isNetwork: true,
            windowedEta: null,
            driveName: 'naspi on naspolya',
        },
        {
            id: 'network-compute',
            caption: 'Network · compute folder sizes (computing → bar)',
            activity: scan({ volumeId: 'smb-naspi', entriesScanned: 1_380_060, dirsFound: 71_200 }),
            aggregation: agg('computing', 41_000, 71_200, 70),
            phase: 'scanning',
            isNetwork: true,
            windowedEta: null,
            driveName: 'naspi on naspolya',
        },
        {
            id: 'replay',
            caption: 'Event-log roll-on · update index (single step + bar)',
            activity: scan({
                phase: 'replaying',
                replayEventsProcessed: 3_400,
                replayEstimatedTotal: 12_000,
                replayStartedAt: NOW - 9_000,
            }),
            aggregation: undefined,
            phase: 'replaying',
            isNetwork: false,
            windowedEta: '20s left',
            driveName: 'Macintosh HD',
        },
    ]

    // The collapsed secondary-drive row the corner indicator shows for every
    // active drive past the first (the primary expands to the full checklist). A
    // first-scan drive (no prior-scan calibration), so the one-line metric is the
    // honest running count (`indexing.summary.found`) rather than a percent — the
    // distinctive summary state, and the only place that string renders.
    const SUMMARY_ACTIVITY = scan({
        volumeId: 'tm',
        entriesScanned: 248_000,
        dirsFound: 19_400,
        scanStartedAt: NOW - 40_000,
    })
</script>

<SectionCard id="graphics-drive-indexing" label="Drive indexing status">
    <p class="intro">
        The per-volume indexing checklist (<code>IndexingStatusBody</code>) shown in the corner hourglass tooltip and the
        breadcrumb drive badge, plus the collapsed one-line summary (<code>IndexingDriveSummary</code>) the corner uses
        for secondary drives. Each tile is a fixture, not live state.
    </p>

    <div class="grid">
        {#each STATES as s (s.id)}
            <figure class="tile" id="graphics-drive-indexing-{s.id}">
                <div class="tooltip-surface">
                    <span class="drive-heading">{s.driveName}</span>
                    <IndexingStatusBody
                        activity={s.activity}
                        aggregation={s.aggregation}
                        now={NOW}
                        windowedEta={s.windowedEta}
                        phase={s.phase}
                        isNetwork={s.isNetwork}
                    />
                </div>
                <figcaption>{s.caption}</figcaption>
            </figure>
        {/each}

        <figure class="tile" id="graphics-drive-indexing-summary">
            <div class="tooltip-surface">
                <IndexingDriveSummary activity={SUMMARY_ACTIVITY} aggregation={undefined} driveName="Time Machine" />
            </div>
            <figcaption>Collapsed secondary-drive summary (multi-drive corner tooltip)</figcaption>
        </figure>
    </div>
</SectionCard>

<style>
    .intro {
        margin: 0 0 var(--spacing-md);
        color: var(--color-text-secondary);
        max-width: 60ch;
    }

    .grid {
        display: grid;
        grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
        gap: var(--spacing-lg);
    }

    .tile {
        margin: 0;
        display: flex;
        flex-direction: column;
        gap: var(--spacing-sm);
    }

    /* Mimics the real tooltip the body normally lives in: the same glass surface,
       radius, and padding, so a captured tile reads like the shipping tooltip. */
    .tooltip-surface {
        align-self: start;
        max-width: 320px;
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
        padding: var(--spacing-sm) var(--spacing-md);
        background: var(--color-bg-glass);
        border: 0.5px solid var(--color-border-glass);
        border-radius: var(--radius-md);
        box-shadow: var(--shadow-md);
    }

    /* Matches the corner row's drive-name title above the checklist. */
    .drive-heading {
        font-weight: 600;
        color: var(--color-text-primary);
    }

    figcaption {
        color: var(--color-text-tertiary);
        font-size: var(--font-size-sm);
    }

    :global(html.reduce-transparency) .tooltip-surface {
        -webkit-backdrop-filter: none;
        backdrop-filter: none;
    }
</style>
