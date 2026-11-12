<script lang="ts">
    /**
     * The NFS block of the add form: the export, and the uid and groups every request
     * acts as. `ServerFormFields` renders it under the name while NFS is selected.
     *
     * ❗ **It asks the server as the person types** (`nfs-discovery.svelte.ts`): the
     * export list once the address names a server, the identity guess once an export
     * is picked. Both wait for the typing to settle, and neither gates anything: a
     * refused listing keeps the export typeable, and a typed uid is never overwritten.
     *
     * ❗ **No password and no Remember box.** `AUTH_SYS` carries a uid, not a secret,
     * so "User ID" is a setting of the place, ❌ not its identity.
     */
    import { untrack } from 'svelte'
    import Combobox, { type ComboboxItem } from '#lib/ui/Combobox.svelte'
    import TextInput from '#lib/ui/TextInput.svelte'
    import { tString } from '#lib/intl/messages.svelte.js'
    import type { MessageKey } from '#lib/intl/keys.gen.js'
    import type { NfsIdentitySourceWire } from '#lib/ipc/bindings.js'
    import { guessNfsIdentity, listNfsExports } from '#lib/tauri-commands/index.js'
    import { parseServerAddress } from './address-parser'
    import { hostWithPort } from './server-form'
    import {
        nfsIdentityWarns,
        readNfsOtherGids,
        withGuessedIdentity,
        withTypedIdentity,
        type NfsFormFields,
    } from './nfs-form'
    import { createNfsDiscovery, type NfsDiscoveryDeps } from './nfs-discovery.svelte'

    /** How long the address has to sit still before the server is asked for its exports. */
    const LIST_SETTLE_MS = 500
    /** How long the export has to sit still before the server is asked who owns its files. */
    const GUESS_SETTLE_MS = 600

    /** The line under the ids: where the guess on screen came from. */
    const IDENTITY_SOURCE_KEY: Record<NfsIdentitySourceWire, MessageKey> = {
        most_common_owner: 'servers.sheet.nfsIdentityMostCommonOwner',
        root_owner: 'servers.sheet.nfsIdentityRootOwner',
        this_mac: 'servers.sheet.nfsIdentityThisMac',
    }

    interface Props {
        fields: NfsFormFields
        /** The form's address, which names the server the questions go to. */
        address: string
        disabled: boolean
        /** ❗ Off in edit mode: the export is the place's identity, and another one is another place. */
        exportEditable: boolean
        /** Edit mode: the line under the locked export saying how to reach another one. */
        exportHint?: string
        /** The sentence under the export, when the last attempt was refused over it. */
        exportRefusal?: string
        onChange: (patch: Partial<NfsFormFields>) => void
        /** The export's input, so a refusal under it can put the caret there. */
        exportInput?: HTMLInputElement
        /** Test seam: the two backend asks. */
        discoveryDeps?: NfsDiscoveryDeps
    }

    /* eslint-disable prefer-const -- $bindable() requires `let` destructuring */
    let {
        fields,
        address,
        disabled,
        exportEditable,
        exportHint,
        exportRefusal,
        onChange,
        exportInput = $bindable(),
        discoveryDeps = {
            listNfsExports: ({ host, port }) => listNfsExports(host, port),
            guessNfsIdentity: ({ host, port, exportPath }) => guessNfsIdentity(host, port, exportPath),
        },
    }: Props = $props()

    const discovery = untrack(() => createNfsDiscovery(discoveryDeps))

    /** The server the address names. ❗ Primitives, so a keystroke that keeps the server doesn't re-ask it. */
    const parsedAddress = $derived(parseServerAddress(address))
    const host = $derived(parsedAddress.kind === 'parsed' ? parsedAddress.host : '')
    /** A port another protocol's scheme named isn't NFS's, the rule `serverTargetFrom` dials by. */
    const port = $derived(
        parsedAddress.kind === 'parsed' && (parsedAddress.protocol ?? 'nfs') === 'nfs'
            ? (parsedAddress.port ?? null)
            : null,
    )
    const exportKey = $derived(fields.exportPath.trim())
    const identityFromGuess = $derived(fields.identityFromGuess)
    /** The server as a sentence names it, with its port when it isn't 2049. */
    const hostLabel = $derived(hostWithPort(host, port ?? undefined, 'nfs'))

    $effect(() => {
        const server = host
        const serverPort = port
        if (!exportEditable) return
        if (server === '') {
            untrack(() => {
                discovery.clear()
            })
            return
        }
        const timer = setTimeout(() => void discovery.listFor({ host: server, port: serverPort }), LIST_SETTLE_MS)
        return () => {
            clearTimeout(timer)
        }
    })

    $effect(() => {
        const server = host
        const serverPort = port
        const exportPath = exportKey
        if (!exportEditable || !identityFromGuess || server === '' || exportPath === '') return
        const timer = setTimeout(() => {
            void discovery.guessFor({ host: server, port: serverPort, exportPath }).then((guess) => {
                // ❗ The fields as they are NOW: the person may have typed an id, or moved
                // to another export, while the server was answering.
                if (guess && fields.exportPath.trim() === exportPath) onChange(withGuessedIdentity(fields, guess))
            })
        }, GUESS_SETTLE_MS)
        return () => {
            clearTimeout(timer)
        }
    })

    const exportItems = $derived<ComboboxItem[]>(
        discovery.listing.kind === 'listed'
            ? discovery.listing.exports.map((exportPath) => ({ value: exportPath, label: exportPath }))
            : [],
    )

    /** The line under the export: a refusal outranks what the listing said, which outranks the help. */
    const exportLine = $derived.by((): { text: string; tone: 'refusal' | 'warning' | 'help' } => {
        if (exportRefusal) return { text: exportRefusal, tone: 'refusal' }
        if (!exportEditable) return { text: exportHint ?? '', tone: 'help' }
        const listing = discovery.listing
        if (listing.kind === 'refused') {
            return { text: tString('servers.sheet.nfsExportsRefused', { host: hostLabel }), tone: 'warning' }
        }
        if (listing.kind === 'no_nfs_v3') {
            return { text: tString('servers.refusal.noNfsV3', { host: hostLabel }), tone: 'warning' }
        }
        if (listing.kind === 'unreachable') {
            return { text: tString('servers.sheet.nfsExportsUnreachable', { host: hostLabel }), tone: 'warning' }
        }
        if (listing.kind === 'listed' && listing.exports.length === 0) {
            return { text: tString('servers.sheet.nfsExportsNone', { host: hostLabel }), tone: 'warning' }
        }
        return { text: tString('servers.sheet.nfsExportHelp'), tone: 'help' }
    })

    /** The line under "Other group IDs" when they don't read: why Connect is waiting. */
    const otherGidsProblem = $derived.by((): string | undefined => {
        const reading = readNfsOtherGids(fields.otherGids)
        if (reading.kind === 'not_ids') return tString('servers.sheet.nfsOtherGidsNotIds')
        if (reading.kind === 'too_many') return tString('servers.sheet.nfsOtherGidsTooMany', { count: reading.count })
        return undefined
    })

    const identityLine = $derived(
        fields.identitySource === null
            ? tString('servers.sheet.nfsIdentityHelp')
            : tString(IDENTITY_SOURCE_KEY[fields.identitySource]),
    )

    let exportBox = $state<HTMLDivElement | undefined>()
    $effect(() => {
        // `Combobox` renders its own input, so the caret's target is found inside it.
        exportInput = exportBox?.querySelector('input') ?? undefined
    })
</script>

<div class="field">
    <span id="server-nfs-export-label" class="field-label">{tString('servers.sheet.nfsExport')}</span>
    <div class="export-box" class:invalid={exportRefusal !== undefined} bind:this={exportBox}>
        <Combobox
            items={exportItems}
            inputValue={fields.exportPath}
            onInputValueChange={(value: string) => {
                onChange({ exportPath: value })
            }}
            loading={discovery.listing.kind === 'loading'}
            disabled={disabled || !exportEditable}
            placeholder={tString('servers.sheet.examplePlaceholder', { example: '/volume1/data' })}
            ariaLabel={tString('servers.sheet.nfsExport')}
            emptyText={tString('servers.sheet.nfsExportsEmptyList')}
        />
    </div>
    {#if exportLine.text !== ''}
        {#if exportLine.tone === 'refusal'}
            <p id="server-nfs-export-refusal" class="field-refusal" role="alert">{exportLine.text}</p>
        {:else}
            <!-- `status`: it arrives on its own once the server answers, and asks for a look. -->
            <p
                id="server-nfs-export-help"
                class={exportLine.tone === 'warning' ? 'field-warning' : 'field-help'}
                role="status"
            >
                {exportLine.text}
            </p>
        {/if}
    {/if}
</div>

<div class="identity-row">
    <div class="field">
        <label for="server-nfs-uid" class="field-label">{tString('servers.sheet.nfsUid')}</label>
        <TextInput
            id="server-nfs-uid"
            value={fields.uid}
            oninput={(e: Event) => {
                onChange(withTypedIdentity(fields, { uid: (e.currentTarget as HTMLInputElement).value }))
            }}
            {disabled}
            warning={nfsIdentityWarns(fields)}
            inputmode="numeric"
            aria-describedby="server-nfs-identity-help"
            autocomplete="off"
            spellcheck={false}
        />
    </div>
    <div class="field">
        <label for="server-nfs-gid" class="field-label">{tString('servers.sheet.nfsGid')}</label>
        <TextInput
            id="server-nfs-gid"
            value={fields.gid}
            oninput={(e: Event) => {
                onChange(withTypedIdentity(fields, { gid: (e.currentTarget as HTMLInputElement).value }))
            }}
            {disabled}
            inputmode="numeric"
            aria-describedby="server-nfs-identity-help"
            autocomplete="off"
            spellcheck={false}
        />
    </div>
</div>
<div class="field other-gids">
    <label for="server-nfs-other-gids" class="field-label">{tString('servers.sheet.nfsOtherGids')}</label>
    <TextInput
        id="server-nfs-other-gids"
        value={fields.otherGids}
        oninput={(e: Event) => {
            onChange(withTypedIdentity(fields, { otherGids: (e.currentTarget as HTMLInputElement).value }))
        }}
        {disabled}
        invalid={otherGidsProblem !== undefined}
        placeholder={tString('servers.sheet.examplePlaceholder', { example: '100, 1001' })}
        aria-describedby={otherGidsProblem === undefined ? 'server-nfs-identity-help' : 'server-nfs-other-gids-problem'}
        autocomplete="off"
        spellcheck={false}
    />
    {#if otherGidsProblem !== undefined}
        <!-- `status`: it changes as the person types, and asks for a look. -->
        <p id="server-nfs-other-gids-problem" class="field-refusal" role="status">{otherGidsProblem}</p>
    {/if}
</div>
<p id="server-nfs-identity-help" class="field-help identity-help">{identityLine}</p>
{#if nfsIdentityWarns(fields)}
    <!-- A warning, ❌ never a block: on a `no_root_squash` export, 0 is exactly what's meant. -->
    <p class="field-warning identity-help" role="status">{tString('servers.sheet.nfsUidZeroWarning')}</p>
{/if}

<style>
    .field {
        margin-bottom: var(--spacing-md);
    }

    .field-label {
        display: block;
        margin-bottom: var(--spacing-xs);
        font-size: var(--font-size-sm);
        font-weight: 500;
        color: var(--color-text-secondary);
    }

    .field-help,
    .field-refusal,
    .field-warning {
        margin: var(--spacing-xs) 0 0;
        font-size: var(--font-size-sm);
    }

    .field-help {
        color: var(--color-text-tertiary);
    }

    .field-refusal {
        color: var(--color-error-text);
    }

    .field-warning {
        color: var(--color-warning-text);
    }

    /* The two ids side by side: they're one setting, the identity every request carries. */
    .identity-row {
        display: grid;
        grid-template-columns: 1fr 1fr;
        gap: var(--spacing-md);
    }

    .identity-row .field {
        margin-bottom: 0;
    }

    .other-gids {
        margin-top: var(--spacing-md);
        margin-bottom: 0;
    }

    .identity-help {
        margin-bottom: var(--spacing-md);
    }

    /* A refusal under the export marks its frame, as `TextInput`'s `invalid` does. */
    .export-box.invalid :global(.combobox-control) {
        border-color: var(--color-error);
    }
</style>
