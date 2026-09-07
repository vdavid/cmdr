<script lang="ts">
    /**
     * One sheet for every credential ask in the app: adding a server, signing in
     * to one, and editing one.
     *
     * ❗ **Dialogs are for entering data; panes are for waiting.** Connecting, a
     * refusal with a retry, and "signed out" all render in the PANE
     * (`file-explorer/pane/RemoteConnectView.svelte`). This sheet exists for the
     * moment someone types.
     *
     * ❗ **It never dials.** The caller hands it an `attempt`
     * (`sign-in-contract.ts`), and the sheet calls it as many times as the user
     * retries. That is what lets SMB's three sites reuse it with their own
     * commands, and what lets S3 plug in with one more renderer.
     *
     * ❗ **It stays open across rounds and arms Cancel before the call.** A first
     * connect to a new SFTP server is three round-trips (the host key, then the
     * credentials, then connected), and a dial runs up to 30 s. A sheet that
     * closed between rounds would lose what was typed; one that armed Cancel
     * afterwards would leave a person watching a spinner they can't stop.
     */
    import { onMount, tick } from 'svelte'
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import Button from '$lib/ui/Button.svelte'
    import Spinner from '$lib/ui/Spinner.svelte'
    import HostKeyStep from './HostKeyStep.svelte'
    import ServerFormFields from './ServerFormFields.svelte'
    import SignInCredentialFields from './SignInCredentialFields.svelte'
    import { parseServerAddress } from './address-parser'
    import { refusalField, wordConnectRefusal, type ConnectRefusalKind } from './connect-refusals'
    import {
        applyParsedAddress,
        emptyServerForm,
        formFromSftpServer,
        formFromWebdavServer,
        nextcloudAddress,
        serverTargetFrom,
        type ServerForm,
    } from './server-form'
    import type {
        SignInAttemptOutcome,
        SignInSheetRequest,
        SignInSheetResult,
        SignInSubmission,
    } from './sign-in-contract'
    import {
        approveSftpHostKey,
        forgetServerSecret,
        getKnownSftpServers,
        getKnownWebdavServers,
        getSftpUnattendedReconnect,
        getWebdavUnattendedReconnect,
        hasServerSecret,
        updateSavedServer,
    } from '$lib/tauri-commands'
    import { tString } from '$lib/intl/messages.svelte'
    import { getAppLogger } from '$lib/logging/logger'
    import type { HostKeyPrompt, SavedServer } from '$lib/ipc/bindings'

    interface Props {
        request: SignInSheetRequest
        onDone: (result: SignInSheetResult) => void
    }

    const { request, onDone }: Props = $props()

    const log = getAppLogger('servers')

    /** The body: the form, the key question, or the one refusal no button can undo. */
    type Step = 'form' | 'host_key' | 'revoked'

    let step = $state<Step>(request.mode === 'sign-in' && request.hostKey ? 'host_key' : 'form')
    let hostKeyPrompt = $state<HostKeyPrompt | null>(request.mode === 'sign-in' ? (request.hostKey ?? null) : null)
    let busy = $state(false)
    // Seeded from the request: the refusal that opened the sheet is why the person
    // is being asked, and the first round should already say so.
    let refusal = $state<ConnectRefusalKind | null>(request.mode === 'sign-in' ? (request.refusal ?? null) : null)
    let form = $state<ServerForm>(emptyServerForm())
    /**
     * Sign-in mode's own fields; the add form holds its own.
     *
     * ❗ `guest` starts ON where the shape allows it: a share that lets anyone in
     * usually means to, and the account fields are one click away. ❌ Not a mode
     * rule — `guestAllowed` is the SHAPE's word, and a site that shouldn't offer
     * guest says so by passing `false` rather than by preselecting around it.
     */
    let credentials = $state({
        username: '',
        secret: '',
        remember: false,
        guest: request.mode === 'sign-in' && request.shape.kind === 'username_password' && request.shape.guestAllowed,
    })
    /** Edit mode's warning, when the backend says unattended reconnect can't work as things stand. */
    let storedSecretWarning = $state<string | null>(null)
    /** What Remember said when the sheet opened, so a flip can be written once, deliberately. */
    let rememberWhenOpened = false
    /** The submission to repeat once a host key is trusted. */
    let pendingSubmission: SignInSubmission | null = null

    let addressInput = $state<HTMLInputElement | undefined>()
    let secretInput = $state<HTMLInputElement | undefined>()

    const isEdit = $derived(request.mode === 'edit')
    /** `nothing` never opens the sheet, so a shape that reaches here always asks something. */
    const shape = $derived(request.mode === 'sign-in' ? request.shape : null)
    /** Edit mode has nothing to try: Save writes and closes. */
    const attempt = $derived(request.mode === 'edit' ? null : request.attempt)
    const editedServer = $derived(request.mode === 'edit' ? request.server : null)

    const sheetTitle = $derived.by(() => {
        if (request.mode === 'add') return tString('servers.sheet.addTitle')
        if (request.mode === 'edit') return tString('servers.sheet.editTitle', { name: request.server.displayName })
        return tString('servers.sheet.signInTitle', { name: request.endpoint.displayName })
    })

    const submitLabel = $derived.by(() => {
        if (request.mode === 'edit') return tString('servers.sheet.save')
        if (request.mode === 'add') return tString('servers.sheet.connect')
        return tString('servers.sheet.signIn')
    })

    /** The subject a refusal's sentence names: the server, and the account on it. */
    const refusalSubject = $derived.by(() => {
        if (request.mode === 'sign-in') {
            return { host: request.endpoint.host, username: request.endpoint.username ?? request.endpoint.displayName }
        }
        const parsed = parseServerAddress(form.address)
        const host = parsed.kind === 'parsed' ? parsed.host : form.address
        return { host, username: form.username || host }
    })

    const refusalText = $derived(refusal ? wordConnectRefusal(refusal, refusalSubject) : undefined)
    const refusalWhere = $derived(refusal ? refusalField(refusal) : null)
    /**
     * The one remedy worth a button: a bare Nextcloud origin answers "nothing
     * here speaks WebDAV", and the fix is a collection path nobody knows.
     * ownCloud shares it. ❌ `certificate_untrusted` gets no button, because none
     * of them could work: trusting a certificate happens in Keychain Access.
     */
    const offersNextcloudRemedy = $derived(refusal === 'not_a_webdav_server' && form.username.trim() !== '')

    const canSubmit = $derived.by(() => {
        if (busy) return false
        if (request.mode === 'sign-in') return credentials.guest || credentials.secret !== ''
        return form.address.trim() !== ''
    })

    onMount(() => {
        void seed()
    })

    /** What the sheet has to ask the backend before it can render honestly. */
    async function seed() {
        if (request.mode === 'add') {
            if (request.prefill !== undefined) {
                form.address = request.prefill
                form = applyParsedAddress(form, parseServerAddress(request.prefill))
            }
            await tick()
            addressInput?.focus()
            return
        }
        if (request.mode === 'sign-in') {
            credentials.username = request.endpoint.username ?? ''
            // ❗ The OPENER decided this, ❌ not the sheet: what "remembered"
            // costs to find out is the protocol's business (`sign-in-contract.ts`).
            credentials.remember = request.remembered
            rememberWhenOpened = request.remembered
            await tick()
            secretInput?.focus()
            return
        }
        await seedEditForm(request.server)
    }

    /**
     * Edit mode reads the per-protocol store row, because `SavedServer` carries
     * no key file, remote folder, or ssh-agent switch.
     *
     * ❗ Matched on the ADDRESS the listing published rather than on an id: the
     * volume id is minted in Rust from `(host, port, username)` and there is no
     * frontend twin of that hash, so re-deriving one here would be a second
     * spelling of an identity that must have exactly one.
     */
    async function seedEditForm(server: SavedServer) {
        if (server.protocol === 'sftp') {
            const saved = (await getKnownSftpServers()).find(
                (s) => `${s.host}:${String(s.port)}` === server.address && s.username === server.username,
            )
            if (saved) form = formFromSftpServer(saved)
        } else if (server.protocol === 'webdav') {
            const saved = (await getKnownWebdavServers()).find(
                (s) => s.url === server.address && s.username === server.username,
            )
            if (saved) form = formFromWebdavServer(saved)
        }
        form.remember = await hasServerSecret(server.id)
        rememberWhenOpened = form.remember
        storedSecretWarning = await readStoredSecretWarning(server.id, server.protocol)
        await tick()
        addressInput?.focus()
    }

    /**
     * The backend's own answer to "auto-reconnect is on and nothing happens".
     *
     * ❗ Asked when the sheet RENDERS, ❌ never derived from a rung plus a
     * credential check: the rung is decided per dial, and a derivation goes stale
     * the moment one lands elsewhere.
     */
    async function readStoredSecretWarning(id: string, protocol: SavedServer['protocol']): Promise<string | null> {
        if (protocol === 'sftp') {
            const state = await getSftpUnattendedReconnect(id)
            return state === 'needs_stored_secret' ? tString('servers.sheet.needsStoredSecret') : null
        }
        if (protocol === 'webdav') {
            const state = await getWebdavUnattendedReconnect(id)
            return state === 'no_stored_secret' ? tString('servers.sheet.needsStoredSecret') : null
        }
        return null
    }

    function close(result: SignInSheetResult) {
        onDone(result)
    }

    async function submit() {
        if (!canSubmit) return
        if (request.mode === 'edit') {
            await save()
            return
        }
        const submission = buildSubmission()
        if (!submission) {
            refusal = 'invalid_url'
            return
        }
        await run(submission)
    }

    /** What this mode is asking the caller to try. */
    function buildSubmission(): SignInSubmission | null {
        if (request.mode === 'sign-in') {
            return {
                mode: 'sign-in',
                secret: credentials.guest ? null : { secret: credentials.secret, remember: credentials.remember },
                // ❗ Only where the VARIANT says the username is editable, and
                // only for a real account: SFTP's and WebDAV's reconnect refuse a
                // changed username because the volume id IS the account, and a
                // guest has none to send.
                username:
                    shape?.kind === 'username_password' && !credentials.guest ? credentials.username.trim() : null,
            }
        }
        if (form.protocol === 'smb') return { mode: 'add_smb', address: form.address.trim() }
        const target = serverTargetFrom(form)
        if (!target) return null
        return {
            mode: 'add',
            target,
            secret: form.secret === '' ? null : { secret: form.secret, remember: form.remember },
        }
    }

    /** One round-trip, with the refusal put where the reader can act on it. */
    async function run(submission: SignInSubmission) {
        if (!attempt) return
        pendingSubmission = submission
        busy = true
        refusal = null
        const outcome = await attempt(submission)
        busy = false
        await applyOutcome(outcome)
    }

    async function applyOutcome(outcome: SignInAttemptOutcome) {
        switch (outcome.kind) {
            case 'connected':
                close({ kind: 'connected', volumeId: outcome.volumeId })
                return
            case 'handed_off':
                close({ kind: 'handed_off' })
                return
            case 'cancelled':
                close({ kind: 'cancelled' })
                return
            case 'needs_host_key':
                hostKeyPrompt = outcome.prompt
                step = 'host_key'
                return
            case 'host_key_revoked':
                // ❌ Deliberately final: no button can safely undo a revocation
                // the user's own `known_hosts` records.
                refusal = 'host_key_revoked'
                step = 'revoked'
                return
            case 'refused':
                refusal = outcome.refusal
                step = 'form'
                await tick()
                // Focus goes back to the field the sentence is about, so a retry
                // is one keystroke rather than a hunt.
                if (refusalField(outcome.refusal) === 'secret') secretInput?.focus()
                else if (refusalField(outcome.refusal) === 'address') addressInput?.focus()
                return
        }
    }

    /**
     * Records the key the user looked at, then runs the round it interrupted.
     *
     * ❗ There may BE no interrupted round: the flow hands the key question over
     * with the sheet opening straight onto this step, so the submission is
     * whatever the form holds right now. That is usually an empty secret, which
     * is correct — the dial past a freshly trusted key often succeeds from the
     * stored one, and comes back asking when it doesn't.
     */
    async function trustHostKey() {
        const submission = pendingSubmission ?? buildSubmission()
        if (!hostKeyPrompt || !submission) return
        busy = true
        const result = await approveSftpHostKey(hostKeyPrompt)
        busy = false
        if (result.outcome === 'superseded') {
            // ❗ Nothing was recorded: the server presents a different key now
            // than the one just shown. The step starts over on the REAL key
            // rather than silently trusting what was on screen.
            hostKeyPrompt = {
                host: result.host,
                port: result.port,
                algorithm: result.algorithm,
                fingerprint: result.fingerprint,
                kind: result.kind,
            }
            return
        }
        if (result.outcome === 'unreachable') {
            // Approving is a live question, and an unanswered one is not a yes.
            refusal = 'unreachable'
            step = 'form'
            return
        }
        step = 'form'
        await run(submission)
    }

    /** Edit mode: write the target, then the Remember flip, deliberately and once. */
    async function save() {
        if (!editedServer) return
        const target = serverTargetFrom(form)
        if (!target) {
            refusal = 'invalid_url'
            return
        }
        busy = true
        try {
            await updateSavedServer(target)
            await writeRememberFlip(editedServer.id)
            close({ kind: 'saved' })
        } catch (e) {
            log.warn('Saving the edited server broke down: {error}', { error: String(e) })
            refusal = 'unreachable'
        } finally {
            busy = false
        }
    }

    /**
     * ❗ Turning Remember OFF forgets the secret NOW. Turning it on can't seed one
     * (there is nothing typed to save), so it rides the next successful sign-in's
     * offer instead. ❌ Neither ever happens as a side effect of a dial: a
     * Keychain entry that exists because a connect happened to succeed is not the
     * user's choice.
     */
    async function writeRememberFlip(id: string) {
        if (form.remember === rememberWhenOpened) return
        if (!form.remember) await forgetServerSecret(id)
        rememberWhenOpened = form.remember
    }

    function handleKeydown(event: KeyboardEvent) {
        if (event.key === 'Enter' && canSubmit && step === 'form') {
            event.preventDefault()
            void submit()
        }
    }
</script>

<ModalDialog
    titleId="server-sign-in-title"
    dialogId="server-sign-in"
    onclose={() => {
        close({ kind: 'cancelled' })
    }}
    onkeydown={handleKeydown}
    containerStyle="width: 460px"
    growDownward
>
    {#snippet title()}{sheetTitle}{/snippet}

    <div class="sheet-body">
        {#if step === 'host_key' && hostKeyPrompt}
            <HostKeyStep prompt={hostKeyPrompt} onTrust={() => void trustHostKey()} {busy} />
        {:else if step === 'revoked'}
            <p class="form-refusal" role="alert">{refusalText}</p>
        {:else if request.mode === 'sign-in' && shape}
            <p class="endpoint-header">{request.endpoint.address}</p>
            <SignInCredentialFields
                {shape}
                accountLabel={request.endpoint.username ?? request.endpoint.address}
                username={credentials.username}
                secret={credentials.secret}
                remember={credentials.remember}
                guest={credentials.guest}
                disabled={busy}
                secretRefusal={refusalWhere === 'secret' ? refusalText : undefined}
                bind:secretInput
                onChange={(patch: Partial<typeof credentials>) => {
                    credentials = { ...credentials, ...patch }
                }}
            />
            {#if refusalWhere === 'form' && refusalText}
                <p class="form-refusal" role="alert">{refusalText}</p>
            {/if}
        {:else}
            <ServerFormFields
                {form}
                disabled={busy}
                protocolEditable={!isEdit}
                addressRefusal={refusalWhere === 'address' ? refusalText : undefined}
                onTryNextcloudAddress={offersNextcloudRemedy
                    ? () => {
                          form.address = nextcloudAddress(form.address, form.username)
                          refusal = null
                          void submit()
                      }
                    : undefined}
                secretRefusal={refusalWhere === 'secret' ? refusalText : undefined}
                storedSecretWarning={storedSecretWarning ?? undefined}
                bind:addressInput
                onChange={(patch: Partial<ServerForm>) => {
                    form = { ...form, ...patch }
                    // A refusal describes the attempt that earned it, and its
                    // sentence names the host off the LIVE form. Editing the form
                    // would otherwise leave that sentence on screen accusing a
                    // host Cmdr never contacted, so the edit retires it.
                    refusal = null
                    // Address first, protocol second: what was typed decides the
                    // toggle, and the toggle stays editable afterwards.
                    if (patch.address !== undefined) form = applyParsedAddress(form, parseServerAddress(patch.address))
                }}
            />
            {#if refusalWhere === 'form' && refusalText}
                <p class="form-refusal" role="alert">{refusalText}</p>
            {/if}
        {/if}
    </div>

    {#snippet footer()}
        <Button
            variant="secondary"
            onclick={() => {
                close({ kind: 'cancelled' })
            }}
        >
            {tString('servers.sheet.cancel')}
        </Button>
        {#if step === 'form'}
            <Button variant="primary" onclick={() => void submit()} disabled={!canSubmit}>
                {#if busy}
                    <span class="submit-content">
                        <Spinner size="sm" />
                        {tString('servers.sheet.connecting')}
                    </span>
                {:else}
                    {submitLabel}
                {/if}
            </Button>
        {/if}
    {/snippet}
</ModalDialog>

<style>
    .sheet-body {
        min-width: 0;
    }

    .endpoint-header {
        margin: 0 0 var(--spacing-lg);
        color: var(--color-text-secondary);
        overflow-wrap: anywhere;
    }

    .form-refusal {
        margin: var(--spacing-md) 0 0;
        font-size: var(--font-size-sm);
        color: var(--color-error-text);
    }

    .submit-content {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
    }
</style>
