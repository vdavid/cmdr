<!--
  Product page (`/product`): active use, settings adoption, feature usage, payment, retention, and feedback & errors. The
  shared range/day picker and nav live in the layout; this page renders the sections from its own loaded subset.
-->
<script lang="ts">
    import type { PageProps } from './$types'
    import ActiveUseSection from '#lib/components/sections/ActiveUseSection.svelte'
    import SettingsAdoptionSection from '#lib/components/sections/SettingsAdoptionSection.svelte'
    import FeatureUsageSection from '#lib/components/sections/FeatureUsageSection.svelte'
    import PaymentSection from '#lib/components/sections/PaymentSection.svelte'
    import RetentionSection from '#lib/components/sections/RetentionSection.svelte'
    import FeedbackErrorsSection from '#lib/components/sections/FeedbackErrorsSection.svelte'

    const { data, form }: PageProps = $props()
</script>

<div class="grid grid-cols-1 gap-6 md:grid-cols-2 xl:grid-cols-3">
    <ActiveUseSection cloudflare={data.cloudflare} license={data.license} selection={data.selection} />
    <SettingsAdoptionSection settingsAdoption={data.settingsAdoption} selection={data.selection} />
    <PaymentSection paddle={data.paddle} selection={data.selection} />
    <RetentionSection paddle={data.paddle} selection={data.selection} />
    <FeatureUsageSection featureUsage={data.featureUsage} selection={data.selection} />
    <FeedbackErrorsSection
        feedbackAndErrors={data.feedbackAndErrors}
        selection={data.selection}
        answerError={form && 'error' in form ? form.error : null}
    />
</div>
