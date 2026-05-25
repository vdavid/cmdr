<script lang="ts">
    import { onDestroy } from 'svelte'
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import ProgressBar from '$lib/ui/ProgressBar.svelte'
    import ProgressOverlay from '$lib/ui/ProgressOverlay.svelte'
    import Button from '$lib/ui/Button.svelte'

    const staticValue = 0.6

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

    let overlayVisible = $state(false)
    let overlayTimer: ReturnType<typeof setTimeout> | undefined

    function showOverlay() {
        overlayVisible = true
        if (overlayTimer !== undefined) clearTimeout(overlayTimer)
        overlayTimer = setTimeout(() => {
            overlayVisible = false
        }, 5000)
    }

    onDestroy(() => {
        if (overlayTimer !== undefined) clearTimeout(overlayTimer)
    })

    const overlayProgress = 0.42
    const overlayPercent = Math.round(staticValue * 100)
</script>

<SectionCard id="components-progress" label="Progress">
    <div class="rows">
        <div class="row">
            <div class="bar-cell">
                <span class="bar-label">size sm, {overlayPercent}%</span>
                <ProgressBar value={staticValue} size="sm" ariaLabel="Sample progress" />
            </div>
            <div class="bar-cell">
                <span class="bar-label">size md, {overlayPercent}%</span>
                <ProgressBar value={staticValue} size="md" ariaLabel="Sample progress" />
            </div>
        </div>

        <div class="row">
            <div class="bar-cell">
                <span class="bar-label">size md, animated</span>
                <ProgressBar value={animatedValue} size="md" ariaLabel="Animated sample" />
            </div>
        </div>

        <div class="row">
            <Button onclick={showOverlay}>Show ProgressOverlay for 5 seconds</Button>
        </div>
    </div>

    <div class="overlay-host">
        <ProgressOverlay
            visible={overlayVisible}
            label="Scanning..."
            detail="42,000 entries"
            progress={overlayProgress}
            eta="~2 min left"
        />
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

    /* The overlay positions itself absolutely; this host gives it a
       positioned ancestor so it pins to this section, not the viewport. */
    .overlay-host {
        position: relative;
        height: 0;
    }
</style>
