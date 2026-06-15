<script lang="ts">
    /**
     * The one loading spinner. Renders the shared `.spinner` utility (defined in `app.css`: a ring
     * with an accent-colored top, spun via the `spin` keyframe, frozen static under
     * `prefers-reduced-motion`). Use this everywhere instead of hand-rolling a ring, a `border-top`
     * trick, or a unicode `⟳`.
     *
     * Size is the only visual knob (sm 12px / md 24px / lg 32px). Pass `label` when the spinner is
     * the sole indicator of a loading state (it becomes an `aria-label` on a `role="status"`); omit
     * it when adjacent text already says "Loading…" and the spinner is decorative. Positioning
     * (centering, absolute placement) stays a concern of the wrapping element.
     */
    interface Props {
        size?: 'sm' | 'md' | 'lg'
        /** Accessible label when the spinner alone conveys "loading". Omit for decorative use. */
        label?: string
    }

    const { size = 'md', label }: Props = $props()
</script>

<span
    class="spinner"
    class:spinner-sm={size === 'sm'}
    class:spinner-md={size === 'md'}
    class:spinner-lg={size === 'lg'}
    role={label ? 'status' : undefined}
    aria-label={label}
    aria-hidden={label ? undefined : 'true'}
></span>
