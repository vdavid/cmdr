<script lang="ts">
    /**
     * The add form: an address, whatever that protocol signs in with, and an
     * Advanced disclosure for the rest.
     *
     * ❗ **Address first, protocol second.** People have an address, not a
     * protocol, so the field comes first and the toggle follows what they typed
     * (`address-parser.ts`). The toggle stays editable: a wrong read costs one
     * click, and asking someone to classify their own NAS before typing it costs
     * the feature.
     *
     * ❗ **SMB asks for nothing here.** Its connect is a share mount, and the
     * credential question comes from the listing or the mount when one refuses.
     * Putting a password field in front of a NAS that lets guests in would ask
     * for something nobody needs.
     */
    import { open as openFilePicker } from '@tauri-apps/plugin-dialog'
    import Button from '$lib/ui/Button.svelte'
    import Checkbox from '$lib/ui/Checkbox.svelte'
    import TextInput from '$lib/ui/TextInput.svelte'
    import ToggleGroup, { type ToggleGroupOption } from '$lib/ui/ToggleGroup.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import { getAppLogger } from '$lib/logging/logger'
    import type { ServerProtocol } from '$lib/ipc/bindings'
    import type { ServerForm } from './server-form'

    interface Props {
        form: ServerForm
        disabled: boolean
        /** ❗ Off in edit mode: changing which protocol a saved server speaks makes it a different server. */
        protocolEditable: boolean
        /**
         * ❗ Off in edit mode, and the address and username go with the protocol
         * toggle. Rust mints the volume id from `(host, port, username)` and
         * `sftp_known_servers::remember` keys on the same tuple, so an edited one
         * upserts a SECOND saved entry beside the first rather than moving
         * anything: the hub grows a duplicate row and the old id's tabs are
         * orphaned. The honest path to a new identity is Forget then Add, which
         * `identityHint` says.
         */
        identityEditable: boolean
        /** The two lines under the locked identity fields, saying what to do instead. */
        identityHint?: string
        /** The sentence under the address field, when the last attempt was refused. */
        addressRefusal?: string
        /** Offered on `not_a_webdav_server`: appends the Nextcloud collection path. Nobody knows that path. */
        onTryNextcloudAddress?: () => void
        /** The sentence under the secret field. */
        secretRefusal?: string
        /** Shown in edit mode when the backend says unattended reconnect can't work as things stand. */
        storedSecretWarning?: string
        onChange: (patch: Partial<ServerForm>) => void
        addressInput?: HTMLInputElement
    }

    /* eslint-disable prefer-const -- $bindable() requires `let` destructuring */
    let {
        form,
        disabled,
        protocolEditable,
        identityEditable,
        identityHint,
        addressRefusal,
        onTryNextcloudAddress,
        secretRefusal,
        storedSecretWarning,
        onChange,
        addressInput = $bindable(),
    }: Props = $props()

    const log = getAppLogger('servers')

    let advancedOpen = $state(false)

    const protocolOptions: ToggleGroupOption[] = $derived([
        { value: 'smb', label: tString('servers.sheet.protocolSmb') },
        { value: 'sftp', label: tString('servers.sheet.protocolSftp') },
        { value: 'webdav', label: tString('servers.sheet.protocolWebdav') },
    ])

    /** SMB signs in from the mount, not from here. */
    const asksForCredentials = $derived(form.protocol !== 'smb')
    const isSftp = $derived(form.protocol === 'sftp')

    async function browseForKeyFile() {
        try {
            const picked = await openFilePicker({ multiple: false, directory: false })
            if (typeof picked === 'string') onChange({ keyFile: picked })
        } catch (e) {
            // The picker not opening leaves the field exactly as it was, which is
            // a path the user can still type. Nothing to put on screen.
            log.warn('The key-file picker did not open: {error}', { error: String(e) })
        }
    }
</script>

<div class="field">
    <label for="server-address" class="field-label">{tString('servers.sheet.address')}</label>
    <TextInput
        id="server-address"
        bind:inputElement={addressInput}
        value={form.address}
        oninput={(e: Event) => {
            onChange({ address: (e.currentTarget as HTMLInputElement).value })
        }}
        disabled={disabled || !identityEditable}
        invalid={addressRefusal !== undefined}
        aria-describedby={addressRefusal ? 'server-address-refusal' : 'server-address-help'}
        placeholder={tString('servers.sheet.addressPlaceholder')}
        autocapitalize="off"
        autocomplete="off"
        spellcheck={false}
    />
    {#if addressRefusal}
        <p id="server-address-refusal" class="field-refusal" role="alert">{addressRefusal}</p>
        {#if onTryNextcloudAddress}
            <div class="remedy-row">
                <Button size="mini" onclick={onTryNextcloudAddress} {disabled}>
                    {tString('servers.sheet.tryNextcloudAddress')}
                </Button>
            </div>
        {/if}
    {:else if identityEditable}
        <p id="server-address-help" class="field-help">{tString('servers.sheet.addressHelp')}</p>
    {/if}
    <!-- ❗ No "paste whatever you have" line under a field nobody can type in.
         The locked group's own sentence sits under the username instead, where
         it covers all three of address, protocol, and account. -->
</div>

<div class="field">
    <ToggleGroup
        semantics="tabs"
        value={form.protocol}
        options={protocolOptions}
        onChange={(value: string) => {
            onChange({ protocol: value as ServerProtocol })
        }}
        disabled={disabled || !protocolEditable}
        ariaLabel={tString('servers.sheet.protocolLegend')}
        fullWidth
    />
</div>

{#if asksForCredentials}
    <div class="field">
        <label for="server-username" class="field-label">{tString('servers.sheet.username')}</label>
        <TextInput
            id="server-username"
            value={form.username}
            oninput={(e: Event) => {
                onChange({ username: (e.currentTarget as HTMLInputElement).value })
            }}
            disabled={disabled || !identityEditable}
            aria-describedby={identityHint ? 'server-identity-hint' : undefined}
            autocomplete="username"
            autocapitalize="off"
            spellcheck={false}
        />
        {#if identityHint}
            <p id="server-identity-hint" class="field-help">{identityHint}</p>
        {/if}
    </div>

    <div class="field">
        <label for="server-secret" class="field-label">{tString('servers.sheet.password')}</label>
        <TextInput
            id="server-secret"
            type="password"
            value={form.secret}
            oninput={(e: Event) => {
                onChange({ secret: (e.currentTarget as HTMLInputElement).value })
            }}
            {disabled}
            invalid={secretRefusal !== undefined}
            aria-describedby={secretRefusal ? 'server-secret-refusal' : undefined}
            autocomplete="current-password"
        />
        {#if secretRefusal}
            <p id="server-secret-refusal" class="field-refusal" role="alert">{secretRefusal}</p>
        {/if}
    </div>

    <div class="field">
        <Checkbox
            checked={form.remember}
            onCheckedChange={(checked: boolean) => {
                onChange({ remember: checked })
            }}
            {disabled}
        >
            {tString('servers.sheet.remember')}
        </Checkbox>
    </div>

    {#if storedSecretWarning}
        <p class="stored-secret-warning" role="status">{storedSecretWarning}</p>
    {/if}
{/if}

<details class="advanced" bind:open={advancedOpen}>
    <summary>{tString('servers.sheet.advanced')}</summary>
    <div class="advanced-body">
        <div class="field">
            <label for="server-name" class="field-label">{tString('servers.sheet.name')}</label>
            <TextInput
                id="server-name"
                value={form.displayName}
                oninput={(e: Event) => {
                    onChange({ displayName: (e.currentTarget as HTMLInputElement).value })
                }}
                {disabled}
                placeholder={form.address}
            />
        </div>

        {#if asksForCredentials}
            <div class="field">
                <label for="server-remote-root" class="field-label">{tString('servers.sheet.remoteFolder')}</label>
                <TextInput
                    id="server-remote-root"
                    value={form.remoteRoot}
                    oninput={(e: Event) => {
                        onChange({ remoteRoot: (e.currentTarget as HTMLInputElement).value })
                    }}
                    {disabled}
                    placeholder="/"
                    autocapitalize="off"
                    spellcheck={false}
                />
            </div>
        {/if}

        {#if isSftp}
            <div class="field">
                <label for="server-key-file" class="field-label">{tString('servers.sheet.keyFile')}</label>
                <div class="path-row">
                    <TextInput
                        id="server-key-file"
                        value={form.keyFile}
                        oninput={(e: Event) => {
                            onChange({ keyFile: (e.currentTarget as HTMLInputElement).value })
                        }}
                        {disabled}
                        autocapitalize="off"
                        spellcheck={false}
                    />
                    <Button onclick={() => void browseForKeyFile()} {disabled}>
                        {tString('servers.sheet.browse')}
                    </Button>
                </div>
            </div>

            <div class="field">
                <Checkbox
                    checked={form.useAgent}
                    onCheckedChange={(checked: boolean) => {
                        onChange({ useAgent: checked })
                    }}
                    {disabled}
                >
                    {tString('servers.sheet.useAgent')}
                </Checkbox>
            </div>
        {/if}

        {#if asksForCredentials}
            <div class="field">
                <Checkbox
                    checked={form.autoReconnect}
                    onCheckedChange={(checked: boolean) => {
                        onChange({ autoReconnect: checked })
                    }}
                    {disabled}
                >
                    {tString('servers.sheet.autoReconnect')}
                </Checkbox>
            </div>
        {/if}
    </div>
</details>

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

    .field-help {
        margin: var(--spacing-xs) 0 0;
        font-size: var(--font-size-sm);
        color: var(--color-text-tertiary);
    }

    .field-refusal {
        margin: var(--spacing-xs) 0 0;
        font-size: var(--font-size-sm);
        color: var(--color-error-text);
    }

    .remedy-row {
        margin-top: var(--spacing-sm);
    }

    .stored-secret-warning {
        margin: 0 0 var(--spacing-md);
        padding: var(--spacing-sm) var(--spacing-md);
        background-color: color-mix(in srgb, var(--color-warning) 15%, transparent);
        border: 1px solid var(--color-warning);
        border-radius: var(--radius-md);
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }

    /* No `cursor: pointer`: Cmdr sets `cursor: default` globally for native feel,
       and links are the only sanctioned exception (`ui/LinkButton.svelte`). */
    .advanced summary {
        user-select: none;
        color: var(--color-text-secondary);
    }

    .advanced-body {
        margin-top: var(--spacing-md);
    }

    .path-row {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
    }

    /* `TextInput`'s own frame is `.text-field`; the Browse button sits beside it
       and the field takes the rest of the row. */
    .path-row :global(.text-field) {
        flex: 1 1 auto;
        min-width: 0;
    }
</style>
