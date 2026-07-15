<script lang="ts">
    /**
     * The image-index importance-threshold slider (the "make it feel nice" ask),
     * rendered inside the "Image search" card in `FileSystemWatchingSection.svelte` once the
     * master `mediaIndex.enabled` toggle is on. It sets how deep image indexing goes, from
     * "only my most-used folders" to "everywhere".
     *
     * Named buckets are the primary UI; the typed `0.0..=1.0` threshold stays under the hood
     * (`mediaIndex.importanceThreshold`). Dragging RIGHT indexes MORE (a lower threshold), so
     * the rightmost bucket (threshold `0.0`, the default) is the broadest — everything scored,
     * junk floored out regardless. Each bucket maps to a fixed threshold stop (`THRESHOLDS`).
     *
     * The persisted setting live-applies to the backend scheduler via `settings-applier.ts`
     * (the `mediaIndex.enabled` precedent), so the control and the enrichment behavior read
     * the SAME importance signal and can't drift. A commit happens on bucket change (the step
     * is discrete, so at most a few commits per drag — no scheduler thrash).
     *
     * The live preview is honest: `mediaIndexCoveredCount(threshold, enabledVolumeIds)` counts
     * ONLY the enabled volumes (local root + opted-in SMB; the backend drops non-opted-in
     * SMB / MTP), debounced so a drag doesn't thrash IPC. When a volume isn't ready yet the
     * backend flags `pending`, and we voice "still scanning" rather than a confident wrong
     * number. A drag also shows the incremental delta vs the last settled level ("adds about
     * 12,000 images"), which folds into the baseline once the value settles.
     */
    import { onMount } from 'svelte'
    import { Slider, type SliderValueChangeDetails } from '@ark-ui/svelte/slider'
    import { getSetting, setSetting, onSpecificSettingChange } from '$lib/settings'
    import { tString } from '$lib/intl/messages.svelte'
    import { formatInteger } from '$lib/intl/number-format'
    import { getEnabledMediaIndexVolumeIds } from '$lib/media-index/enabled-volumes'
    import { ROOT_VOLUME_ID } from '$lib/indexing'
    import {
        mediaIndexCoveredCount,
        mediaIndexVolumeState,
        type CoveredCount,
        type MediaIndexVolumeState,
    } from '$lib/tauri-commands'
    import { getAppLogger } from '$lib/logging/logger'
    import { shouldRepollPreview } from './media-index-preview-poll'
    import { shouldOfferReclaim } from './media-index-reclaim'
    import MediaIndexReclaim from './MediaIndexReclaim.svelte'

    const log = getAppLogger('media-index')

    // The five named coverage levels, left (most restrictive) → right (broadest). The slider
    // operates on the bucket index (0..4); each maps to a fixed importance threshold. The
    // rightmost (threshold 0.0) matches the backend default, so an unpersisted (sparse) store
    // and the UI agree without eagerly writing a default.
    interface Bucket {
        threshold: number
        labelKey: Parameters<typeof tString>[0]
    }
    const BUCKETS: readonly Bucket[] = [
        { threshold: 0.8, labelKey: 'settings.mediaIndex.importanceThreshold.bucket.mostUsed' },
        { threshold: 0.6, labelKey: 'settings.mediaIndex.importanceThreshold.bucket.often' },
        { threshold: 0.4, labelKey: 'settings.mediaIndex.importanceThreshold.bucket.sometimes' },
        { threshold: 0.2, labelKey: 'settings.mediaIndex.importanceThreshold.bucket.most' },
        { threshold: 0.0, labelKey: 'settings.mediaIndex.importanceThreshold.bucket.everywhere' },
    ]
    const MAX_BUCKET = BUCKETS.length - 1
    const PREVIEW_DEBOUNCE_MS = 200
    // How long the value must sit still before the drag delta folds into the baseline.
    const SETTLE_MS = 900

    /** The bucket index whose threshold is nearest `threshold` (persisted values are floats). */
    function bucketFromThreshold(threshold: number): number {
        let best = 0
        let bestDist = Infinity
        for (let i = 0; i < BUCKETS.length; i++) {
            const dist = Math.abs(BUCKETS[i].threshold - threshold)
            if (dist < bestDist) {
                bestDist = dist
                best = i
            }
        }
        return best
    }

    let bucket = $state(bucketFromThreshold(getSetting('mediaIndex.importanceThreshold')))
    // The covered-count preview for the live bucket (`null` until the first result lands).
    let covered = $state<CoveredCount | null>(null)
    // Baseline for the incremental hint: the settled bucket's image count. `null` until known.
    let baselineBucket = $state(bucket)
    let baselineImages = $state<number | null>(null)

    let previewTimer: ReturnType<typeof setTimeout> | undefined
    let settleTimer: ReturnType<typeof setTimeout> | undefined
    // Monotonic request id so a late covered-count response for a superseded bucket is dropped.
    let previewSeq = 0

    // Per-volume enrichment progress (local root + opted-in network), polled while visible.
    let localState = $state<MediaIndexVolumeState | null>(null)

    async function refreshPreview(targetBucket: number): Promise<void> {
        const seq = ++previewSeq
        const threshold = BUCKETS[targetBucket].threshold
        try {
            const result = await mediaIndexCoveredCount(threshold, getEnabledMediaIndexVolumeIds())
            if (seq !== previewSeq) return
            covered = result
            // Seed the baseline the first time we get a real number, so the first drag has
            // something to diff against.
            if (baselineImages === null) {
                baselineImages = result.images
                baselineBucket = targetBucket
            }
        } catch (err) {
            if (seq !== previewSeq) return
            covered = null
            log.warn('covered-count query failed: {err}', { err: String(err) })
        }
    }

    function schedulePreview(targetBucket: number): void {
        if (previewTimer) clearTimeout(previewTimer)
        previewTimer = setTimeout(() => void refreshPreview(targetBucket), PREVIEW_DEBOUNCE_MS)
    }

    /** After the value sits still, fold the current count into the baseline so the hint clears. */
    function scheduleSettle(targetBucket: number): void {
        if (settleTimer) clearTimeout(settleTimer)
        settleTimer = setTimeout(() => {
            baselineBucket = targetBucket
            if (covered) baselineImages = covered.images
        }, SETTLE_MS)
    }

    function handleChange(details: SliderValueChangeDetails): void {
        const next = Math.min(MAX_BUCKET, Math.max(0, details.value[0]))
        if (next === bucket) return
        bucket = next
        // Commit + live-apply the typed threshold (persist is sparse; the applier pushes it to
        // the scheduler). Discrete steps mean only a handful of commits across a full drag.
        setSetting('mediaIndex.importanceThreshold', BUCKETS[next].threshold)
        schedulePreview(next)
        scheduleSettle(next)
    }

    async function refreshLocalState(): Promise<void> {
        try {
            localState = await mediaIndexVolumeState(ROOT_VOLUME_ID)
        } catch {
            // Leave the prior snapshot; a transient read failure shouldn't blank the line.
        }
    }

    onMount(() => {
        void refreshPreview(bucket)
        void refreshLocalState()
        // Keep the slider in sync if the threshold changes in another window.
        const unsub = onSpecificSettingChange('mediaIndex.importanceThreshold', (_id, value) => {
            const next = bucketFromThreshold(value)
            if (next !== bucket) {
                bucket = next
                schedulePreview(next)
                scheduleSettle(next)
            }
        })
        const timer = setInterval(() => {
            void refreshLocalState()
            // Re-poll the covered-count preview while it's unresolved (first fetch not
            // yet landed, or the backend still reports pending — a drive scanning, or
            // importance not scored yet), so a `pending` result resolves on its own
            // instead of sitting forever. Stops once resolved.
            if (shouldRepollPreview(covered)) void refreshPreview(bucket)
        }, 3000)
        return () => {
            unsub()
            clearInterval(timer)
            if (previewTimer) clearTimeout(previewTimer)
            if (settleTimer) clearTimeout(settleTimer)
        }
    })

    const currentLabel = $derived(tString(BUCKETS[bucket].labelKey))

    // Image indexing is deferred on the local disk because importance hasn't scored
    // its folders yet (drive scanned + enabled, but the ranking that decides indexing
    // order isn't ready). Voiced honestly, and it REPLACES the generic covered-count
    // spinner so the panel never shows two spinners for one wait (plan M1).
    const waitingForImportance = $derived(localState?.waitingForImportance ?? false)

    // The incremental hint: the signed image delta vs the last settled bucket. Shown only while
    // the live bucket differs from the settled baseline (i.e. mid-adjustment).
    const delta = $derived.by(() => {
        if (bucket === baselineBucket || baselineImages === null || covered === null || covered.pending) return null
        return covered.images - baselineImages
    })

    // Progress line for the local disk, threshold-aware (plan M5): "N of M in your
    // covered folders", where M is `coveredQualifyingCount` (the folders the current
    // slider setting includes) and N is the indexed rows INSIDE that coverage
    // (`enrichedCount - keptCount`). It can honestly reach done at any slider position,
    // unlike the whole-drive total. When importance hasn't scored the volume yet
    // (`coveredQualifyingCount === null`), fall back to the whole-drive count.
    const localProgress = $derived.by(() => {
        const s = localState
        if (!s || !s.enabled) return null
        if (s.coveredQualifyingCount != null) {
            const covered = s.coveredQualifyingCount
            if (covered === 0) return null
            const indexed = Math.max(0, Math.min(covered, s.enrichedCount - (s.keptCount ?? 0)))
            if (indexed >= covered) {
                return tString('settings.mediaIndex.progress.coveredDone', {
                    total: covered,
                    totalText: formatInteger(covered),
                })
            }
            return tString('settings.mediaIndex.progress.coveredOfTotal', {
                total: covered,
                enrichedText: formatInteger(indexed),
                totalText: formatInteger(covered),
            })
        }
        // Not scored yet: the whole-drive count (or "counting" before the index is ready).
        if (s.qualifyingCount === null) {
            return s.enrichedCount > 0 ? null : tString('settings.mediaIndex.progress.counting')
        }
        if (s.qualifyingCount === 0) return null
        if (s.enrichedCount >= s.qualifyingCount) {
            return tString('settings.mediaIndex.progress.done', {
                total: s.qualifyingCount,
                totalText: formatInteger(s.qualifyingCount),
            })
        }
        return tString('settings.mediaIndex.progress.ofTotal', {
            total: s.qualifyingCount,
            enrichedText: formatInteger(s.enrichedCount),
            totalText: formatInteger(s.qualifyingCount),
        })
    })

    // The quiet kept-rows line (plan M5): images indexed under a BROADER past setting,
    // kept searchable (the slider is forward-only). Shown only when the fuller reclaim
    // line ISN'T offered, so the two never duplicate — they're ONE narrative (the kept
    // line frames the value; the reclaim line adds the delete-to-free tradeoff). Keys on
    // the SAME `shouldOfferReclaim` floor the reclaim component uses, over the same
    // single-source counts (`enrichedCount` = total stored, `keptCount` = doomed).
    const keptLine = $derived.by(() => {
        const s = localState
        if (!s || !s.enabled || s.keptCount == null || s.keptCount === 0) return null
        if (shouldOfferReclaim(s.enrichedCount, s.keptCount)) return null
        return tString('settings.mediaIndex.progress.kept', {
            kept: s.keptCount,
            keptText: formatInteger(s.keptCount),
        })
    })

    /** Screen-reader value text: announce the bucket name, not the raw index. */
    function ariaValueText(): string {
        return currentLabel
    }
</script>

<div class="mi-slider">
    <div class="mi-slider-head">
        <span class="mi-slider-value" aria-hidden="true">{currentLabel}</span>
    </div>

    <Slider.Root
        value={[bucket]}
        onValueChange={handleChange}
        min={0}
        max={MAX_BUCKET}
        step={1}
        getAriaValueText={ariaValueText}
        aria-label={[tString('settings.mediaIndex.importanceThreshold.label')]}
        class="mi-slider-root"
    >
        <Slider.Control class="mi-slider-control">
            <Slider.Track class="mi-slider-track">
                <Slider.Range class="mi-slider-range" />
            </Slider.Track>
            <!-- No `Slider.HiddenInput`: nesting a focusable input inside the thumb trips axe's
                 nested-interactive + unlabeled-input rules. The E2E drives the thumb by its
                 `.mi-slider-thumb` class, so the data-test hook rides the thumb itself. -->
            <Slider.Thumb index={0} class="mi-slider-thumb" data-test="media-importance-threshold" />
            <div class="mi-slider-ticks" aria-hidden="true">
                {#each BUCKETS as b, i (b.threshold)}
                    <span class="mi-slider-tick" class:active={i <= bucket} style="left: {(i / MAX_BUCKET) * 100}%"
                    ></span>
                {/each}
            </div>
        </Slider.Control>
        <div class="mi-slider-ends" aria-hidden="true">
            <span>{tString('settings.mediaIndex.importanceThreshold.bucket.mostUsed')}</span>
            <span>{tString('settings.mediaIndex.importanceThreshold.bucket.everywhere')}</span>
        </div>
    </Slider.Root>

    <p class="mi-preview" aria-live="polite">
        {#if waitingForImportance}
            <!-- Deferred on importance: one honest line, replacing the generic
                 "Working out how much this covers…" spinner (same underlying wait). -->
            {tString('settings.mediaIndex.importanceThreshold.waitingForImportance')}
        {:else if covered === null}
            {tString('settings.mediaIndex.importanceThreshold.previewCounting')}
        {:else if covered.folders > 0}
            {tString('settings.mediaIndex.importanceThreshold.preview', {
                images: covered.images,
                imagesText: formatInteger(covered.images),
                folders: covered.folders,
                foldersText: formatInteger(covered.folders),
            })}
            {#if covered.pending}
                <span class="mi-preview-pending"
                    >{tString('settings.mediaIndex.importanceThreshold.pending')}</span
                >
            {/if}
        {:else if covered.pending}
            <!-- Zero folders SO FAR but a volume is still scanning: honest "still counting",
                 never a confident "nothing matches". -->
            {tString('settings.mediaIndex.importanceThreshold.previewCounting')}
        {:else}
            {tString('settings.mediaIndex.importanceThreshold.previewNone')}
        {/if}
    </p>

    {#if delta !== null && delta !== 0}
        <p class="mi-delta">
            {delta > 0
                ? tString('settings.mediaIndex.importanceThreshold.deltaAdd', {
                      images: delta,
                      imagesText: formatInteger(delta),
                  })
                : tString('settings.mediaIndex.importanceThreshold.deltaRemove', {
                      images: -delta,
                      imagesText: formatInteger(-delta),
                  })}
        </p>
    {/if}

    <p class="mi-floor">{tString('settings.mediaIndex.importanceThreshold.floor')}</p>

    {#if localProgress}
        <div class="mi-progress">
            <span class="mi-progress-name">{tString('settings.mediaIndex.progress.local')}</span>
            <span class="mi-progress-line">{localProgress}</span>
        </div>
    {/if}

    {#if keptLine}
        <p class="mi-kept">{keptLine}</p>
    {/if}

    <!-- Reclaim space: lowering the slider never deletes rows, so a drive indexed at a
         broader setting keeps that coverage. This offers to delete the leftover, but only
         once counts have settled (not while waiting on importance or a scan). -->
    <MediaIndexReclaim
        threshold={BUCKETS[bucket].threshold}
        blocked={waitingForImportance || covered === null || covered.pending}
    />
</div>

<style>
    .mi-slider {
        padding: var(--spacing-sm) 0 var(--spacing-xs);
    }

    .mi-slider-head {
        margin-bottom: var(--spacing-xs);
    }

    .mi-slider-value {
        font-weight: 600;
        color: var(--color-text-primary);
    }

    :global(.mi-slider-root) {
        display: block;
        width: 100%;
    }

    :global(.mi-slider-control) {
        position: relative;
        display: flex;
        align-items: center;
        height: 20px;
        width: 100%;
    }

    :global(.mi-slider-track) {
        flex: 1;
        height: 4px;
        background: var(--color-bg-tertiary);
        border-radius: var(--radius-xs);
        position: relative;
    }

    :global(.mi-slider-range) {
        height: 100%;
        background: var(--color-accent);
        border-radius: var(--radius-xs);
        transition: width var(--transition-base);
    }

    :global(.mi-slider-thumb) {
        width: 16px;
        height: 16px;
        background: white;
        border: 2px solid var(--color-accent);
        border-radius: var(--radius-full);
        cursor: default;
        box-shadow: var(--shadow-sm);
        z-index: 2;
        position: relative;
    }

    :global(.mi-slider-thumb:hover) {
        border-color: var(--color-accent-hover);
    }

    :global(.mi-slider-thumb:focus-visible) {
        outline: 2px solid var(--color-accent);
        outline-offset: 2px;
        box-shadow: var(--shadow-focus);
    }

    .mi-slider-ticks {
        position: absolute;
        left: 0;
        right: 0;
        top: 50%;
        transform: translateY(-50%);
        height: 4px;
        pointer-events: none;
        z-index: 0;
    }

    .mi-slider-tick {
        position: absolute;
        width: 2px;
        height: 8px;
        background: var(--color-border);
        transform: translate(-50%, -50%);
        top: 50%;
    }

    .mi-slider-tick.active {
        background: var(--color-accent);
    }

    .mi-slider-ends {
        display: flex;
        justify-content: space-between;
        margin-top: var(--spacing-xs);
        color: var(--color-text-tertiary);
        font-size: var(--font-size-sm);
    }

    .mi-preview {
        margin: var(--spacing-sm) 0 0;
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
        line-height: 1.4;
    }

    .mi-preview-pending {
        color: var(--color-text-tertiary);
    }

    .mi-delta {
        margin: var(--spacing-xxs) 0 0;
        color: var(--color-accent-text);
        font-size: var(--font-size-sm);
        line-height: 1.4;
    }

    .mi-floor {
        margin: var(--spacing-xs) 0 0;
        color: var(--color-text-tertiary);
        font-size: var(--font-size-sm);
        line-height: 1.4;
    }

    .mi-progress {
        display: flex;
        align-items: baseline;
        justify-content: space-between;
        gap: var(--spacing-md);
        margin-top: var(--spacing-sm);
        padding-top: var(--spacing-sm);
        border-top: 1px solid var(--color-border-subtle);
    }

    .mi-progress-name {
        font-weight: 500;
        color: var(--color-text-primary);
        font-size: var(--font-size-sm);
    }

    .mi-progress-line {
        color: var(--color-text-tertiary);
        font-size: var(--font-size-sm);
    }

    /* The quiet kept-rows line: still-searchable images from a broader past setting.
       Tertiary + small so it reads as a reassuring aside, not a call to action. */
    .mi-kept {
        margin: var(--spacing-xxs) 0 0;
        color: var(--color-text-tertiary);
        font-size: var(--font-size-sm);
        line-height: 1.4;
    }

    /* Honor reduced-motion: drop the range-fill transition (WKWebView reflects this media
       query for prefers-reduced-motion, unlike prefers-reduced-transparency). */
    @media (prefers-reduced-motion: reduce) {
        :global(.mi-slider-range) {
            transition: none;
        }
    }
</style>
