<script lang="ts">
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import Button from '$lib/ui/Button.svelte'

    interface Cell {
        variant: 'primary' | 'secondary' | 'danger'
        size: 'regular' | 'mini'
    }

    const cells: Cell[] = [
        { variant: 'primary', size: 'regular' },
        { variant: 'secondary', size: 'regular' },
        { variant: 'danger', size: 'regular' },
        { variant: 'primary', size: 'mini' },
        { variant: 'secondary', size: 'mini' },
        { variant: 'danger', size: 'mini' },
    ]

    function cellLabel(c: Cell): string {
        return c.size === 'regular' ? 'Action' : 'Mini'
    }
</script>

<SectionCard id="components-buttons" label="Buttons">
    <div class="matrix">
        <div class="row-head" aria-hidden="true"></div>
        {#each cells as c, i (i)}
            <div class="col-head">{c.variant}/{c.size}</div>
        {/each}

        <div class="row-head">normal</div>
        {#each cells as c, i (`normal-${String(i)}`)}
            <div class="cell">
                <Button variant={c.variant} size={c.size}>{cellLabel(c)}</Button>
            </div>
        {/each}

        <div class="row-head">hover</div>
        {#each cells as c, i (`hover-${String(i)}`)}
            <div class="cell">
                <span
                    class:demo-hover-primary={c.variant === 'primary'}
                    class:demo-hover-secondary={c.variant === 'secondary'}
                    class:demo-hover-danger={c.variant === 'danger'}
                >
                    <Button variant={c.variant} size={c.size}>{cellLabel(c)}</Button>
                </span>
            </div>
        {/each}

        <div class="row-head">focused</div>
        {#each cells as c, i (`focused-${String(i)}`)}
            <div class="cell">
                <span class="demo-focus">
                    <Button variant={c.variant} size={c.size}>{cellLabel(c)}</Button>
                </span>
            </div>
        {/each}

        <div class="row-head">disabled</div>
        {#each cells as c, i (`disabled-${String(i)}`)}
            <div class="cell">
                <Button variant={c.variant} size={c.size} disabled>{cellLabel(c)}</Button>
            </div>
        {/each}
    </div>
</SectionCard>

<style>
    .matrix {
        display: grid;
        grid-template-columns: auto repeat(6, 1fr);
        gap: var(--spacing-sm) var(--spacing-md);
        align-items: center;
    }

    .col-head {
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
        text-align: center;
        text-transform: lowercase;
    }

    .row-head {
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
        padding-right: var(--spacing-sm);
    }

    .cell {
        display: flex;
        justify-content: center;
        align-items: center;
    }

    /* Hover preview: mirror the `:hover:not(:disabled)` rules from Button.svelte
       so the static row matches the real hover state. Per-variant wrappers keep
       us from blanket-overriding the canonical `.btn-*` colors. */

    /* allowed-btn-restyle: catalog static-hover preview mirrors Button.svelte */
    .demo-hover-primary :global(.btn-primary) {
        background: var(--color-accent-hover);
    }

    /* allowed-btn-restyle: catalog static-hover preview mirrors Button.svelte */
    .demo-hover-secondary :global(.btn-secondary) {
        background: var(--color-bg-tertiary);
        color: var(--color-text-primary);
    }

    /* allowed-btn-restyle: catalog static-hover preview mirrors Button.svelte */
    .demo-hover-danger :global(.btn-danger) {
        background: color-mix(in srgb, var(--color-error), transparent 90%);
    }

    /* Focus-visible preview: mirror Button.svelte's `:focus-visible` rules. */
    .demo-focus :global(.btn) {
        outline: 2px solid var(--color-accent);
        outline-offset: 1px;
        box-shadow: var(--shadow-focus-contrast);
    }
</style>
