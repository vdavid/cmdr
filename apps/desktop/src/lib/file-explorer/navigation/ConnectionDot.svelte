<script lang="ts">
    /**
     * The small colored circle saying how live a remote place is, in both placements: the
     * breadcrumb chip and each switcher row.
     *
     * ❗ The modifier class is built from the state name, so a `ConnectionState` variant with
     * no `.smb-indicator-<state>` rule below renders as an unpainted circle. Green = a live
     * session, amber = the OS-mount fallback or a waiting sign-in, red = a changed host key,
     * grey = the backoff loop owns it, hollow = `saved`.
     */
    import { tooltip } from '$lib/tooltip/tooltip'
    import { getConnectionTooltip } from './connection-tooltips'
    import type { ConnectionState } from '../types'

    interface Props {
        state: ConnectionState
        /**
         * The volume can also be reached through the OS mount (SMB), from its backend's
         * `capabilities.hasOsMountFallback`: only then does a green dot say "directly".
         */
        hasOsMountFallback?: boolean
        /** Chip placement: a small left margin so it doesn't jam against the label. */
        breadcrumb?: boolean
        /** Off where an enclosing control says it instead (the chip's options trigger). */
        explain?: boolean
    }

    const { state, hasOsMountFallback = false, breadcrumb = false, explain = true }: Props = $props()
</script>

<span
    class="smb-indicator smb-indicator-{state}"
    class:breadcrumb-smb-indicator={breadcrumb}
    use:tooltip={explain ? getConnectionTooltip(state, { hasOsMountFallback }) : ''}
></span>

<style>
    .smb-indicator {
        width: 10px;
        height: 10px;
        border-radius: 50%;
        flex-shrink: 0;
        opacity: 0.8;
    }

    /*noinspection CssUnusedSymbol*/
    .smb-indicator-direct {
        background-color: var(--color-allow);
    }

    /*noinspection CssUnusedSymbol*/
    .smb-indicator-os_mount {
        background-color: var(--color-warning);
    }

    /* The session dropped and the backoff loop owns getting it back: grey
       and filled, so it reads as "not right now" rather than as something
       the user has to answer. */
    /*noinspection CssUnusedSymbol*/
    .smb-indicator-disconnected {
        background-color: var(--color-text-tertiary);
    }

    /* Waiting on the user, not on the network: the same amber as the
       OS-mount fallback, because both are "reachable, but not the way you
       asked for". */
    /*noinspection CssUnusedSymbol*/
    .smb-indicator-needs_sign_in {
        background-color: var(--color-warning);
    }

    /* A changed host key is the one state that is a warning about the
       server rather than about Cmdr. */
    /*noinspection CssUnusedSymbol*/
    .smb-indicator-needs_host_key_approval {
        background-color: var(--color-error);
    }

    /* A saved place nobody has dialed: hollow, so a row the user can open
       reads as "here, not connected" rather than as a failure. */
    /*noinspection CssUnusedSymbol*/
    .smb-indicator-saved {
        background-color: transparent;
        border: 1.5px solid var(--color-border-strong);
        opacity: 0.7;
    }

    /*noinspection CssUnusedSymbol*/
    .breadcrumb-smb-indicator {
        margin-left: var(--spacing-xs);
    }
</style>
