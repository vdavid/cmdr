<script lang="ts">
    /**
     * FilterDropdown: the labelled-grid filter surface used by the query dialogs' Size, Modified,
     * and Search-in popovers. A thin composition of `Dropdown` (positioning, focus trap, Esc-scoped
     * close) plus the shared section header (an uppercase label above the controls).
     *
     * It exists as its own component, rather than a `variant` prop on `Dropdown`, so the generic
     * `Dropdown` stays free of filter-specific markup and the three filter popovers thread only
     * `anchor` / `open` / `onClose` / `label`. The header layout (`.popover-section` /
     * `.popover-label`) lives in the shared `filter-popover.css` so it can also style the grid
     * children, which Svelte's component-scoped `<style>` can't reach.
     *
     * Pass `labelFor` when the header labels a single form control (the Search-in textarea), so the
     * header renders a real `<label for=…>` association; otherwise it's a plain `<span>` heading
     * above a radio grid.
     */
    import type { Snippet } from 'svelte'
    import Dropdown from './Dropdown.svelte'
    import '$lib/query-ui/filter-chips/filter-popover.css'

    interface Props {
        /** The chip element, used by the dropdown shell for positioning + focus return. */
        anchor: HTMLElement
        /** Whether the popover is shown (owned by the parent's `openChip` state). */
        open: boolean
        /** Fired when the popover wants to close (Esc / click outside). */
        onClose: () => void
        /** The filter name shown in the header ("Size", "Modified", "Search in"). */
        label: string
        /** aria-label for the dropdown region. */
        ariaLabel: string
        /** When set, the header is a `<label for={labelFor}>` (single-control surfaces like Scope). */
        labelFor?: string
        /** Widens the section for the dense grid (Size / Modified) or the scope textarea. */
        sectionClass?: 'size-grid-section' | 'scope-popover'
        children: Snippet
    }

    const { anchor, open, onClose, label, ariaLabel, labelFor, sectionClass, children }: Props = $props()
</script>

<Dropdown {anchor} {open} {onClose} {ariaLabel}>
    <!-- `class:` directives (not a `{sectionClass}` interpolation) so the css-unused checker
         registers `size-grid-section` / `scope-popover` as live usages. -->
    <div
        class="popover-section"
        class:size-grid-section={sectionClass === 'size-grid-section'}
        class:scope-popover={sectionClass === 'scope-popover'}
    >
        {#if labelFor}
            <label class="popover-label" for={labelFor}>{label}</label>
        {:else}
            <span class="popover-label">{label}</span>
        {/if}
        {@render children()}
    </div>
</Dropdown>
