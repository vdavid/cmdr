<!-- A footer row of "open this in the source tool" links under a section. -->
<script lang="ts">
    let { links }: { links: Array<{ label: string; href: string }> } = $props()
</script>

<div class="mt-4 flex flex-wrap gap-3 border-t border-border-subtle pt-3">
    {#each links as link}
        <a
            href={link.href}
            target="_blank"
            rel="noopener noreferrer"
            class="text-xs text-text-tertiary transition-colors hover:text-accent"
        >
            {link.label} &#x2197;
        </a>
    {/each}
</div>
