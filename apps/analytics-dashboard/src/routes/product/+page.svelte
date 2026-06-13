<!--
  Product page (`/product`): active use, payment, retention, and feedback & errors. The shared range/day
  picker and nav live in the layout; this page renders the four sections from its own loaded subset.
-->
<script lang="ts">
    import ActiveUseSection from '$lib/components/sections/ActiveUseSection.svelte'
    import PaymentSection from '$lib/components/sections/PaymentSection.svelte'
    import RetentionSection from '$lib/components/sections/RetentionSection.svelte'
    import FeedbackErrorsSection from '$lib/components/sections/FeedbackErrorsSection.svelte'

    let { data } = $props()
</script>

<div class="grid grid-cols-1 gap-6 md:grid-cols-2 xl:grid-cols-3">
    <ActiveUseSection cloudflare={data.cloudflare} license={data.license} selection={data.selection} />
    <PaymentSection paddle={data.paddle} selection={data.selection} />
    <RetentionSection paddle={data.paddle} selection={data.selection} />
    <FeedbackErrorsSection feedbackAndErrors={data.feedbackAndErrors} selection={data.selection} />
</div>
