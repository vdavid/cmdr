<script lang="ts">
    import SectionCard from '#lib/ui/SectionCard.svelte'
    import FindBar from '#lib/ui/FindBar.svelte'

    // The first demo steps for real, wrapping at both ends; the others hold one state each.
    let query = $state('report')
    let current = $state(3)
    const total = 12
    const noop = (): void => {}
</script>

<SectionCard id="components-findbar" label="FindBar">
    <div class="stack">
        <FindBar
            bind:query
            {total}
            {current}
            ariaLabel="Find in this demo"
            placeholder="Find in chat…"
            regionLabel="Demo find bar"
            onPrevious={() => {
                current = current === 1 ? total : current - 1
            }}
            onNext={() => {
                current = current === total ? 1 : current + 1
            }}
            onClose={noop}
        />
        <FindBar
            query="nothing like this"
            ariaLabel="Find, no matches"
            placeholder="Find in chat…"
            onPrevious={noop}
            onNext={noop}
            onClose={noop}
        />
        <FindBar
            query="older"
            total={2}
            current={1}
            note="Searching loaded messages"
            ariaLabel="Find, with a note"
            placeholder="Find in chat…"
            onPrevious={noop}
            onNext={noop}
            onClose={noop}
        />
    </div>
</SectionCard>

<style>
    .stack {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-md);
    }
</style>
