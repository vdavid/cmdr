<script lang="ts">
    const {
        value,
        size = 'md',
        ariaLabel,
    }: {
        value: number
        size?: 'sm' | 'md'
        ariaLabel?: string
    } = $props()

    const percent = $derived(Math.min(100, Math.round(value * 100)))
    const widthPercent = $derived(Math.min(100, value * 100))
</script>

<div
    class="track {size}"
    role="progressbar"
    aria-valuenow={percent}
    aria-valuemin={0}
    aria-valuemax={100}
    aria-label={ariaLabel}
>
    <div class="fill" style="width: {widthPercent}%"></div>
</div>

<style>
    .track {
        flex: 1;
        min-width: 0;
        background: var(--color-bg-tertiary);
        overflow: hidden;
    }

    .track.sm {
        height: 4px;
        border-radius: var(--radius-xs);
    }

    .track.md {
        height: 8px;
        border-radius: var(--radius-sm);
    }

    .fill {
        position: relative;
        height: 100%;
        background: var(--color-accent);
        border-radius: inherit;
        transition: width 0.15s ease-out;
        overflow: hidden;
    }

    .fill::after {
        content: '';
        position: absolute;
        inset: 0;
        background: linear-gradient(
            45deg,
            transparent 20%,
            color-mix(in oklch, var(--color-accent), white 30%) 50%,
            transparent 80%
        );
        background-size: 200% 100%;
        animation: shimmer 2.5s infinite linear;
    }

    @keyframes shimmer {
        from {
            background-position: 200% 0;
        }
        to {
            background-position: -200% 0;
        }
    }
</style>
