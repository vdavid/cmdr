<script lang="ts">
    import CatalogPage from '../CatalogPage.svelte'
    import IconsSection from './sections/IconsSection.svelte'
    import SpinnersSection from './sections/SpinnersSection.svelte'
    import StatusBadgesSection from './sections/StatusBadgesSection.svelte'
    import IllustrationsSection from './sections/IllustrationsSection.svelte'
    import AnimationsSection from './sections/AnimationsSection.svelte'
    import IndexingStatusSection from './sections/IndexingStatusSection.svelte'

    interface Props {
        /** Optional sub-anchor (e.g. `'icons'`). Catalog scrolls to `#graphics-<anchor>` when this changes. */
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
    const SUB_IDS = ['icons', 'spinners', 'status-badges', 'illustrations', 'animations', 'drive-indexing'] as const
</script>

<!-- Renders in dev, and ALSO in the i18n screenshot-capture build (`__CMDR_I18N_CAPTURE__`,
     a Vite define true only there, dead-code-eliminated in prod) so the capture driver can
     screenshot the drive-indexing checklist tiles by their anchors. Zero shipping impact. -->
{#if import.meta.env.DEV || __CMDR_I18N_CAPTURE__}
    <CatalogPage
        prefix="graphics"
        subIds={SUB_IDS}
        route="/dev/graphics"
        title="Graphics"
        {targetAnchor}
        {onSectionInView}
    >
        {#snippet description()}
            Every visual asset the app renders: icons, spinners, status badges, illustrations, and animations. Each item
            carries a tooltip describing where it shows up in the app, so a designer can review them for consistency.
        {/snippet}

        <IconsSection />
        <SpinnersSection />
        <StatusBadgesSection />
        <IllustrationsSection />
        <AnimationsSection />
        <IndexingStatusSection />
    </CatalogPage>
{/if}
