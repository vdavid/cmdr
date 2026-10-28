<script lang="ts">
    // A button label as wide as the widest of a set, so a column of buttons whose labels differ
    // ("Move 4 files", "Move 5 files to Trash") still lines up. Every label sits in the same grid
    // cell and only `label` shows: the browser sizes the cell to the widest, at the user's own
    // font and text size, with nothing measured in script. The rest are `visibility: hidden` CSS
    // content, so they take no clicks, a screen reader hears only `label`, and the button's
    // `textContent` is `label` alone.
    interface Props {
        label: string
        /** Every label a button in the set shows right now, this one's included or not. */
        labels: string[]
    }

    const { label, labels }: Props = $props()

    /** The others, deduplicated, so a dialog of seven "Move 4 files" holds one spare copy. */
    const others = $derived(labels.filter((other, index) => other !== label && labels.indexOf(other) === index))
</script>

<span class="sized-label">
    <span>{label}</span>
    {#each others as other (other)}
        <!-- Drawn by CSS from the attribute, so the button's text is its own label alone, for a
             copy, a test, or anything else that reads it. -->
        <span class="spare" aria-hidden="true" data-label={other}></span>
    {/each}
</span>

<style>
    .sized-label {
        display: inline-grid;
        justify-items: center;
    }

    .sized-label > span {
        grid-area: 1 / 1;
        white-space: nowrap;
    }

    .spare {
        visibility: hidden;
    }

    .spare::before {
        content: attr(data-label);
    }
</style>
