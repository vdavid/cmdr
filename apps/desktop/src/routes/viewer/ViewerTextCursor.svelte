<!--
  The viewer's optional text cursor: a thin blinking bar at the selection's focus, the
  point Shift+Arrow grows the selection from. Off unless `viewer.showTextCursor` is on.

  Mounted in `.scroll-spacer`, ❌ never in `.lines-container`: that container's height is
  divided by its child count to derive `avgWrappedLineHeight`, so one extra child there
  silently corrupts virtual scrolling in word-wrap mode.

  Its own component rather than markup inlined in `+page.svelte` so it can be mounted on
  its own in `viewer.a11y.test.ts`, which never mounts the page.
-->
<script lang="ts">
    import type { TextCursorBox } from './viewer-text-cursor.svelte'

    interface Props {
        /** Where to paint, in spacer coordinates. `null` renders nothing. */
        box: TextCursorBox | null
        /** Changes whenever the focus moves, restarting the blink so a keypress always
         *  leaves the cursor solid. */
        blinkKey: string
    }

    const { box, blinkKey }: Props = $props()
</script>

{#if box !== null}
    {#key blinkKey}
        <!-- `aria-hidden`: a visual echo of the selection, which the page's own
             `aria-live` region already announces. A second one would be noise. -->
        <div
            class="text-cursor"
            aria-hidden="true"
            style="top: {box.top}px; left: {box.left}px; height: {box.height}px"
        ></div>
    {/key}
{/if}

<style>
    .text-cursor {
        position: absolute;
        /* Token-less on purpose: a caret is a hairline, the same 1px a border is. */
        width: 1px;
        background: var(--color-text-primary);
        pointer-events: none;
    }

    /* Blink is decoration, so it goes away entirely under reduced motion and the bar
       stays solid. `step-end` snaps between the two states the way a native caret does;
       an eased fade reads as a pulse. */
    @media (prefers-reduced-motion: no-preference) {
        .text-cursor {
            animation: text-cursor-blink 1s step-end infinite;
        }
    }

    @keyframes text-cursor-blink {
        0% {
            opacity: 1;
        }

        50% {
            opacity: 0;
        }
    }
</style>
