<script lang="ts">
    import CatalogPage from '../CatalogPage.svelte'
    import Buttons from './sections/Buttons.svelte'
    import Links from './sections/Links.svelte'
    import Groups from './sections/Groups.svelte'
    import ToggleGroupSection from './sections/ToggleGroupSection.svelte'
    import CheckboxSection from './sections/CheckboxSection.svelte'
    import SwitchSection from './sections/SwitchSection.svelte'
    import RadioGroupSection from './sections/RadioGroupSection.svelte'
    import SelectSection from './sections/SelectSection.svelte'
    import ComboboxSection from './sections/ComboboxSection.svelte'
    import SliderSection from './sections/SliderSection.svelte'
    import NumberInputSection from './sections/NumberInputSection.svelte'
    import TextInputSection from './sections/TextInputSection.svelte'
    import PopoverSection from './sections/PopoverSection.svelte'
    import MenuSection from './sections/MenuSection.svelte'
    import FilterPopoverSection from './sections/FilterPopoverSection.svelte'
    import ChipSection from './sections/ChipSection.svelte'
    import Dialogs from './sections/Dialogs.svelte'
    import Toasts from './sections/Toasts.svelte'
    import Progress from './sections/Progress.svelte'
    import Loading from './sections/Loading.svelte'
    import Tooltips from './sections/Tooltips.svelte'
    import SizeBadges from './sections/SizeBadges.svelte'
    import StatusBadgeSection from './sections/StatusBadgeSection.svelte'
    import DateLabelSection from './sections/DateLabelSection.svelte'
    import ShortcutChipSection from './sections/ShortcutChip.svelte'
    import CopyBoxSection from './sections/CopyBoxSection.svelte'
    import EmptyStates from './sections/EmptyStates.svelte'

    interface Props {
        /** Optional sub-anchor (e.g. `'buttons'`). Catalog scrolls to `#components-<anchor>` when this changes. */
        targetAnchor?: string | null
        /** Fires when a new section scrolls into view. `null` when scrolled to top. */
        onSectionInView?: (subId: string | null) => void
    }

    // This file is both a standalone dev route AND imported as a regular component
    // by the Debug window's sidebar nesting. The page-props lint rule fires on the
    // route side; the component-import use is what gives these props meaning.
    // eslint-disable-next-line svelte/valid-prop-names-in-kit-pages
    const { targetAnchor = null, onSectionInView }: Props = $props()

    /** Ordered sub-ids matching the sidebar order. */
    const SUB_IDS = [
        'buttons',
        'links',
        'groups',
        'toggle-group',
        'checkbox',
        'radio-group',
        'select',
        'combobox',
        'slider',
        'number-input',
        'text-input',
        'popover',
        'menu',
        'filter-popover',
        'chip',
        'dialogs',
        'toasts',
        'progress',
        'loading',
        'tooltips',
        'size-badges',
        'status-badge',
        'date-label',
        'shortcut-chip',
        'commandbox',
        'empty-states',
    ] as const
</script>

{#if import.meta.env.DEV}
    <CatalogPage
        prefix="components"
        subIds={SUB_IDS}
        route="/dev/components"
        title="Components"
        {targetAnchor}
        {onSectionInView}
    >
        {#snippet description()}
            Catalog of every primitive in <code>lib/ui</code>. Add new primitives here — see
            <code>lib/ui/CLAUDE.md</code>.
        {/snippet}

        <Buttons />
        <Links />
        <Groups />
        <ToggleGroupSection />
        <CheckboxSection />
        <SwitchSection />
        <RadioGroupSection />
        <SelectSection />
        <ComboboxSection />
        <SliderSection />
        <NumberInputSection />
        <TextInputSection />
        <PopoverSection />
        <MenuSection />
        <FilterPopoverSection />
        <ChipSection />
        <Dialogs />
        <Toasts />
        <Progress />
        <Loading />
        <Tooltips />
        <SizeBadges />
        <StatusBadgeSection />
        <DateLabelSection />
        <ShortcutChipSection />
        <CopyBoxSection />
        <EmptyStates />
    </CatalogPage>
{/if}

<style>
    /* The inline-code pills in the header description above. */
    code {
        font-family: var(--font-mono);
        font-size: var(--font-size-xs);
        background: var(--color-bg-tertiary);
        /* stylelint-disable-next-line declaration-property-value-disallowed-list -- Inline-code pill: 1px 4px is tighter than any spacing token */
        padding: 1px 4px;
        border-radius: var(--radius-sm);
    }
</style>
