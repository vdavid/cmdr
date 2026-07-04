<script lang="ts">
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import Menu, { type MenuItem } from '$lib/ui/Menu.svelte'

    let open = $state(false)
    let anchorPoint = $state<{ x: number; y: number } | null>(null)
    let lastChoice = $state<string | null>(null)

    const items: MenuItem[] = [
        { value: 'browse', label: 'Browse like a folder' },
        { value: 'open', label: 'Open with default app' },
        { value: 'configure', label: 'Configure…' },
    ]

    function openAt(event: MouseEvent): void {
        anchorPoint = { x: event.clientX, y: event.clientY }
        open = true
    }

    function handleOpenChange(next: boolean): void {
        open = next
    }

    function handleSelect(value: string): void {
        lastChoice = value
    }
</script>

<SectionCard id="components-menu" label="Menu">
    <div class="cell">
        <p class="caption">
            Controlled action menu (Ark UI): opened at a point, keyboard nav + typeahead + Esc from Ark. Click the anchor
            to open it at the cursor.
        </p>
        <button
            type="button"
            class="demo-anchor"
            onclick={(e) => {
                openAt(e)
            }}
        >
            Open menu here
        </button>
        {#if lastChoice}
            <p class="caption">Last choice: {lastChoice}</p>
        {/if}
        <Menu
            {open}
            onOpenChange={handleOpenChange}
            onSelect={handleSelect}
            {items}
            {anchorPoint}
            defaultHighlightedValue="browse"
            ariaLabel="Demo menu"
            portal
        />
    </div>
</SectionCard>

<style>
    .caption {
        margin: 0 0 var(--spacing-sm);
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
    }

    .demo-anchor {
        padding: var(--spacing-xs) var(--spacing-md);
        font-size: var(--font-size-sm);
        background: transparent;
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        color: var(--color-text-primary);
    }
</style>
