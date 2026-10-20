<script lang="ts">
    /**
     * The trust question (an SSH host key, or a TLS certificate macOS refused),
     * as a step that REPLACES the sheet's body.
     *
     * ❗ A step, ❌ not a second dialog. Trusting a key is part of one connect,
     * and stacking a second modal on the first is how a person loses track of
     * what they were doing.
     *
     * ❗ **First contact and a changed key must never share a path.** First
     * contact is routine and gets a plain primary button. A changed key is the
     * shape a man-in-the-middle takes, so it carries the red weight, says what
     * else it can mean, and puts its trust button behind a disclosure the user
     * has to open. Same component, two very different faces, and the same two
     * for a certificate: only the words differ.
     */
    import Button from '#lib/ui/Button.svelte'
    import Icon from '#lib/ui/Icon.svelte'
    import { tString } from '#lib/intl/messages.svelte.js'
    import type { TrustPrompt } from './sign-in-contract'

    interface Props {
        trust: TrustPrompt
        /** Records the key or pins the certificate, and dials again. */
        onTrust: () => void
        busy: boolean
    }

    const { trust, onTrust, busy }: Props = $props()

    const changed = $derived(trust.prompt.kind === 'changed')
    const host = $derived(trust.prompt.host)
    let disclosureOpen = $state(false)

    /** Each subject's own words, so a certificate is never called a key. */
    const words = $derived(
        trust.subject === 'host_key'
            ? {
                  title: changed
                      ? tString('servers.hostKey.changedTitle', { host })
                      : tString('servers.hostKey.firstContactTitle', { host }),
                  body: changed ? tString('servers.hostKey.changedBody') : tString('servers.hostKey.firstContactBody'),
                  fingerprintLabel: tString('servers.hostKey.fingerprintLabel'),
                  fingerprint: `${trust.prompt.algorithm} ${trust.prompt.fingerprint}`,
                  disclosure: tString('servers.hostKey.changedDisclosure'),
                  trustChanged: tString('servers.hostKey.trustNewKey'),
                  trustFirst: tString('servers.hostKey.trustAndConnect'),
              }
            : {
                  title: changed
                      ? tString('servers.certificate.changedTitle', { host })
                      : tString('servers.certificate.firstContactTitle', { host }),
                  body: changed
                      ? tString('servers.certificate.changedBody')
                      : tString('servers.certificate.firstContactBody'),
                  fingerprintLabel: tString('servers.certificate.fingerprintLabel'),
                  fingerprint: trust.prompt.fingerprint,
                  disclosure: tString('servers.certificate.changedDisclosure'),
                  trustChanged: tString('servers.certificate.trustNewCertificate'),
                  trustFirst: tString('servers.certificate.trustAndConnect'),
              },
    )
</script>

<div class="host-key" class:changed>
    <h3 class="host-key-title">
        {#if changed}
            <span class="warning-icon"><Icon name="triangle-alert" size={18} aria-hidden="true" /></span>
        {/if}
        {words.title}
    </h3>

    <p class="host-key-body">{words.body}</p>

    <div class="fingerprint">
        <span class="fingerprint-label">{words.fingerprintLabel}</span>
        <!-- Monospace and selectable: this is the string a person compares
             against `ssh-keygen -lf` or the server's certificate, character by character. -->
        <code class="fingerprint-value">{words.fingerprint}</code>
    </div>

    {#if changed}
        <details class="trust-disclosure" bind:open={disclosureOpen}>
            <summary>{words.disclosure}</summary>
            <div class="trust-disclosure-body">
                <Button variant="danger" onclick={onTrust} disabled={busy}>
                    {words.trustChanged}
                </Button>
            </div>
        </details>
    {:else}
        <div class="trust-row">
            <Button variant="primary" onclick={onTrust} disabled={busy}>
                {words.trustFirst}
            </Button>
        </div>
    {/if}
</div>

<style>
    .host-key-title {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
        margin: 0 0 var(--spacing-sm);
        font-size: var(--font-size-lg);
        font-weight: 600;
        color: var(--color-text-primary);
    }

    .host-key.changed .host-key-title {
        color: var(--color-error-text);
    }

    .warning-icon {
        display: inline-flex;
        align-items: center;
        flex-shrink: 0;
        color: var(--color-error);
    }

    .host-key-body {
        margin: 0 0 var(--spacing-md);
        color: var(--color-text-secondary);
    }

    .fingerprint {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
        padding: var(--spacing-md);
        background-color: var(--color-bg-secondary);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-md);
    }

    .host-key.changed .fingerprint {
        border-color: var(--color-error);
    }

    .fingerprint-label {
        font-size: var(--font-size-sm);
        font-weight: 500;
        color: var(--color-text-secondary);
    }

    .fingerprint-value {
        font-family: var(--font-mono);
        font-size: var(--font-size-sm);
        color: var(--color-text-primary);
        overflow-wrap: anywhere;
        user-select: text;
    }

    .trust-row,
    .trust-disclosure {
        margin-top: var(--spacing-lg);
    }

    /* No `cursor: pointer`: Cmdr sets `cursor: default` globally for native feel,
       and links are the only sanctioned exception (`ui/LinkButton.svelte`). */
    .trust-disclosure summary {
        user-select: none;
        color: var(--color-text-secondary);
    }

    .trust-disclosure-body {
        margin-top: var(--spacing-md);
    }
</style>
