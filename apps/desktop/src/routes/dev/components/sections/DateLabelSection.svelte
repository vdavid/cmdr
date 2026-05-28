<script lang="ts">
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import DateLabel from '$lib/ui/DateLabel.svelte'

    /**
     * Catalog entries cover every age tier the per-component coloring rules can
     * surface (year / month / day / time). DateLabel reads `appearance.dateFormat`
     * and `appearance.dateColors` from the store at render time, so what you see
     * here reflects whatever the active settings are. Switch palettes in the
     * Settings window to compare; the catalog re-renders live.
     */
    interface Row {
        caption: string
        ts: number | null | undefined
    }

    function hoursAgo(h: number): number {
        return Math.floor(Date.now() / 1000) - h * 60 * 60
    }

    function daysAgo(d: number): number {
        return hoursAgo(d * 24)
    }

    const rows: Row[] = [
        { caption: 'A few hours ago (time tier)', ts: hoursAgo(2) },
        { caption: 'Today (day tier: today)', ts: hoursAgo(6) },
        { caption: 'Yesterday (day tier: yesterday)', ts: daysAgo(1) },
        { caption: 'Two days ago (day tier: 2 days)', ts: daysAgo(2) },
        { caption: 'A week ago (month tier: this month)', ts: daysAgo(7) },
        { caption: 'Three months ago (year tier: this year)', ts: daysAgo(90) },
        { caption: 'Last year (year tier: last year)', ts: daysAgo(400) },
        { caption: 'Two years ago (year tier: 2 years)', ts: daysAgo(730) },
        { caption: 'Four years ago (year tier: old)', ts: daysAgo(1460) },
        { caption: 'null (empty state)', ts: null },
        { caption: 'undefined (empty state)', ts: undefined },
    ]
</script>

<SectionCard id="components-date-label" label="Date label">
    <p class="hint">
        Colors track the active <code>appearance.dateColors</code> palette and per-component age tiers (year / month /
        day / time). Switch palettes in Settings &rarr; Appearance to compare; this catalog re-renders live.
    </p>
    <div class="rows">
        {#each rows as row, i (i)}
            <div class="row">
                <p class="caption">{row.caption}</p>
                <DateLabel modifiedAt={row.ts} />
            </div>
        {/each}
    </div>
</SectionCard>

<style>
    .hint {
        margin: 0 0 var(--spacing-md);
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
    }

    .hint code {
        font-family: var(--font-mono);
        font-size: var(--font-size-xs);
        color: var(--color-text-secondary);
    }

    .rows {
        display: grid;
        grid-template-columns: 1fr auto;
        gap: var(--spacing-xs) var(--spacing-lg);
        align-items: baseline;
    }

    .caption {
        margin: 0;
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
    }

    .row {
        display: contents;
    }
</style>
