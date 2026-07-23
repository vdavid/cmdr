<script lang="ts">
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import Slider from '$lib/ui/Slider.svelte'

    let plain = $state(50)
    let zoom = $state(100)
    let level = $state(6)
    let bucket = $state(3)
    let disabledValue = $state(40)

    const ZOOM_STOPS = [75, 100, 125, 150]
    const LEVELS = [1, 2, 3, 4, 5, 6, 7, 8, 9]
    const BUCKETS = ['Only my most-used', 'Often used', 'Sometimes used', 'Most folders', 'Everywhere']
</script>

<SectionCard id="components-slider" label="Slider">
    <div class="grid">
        <div class="cell">
            <p class="caption">Bare track, no readout</p>
            <Slider
                value={plain}
                onChange={(v: number) => {
                    plain = v
                }}
                min={0}
                max={100}
                ariaLabel="Plain slider"
            />
        </div>

        <div class="cell">
            <p class="caption">Ticks + magnetic snapping + trailing readout (double-click the thumb to reset)</p>
            <Slider
                value={zoom}
                onChange={(v: number) => {
                    zoom = v
                }}
                min={75}
                max={150}
                step={5}
                ariaLabel="Text size"
                ariaValueText={(v: number) => `${String(v)}%`}
                ticks={ZOOM_STOPS}
                snapTargets={ZOOM_STOPS}
                valueLabel={`${String(zoom)}%`}
                onThumbDoubleClick={() => {
                    zoom = 100
                }}
            />
        </div>

        <div class="cell">
            <p class="caption">End labels</p>
            <Slider
                value={level}
                onChange={(v: number) => {
                    level = v
                }}
                min={1}
                max={9}
                ariaLabel="Compression level"
                ticks={LEVELS}
                snapTargets={LEVELS}
                endLabels={['Faster', 'Smaller']}
                valueLabel={String(level)}
            />
        </div>

        <div class="cell">
            <p class="caption">Named steps: readout above, and the value announced as a name</p>
            <Slider
                value={bucket}
                onChange={(v: number) => {
                    bucket = v
                }}
                min={0}
                max={4}
                ariaLabel="Coverage"
                ariaValueText={(v: number) => BUCKETS[v]}
                ticks={[0, 1, 2, 3, 4]}
                endLabels={[BUCKETS[0], BUCKETS[4]]}
                valueLabel={BUCKETS[bucket]}
                valueLabelPlacement="above"
            />
        </div>

        <div class="cell">
            <p class="caption">Disabled</p>
            <Slider
                value={disabledValue}
                onChange={(v: number) => {
                    disabledValue = v
                }}
                min={0}
                max={100}
                ariaLabel="Disabled slider"
                valueLabel={String(disabledValue)}
                disabled
            />
        </div>
    </div>
</SectionCard>

<style>
    .grid {
        display: grid;
        grid-template-columns: 1fr;
        gap: var(--spacing-lg);
        max-width: 420px;
    }

    .caption {
        margin: 0 0 var(--spacing-sm);
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
    }
</style>
