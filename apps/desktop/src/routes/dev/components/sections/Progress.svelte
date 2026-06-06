<script lang="ts">
    import { onDestroy } from 'svelte'
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import ProgressBar from '$lib/ui/ProgressBar.svelte'

    const staticValue = 0.6
    const staticPercent = Math.round(staticValue * 100)

    let animatedValue = $state(0)
    let direction = 1
    const stepMs = 50
    const stepSize = stepMs / 3000

    const interval = setInterval(() => {
        animatedValue += direction * stepSize
        if (animatedValue >= 1) {
            animatedValue = 1
            direction = -1
        } else if (animatedValue <= 0) {
            animatedValue = 0
            direction = 1
        }
    }, stepMs)

    onDestroy(() => {
        clearInterval(interval)
    })
</script>

<SectionCard id="components-progress" label="Progress">
    <div class="rows">
        <div class="row">
            <div class="bar-cell">
                <span class="bar-label">size sm, {staticPercent}%</span>
                <ProgressBar value={staticValue} size="sm" ariaLabel="Sample progress" />
            </div>
            <div class="bar-cell">
                <span class="bar-label">size md, {staticPercent}%</span>
                <ProgressBar value={staticValue} size="md" ariaLabel="Sample progress" />
            </div>
        </div>

        <div class="row">
            <div class="bar-cell">
                <span class="bar-label">size md, animated</span>
                <ProgressBar value={animatedValue} size="md" ariaLabel="Animated sample" />
            </div>
        </div>
    </div>
</SectionCard>

<style>
    .rows {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-lg);
    }

    .row {
        display: flex;
        gap: var(--spacing-xl);
        align-items: center;
        flex-wrap: wrap;
    }

    .bar-cell {
        display: flex;
        align-items: center;
        gap: var(--spacing-md);
        flex: 1;
        min-width: 240px;
    }

    .bar-label {
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
        white-space: nowrap;
    }
</style>
