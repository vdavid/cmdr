<script lang="ts">
    /**
     * `File systems > Servers (SFTP, WebDAV)`: one card, the SSH host keys this
     * Mac has trusted, each with a Forget button.
     *
     * ❗ **Saved servers are NOT here.** They live in the hub, the switcher's
     * "Servers" row, which is a pane state so a person can navigate, sort, and act
     * on them the way they do files. This page keeps only what the hub can't show.
     *
     * ❗ The page has no CONTROL of its own, so both its sidebar row and its
     * "host key" search hit come from the anchoring `SearchableRow` in
     * `ServersSection.rows.ts`, never a registry entry.
     */
    import { onMount } from 'svelte'
    import SettingsSection from '../components/SettingsSection.svelte'
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import Button from '$lib/ui/Button.svelte'
    import DateLabel from '$lib/ui/DateLabel.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import { createShouldShow } from '$lib/settings/settings-search'
    import { listTrustedSftpHostKeys, forgetSftpHostKey } from '$lib/tauri-commands'
    import type { TrustedHostKey } from '$lib/ipc/bindings'
    import { confirmDialog } from '$lib/utils/confirm-dialog'
    import { getAppLogger } from '$lib/logging/logger'

    interface Props {
        searchQuery: string
    }

    const { searchQuery }: Props = $props()

    const log = getAppLogger('settings')

    const shouldShow = $derived(createShouldShow(searchQuery))

    // `null` until the store has answered, so the empty sentence never flashes in
    // front of someone who does have trusted keys.
    let keys = $state<TrustedHostKey[] | null>(null)
    let forgetting = $state<string | null>(null)

    /** What a key is filed under: host, port, and algorithm together. */
    function keyId(key: TrustedHostKey): string {
        return `${key.host}:${String(key.port)}/${key.algorithm}`
    }

    async function load(): Promise<void> {
        try {
            keys = await listTrustedSftpHostKeys()
        } catch (error) {
            log.warn('Could not read the trusted host keys: {error}', { error: String(error) })
            keys = []
        }
    }

    onMount(() => {
        void load()
    })

    // Confirmed, because the next connection to that server stops being routine:
    // it becomes first contact again, with a fingerprint to check.
    async function handleForget(key: TrustedHostKey): Promise<void> {
        const confirmed = await confirmDialog(
            tString('settings.servers.trustedHostKeys.confirm', { host: key.host }),
            tString('settings.servers.trustedHostKeys.confirmTitle'),
        )
        if (!confirmed) return
        forgetting = keyId(key)
        try {
            await forgetSftpHostKey(key.host, key.port, key.algorithm)
            await load()
        } catch (error) {
            log.warn('Could not forget a trusted host key: {error}', { error: String(error) })
        } finally {
            forgetting = null
        }
    }

    /** `approvedAt` is ISO 8601; `DateLabel` speaks seconds. */
    function approvedSeconds(key: TrustedHostKey): number | null {
        const parsed = Date.parse(key.approvedAt)
        return Number.isNaN(parsed) ? null : Math.floor(parsed / 1000)
    }
</script>

<SettingsSection title={tString('settings.section.servers')}>
    {#if shouldShow('row:network.trustedHostKeys')}
        <SectionCard label={tString('settings.servers.card.trustedHostKeys')}>
            <p class="card-description">{tString('settings.servers.trustedHostKeys.description')}</p>

            {#if keys !== null}
                {#if keys.length === 0}
                    <p class="empty">{tString('settings.servers.trustedHostKeys.empty')}</p>
                {:else}
                    <ul class="key-list">
                        {#each keys as key (keyId(key))}
                            <li class="key-row">
                                <div class="key-facts">
                                    <!-- The Forget button points here rather than
                                         carrying the host in its own name: one short
                                         label, and a screen reader still says which
                                         row it belongs to. -->
                                    <span class="key-host" id="host-key-{keyId(key)}">{key.host}:{key.port}</span>
                                    <span class="key-fingerprint">{key.algorithm} · {key.fingerprint}</span>
                                    <span class="key-approved">
                                        {tString('settings.servers.trustedHostKeys.approvedPrefix')}
                                        <DateLabel modifiedAt={approvedSeconds(key)} />
                                    </span>
                                </div>
                                <Button
                                    variant="secondary"
                                    size="mini"
                                    disabled={forgetting === keyId(key)}
                                    aria-describedby="host-key-{keyId(key)}"
                                    onclick={() => void handleForget(key)}
                                >
                                    {tString('settings.servers.trustedHostKeys.forget')}
                                </Button>
                            </li>
                        {/each}
                    </ul>
                {/if}
            {/if}
        </SectionCard>
    {/if}
</SettingsSection>

<style>
    .card-description,
    .empty {
        margin: 0;
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
    }

    .empty {
        margin-top: var(--spacing-sm);
    }

    .key-list {
        margin: var(--spacing-sm) 0 0;
        padding: 0;
        list-style: none;
    }

    .key-row {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: var(--spacing-md);
        padding: var(--spacing-sm) 0;
        border-bottom: 1px solid var(--color-border-subtle);
    }

    .key-row:last-child {
        border-bottom: none;
    }

    .key-facts {
        display: flex;
        min-width: 0;
        flex-direction: column;
        gap: var(--spacing-xxs);
    }

    .key-host {
        color: var(--color-text-primary);
    }

    .key-fingerprint {
        font-family: var(--font-mono);
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
        overflow-wrap: anywhere;
    }

    .key-approved {
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }
</style>
