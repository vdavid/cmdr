<!--
  Small uppercase stability pill (ALPHA / BETA) shown next to a feature's title.
  Stable features carry no badge by policy: don't mount this component for them.
  The status itself comes from the repo-root `feature-status.json` via
  `$lib/feature-status` (`getBadgeStatus(id)`); see `docs/feature-status.md`.
  Visual pattern shared with `ToggleGroup.svelte`'s `.tg-badge` (the "AI" chip).
-->
<script lang="ts">
    import { tooltip } from '$lib/tooltip/tooltip'
    import type { BadgeStatus } from '$lib/feature-status'

    interface Props {
        status: BadgeStatus
    }

    const { status }: Props = $props()

    const statusToTooltipMap: Record<BadgeStatus, string> = {
        alpha: 'Early-stage feature: works, but expect rough edges',
        beta: 'Mostly solid: unusual setups can still surprise',
    }
</script>

<span class="feature-status-badge" use:tooltip={statusToTooltipMap[status]}>{status}</span>

<style>
    .feature-status-badge {
        font-size: var(--font-size-xs);
        font-family: var(--font-mono);
        font-weight: 600;
        letter-spacing: 0.04em;
        text-transform: uppercase;
        padding: var(--spacing-xxs) var(--spacing-xs);
        background: var(--color-accent-subtle);
        color: var(--color-text-primary);
        border-radius: var(--radius-xs);
        line-height: 1;
    }
</style>
