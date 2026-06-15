<script lang="ts">
  import Icon from '$lib/ui/Icon.svelte'
  import type { RepoInfo } from './git-store.svelte'

  interface Props {
    info: RepoInfo
  }

  const { info }: Props = $props()

  /** Visual state derived from the info, drives the pill color. */
  const state = $derived.by((): 'clean' | 'ahead' | 'behind' | 'dirty' | 'detached' | 'unborn' => {
    if (info.unborn) return 'unborn'
    if (info.detachedSha) return 'detached'
    if (info.isDirty) return 'dirty'
    if ((info.ahead ?? 0) > 0) return 'ahead'
    if ((info.behind ?? 0) > 0) return 'behind'
    return 'clean'
  })

  const label = $derived.by((): string => {
    if (info.unborn) return `${info.branch ?? 'main'} (no commits yet)`
    if (info.detachedSha) return `(detached) ${info.detachedSha}`
    return info.branch ?? '(detached)'
  })

  const subtitle = $derived.by((): string => {
    const parts: string[] = []
    if (info.ahead != null && info.ahead > 0) parts.push(`+${String(info.ahead)}`)
    if (info.behind != null && info.behind > 0) parts.push(`-${String(info.behind)}`)
    if (info.isDirty) parts.push('dirty')
    return parts.join(' / ')
  })

  const tooltip = $derived.by((): string => {
    const lines: string[] = []
    if (info.unborn) {
      lines.push(`On unborn branch ${info.branch ?? 'main'} (no commits yet).`)
    } else if (info.detachedSha) {
      lines.push(`Detached at ${info.detachedSha}.`)
    } else if (info.branch) {
      lines.push(`On branch ${info.branch}.`)
    }
    if (info.upstream) {
      const a = info.ahead ?? 0
      const b = info.behind ?? 0
      lines.push(`${String(a)} ahead, ${String(b)} behind ${info.upstream}.`)
    }
    if (info.isDirty) lines.push('Working tree has uncommitted changes.')
    return lines.join(' ')
  })
</script>

<span class="repo-chip" class:dirty={state === 'dirty'} class:ahead={state === 'ahead'} class:behind={state === 'behind'} class:detached={state === 'detached'} class:unborn={state === 'unborn'} title={tooltip} aria-label={tooltip} data-state={state}>
  <span class="icon"><Icon name="git-branch" size={12} /></span>
  <span class="label">{label}</span>
  {#if subtitle}
    <span class="sep" aria-hidden="true">·</span>
    <span class="sub">{subtitle}</span>
  {/if}
</span>

<style>
  .repo-chip {
    /* Match the path bar's cross-axis height exactly at every zoom level.
       Approach:
       - `align-self: stretch` makes the chip's height follow the parent row,
         not the other way around.
       - Zero vertical padding and `line-height: 1` keep the chip's intrinsic
         height strictly shorter than `--font-size-sm × default line-height`
         (the path bar's natural line box), so the chip never forces the row
         to grow. Without this clamp, sub-pixel rounding at higher zoom levels
         tipped the chip past the row by one device pixel and caused a 1 px
         layout shift the instant a repo was detected.
       - The contents (icon + label) center vertically via the chip's own
         inline-flex `align-items: center`. */
    align-self: stretch;
    display: inline-flex;
    align-items: center;
    gap: var(--spacing-xs);
    padding: 0 var(--spacing-sm);
    line-height: 1;
    border-radius: var(--radius-full);
    font-size: var(--font-size-xs);
    font-weight: 500;
    background: var(--color-bg-tertiary);
    color: var(--color-text-secondary);
    /* Outline as an inset shadow rather than a `border`: a border adds layout
       height, which would push the row by 1 px. `box-shadow: inset` is
       paint-only. */
    box-shadow: inset 0 0 0 1px var(--color-border);
    white-space: nowrap;
    user-select: none;
    cursor: default;
  }

  .repo-chip.dirty {
    background: var(--color-warning-bg);
    color: var(--color-warning-text);
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--color-warning) 30%, transparent);
  }

  .repo-chip.ahead,
  .repo-chip.behind {
    background: var(--color-git-portal-subtle);
    color: var(--color-git-portal-text);
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--color-git-portal) 30%, transparent);
  }

  .repo-chip.detached,
  .repo-chip.unborn {
    background: var(--color-bg-tertiary);
    color: var(--color-text-secondary);
    font-style: italic;
  }

  .icon {
    display: inline-flex;
    color: var(--color-git-portal);
  }

  .label {
    font-family: var(--font-mono);
  }

  .sep {
    opacity: 0.5;
  }

  .sub {
    font-variant-numeric: tabular-nums;
  }
</style>
