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
    import { focusFirstField } from '$lib/ui/focus-trap'
    import Button from '$lib/ui/Button.svelte'
    import Spinner from '$lib/ui/Spinner.svelte'
    import HostKeyStep from './HostKeyStep.svelte'
    import ServerFormFields from './ServerFormFields.svelte'
    import SignInCredentialFields from './SignInCredentialFields.svelte'
    import { addressLooksLike, parseServerAddress } from './address-parser'
    import {
        refusalField,
        refusalShownOnOpen,
        wordConnectRefusal,
        wordRefusalHint,
        type ConnectRefusalKind,
        type RefusalField,
        type RefusalHint,
    } from './connect-refusals'
    import {
        applyParsedAddress,
        emptyServerForm,
        formFromPrefill,
        formFromSmbHost,
        isStartFolderUnderRoot,
        nameFallbackOf,
        withSavedAccount,
        nextcloudAddress,
        serverTargetFrom,
        smbAddressFrom,
        type ServerForm,
        hostWithPort,
    } from './server-form'
    import { readSavedServerOutcome, type SaveOutcome } from './server-outcomes'
    import { s3FieldProblem, s3HostOf, s3RequiredFieldOf } from './s3-form'
    import { saveTargetSecret, savedEditForm, unattendedReconnectWarning } from './saved-server-io'
    import type {
        AddIntent,
        SignInAttemptOutcome,
        SignInSheetRequest,
        SignInSheetResult,
        SignInSubmission,
    } from './sign-in-contract'
    import {
        approveSftpHostKey,
        forgetServerSecret,
        hasServerSecret,
        listSavedServers,
        savedServerId,
        updateSavedS3Account,
        updateSavedServer,
        updateSavedSmbHost,
    } from '$lib/tauri-commands'
    import { tString } from '$lib/intl/messages.svelte'
    import type { MessageKey } from '$lib/intl/keys.gen'
    import { getAppLogger } from '$lib/logging/logger'
    import type { HostKeyPrompt, SavedServer, ServerTarget } from '$lib/ipc/bindings'

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
    let refusal = $state<ConnectRefusalKind | null>(
        request.mode === 'sign-in' ? refusalShownOnOpen(request.refusal) : null,
    )
    /** The hint the last refused round came with, and the refusal it belongs under. */
    let hinted = $state<{ refusal: ConnectRefusalKind; hint: RefusalHint } | null>(null)
    /** S3's `region_mismatch`: the region the server says the bucket lives in, when it said. */
    let refusalRegion = $state<string | null>(null)
    /** `address_taken`: what the saved server already at the typed address is called. */
    let refusalTakenBy = $state<string | null>(null)
    /**
     * Edit mode: the id the place has since a save that landed, which differs from the one the
     * sheet opened on once the save moved its address. Plain, not `$state`: nothing renders it.
     */
    let savedAs: string | null = null
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
    /** Edit mode: whether the store held a secret when the sheet opened, for the password field's placeholder. */
    let secretStoredWhenOpened = $state(false)
    /** The submission to repeat once a host key is trusted. */
    let pendingSubmission: SignInSubmission | null = null
    /**
     * Edit mode opens Advanced: the root folder and the start folder, the
     * settings someone came to change beside the name, live there.
     */
    let advancedOpen = $state(request.mode === 'edit')
    /**
     * Whether the start folder has lost focus once. The inline "under the root"
     * sentence waits for it, so it doesn't flash while someone is still typing
     * `/srv/da`.
     */
    let startFolderTouched = $state(false)
    /** The account the last round sent, which is who that round's refusal is about. */
    let roundUsername = $state<string | null>(null)

    let sheetBody = $state<HTMLDivElement | undefined>()
    let addressInput = $state<HTMLInputElement | undefined>()
    let secretInput = $state<HTMLInputElement | undefined>()
    let rootInput = $state<HTMLInputElement | undefined>()
    let startFolderInput = $state<HTMLInputElement | undefined>()
    let regionInput = $state<HTMLInputElement | undefined>()
    let bucketInput = $state<HTMLInputElement | undefined>()

    const isEdit = $derived(request.mode === 'edit')
    /** `nothing` never opens the sheet, so a shape that reaches here always asks something. */
    const shape = $derived(request.mode === 'sign-in' ? request.shape : null)
    /** Edit mode has nothing to try: Save writes and closes. */
    const attempt = $derived(request.mode === 'edit' ? null : request.attempt)
    const editedServer = $derived(request.mode === 'edit' ? request.server : null)
    /**
     * The saved entry an edit reads and writes, by id: the place it was raised on
     * where a server keeps several (an S3 account's buckets, each saved on its own),
     * else the server's own id, which for a one-place server IS its place's.
     */
    const editedId = $derived(request.mode === 'edit' ? (request.placeVolumeId ?? request.server.id) : null)
    /** The edited place's listing entry, for its label: an S3 bucket's own, not its account's. */
    const editedPlace = $derived(
        request.mode === 'edit' ? request.server.places.find((p) => p.volumeId === request.placeVolumeId) : undefined,
    )
    /**
     * What an S3 edit edits. ❗ The ACCOUNT carries the name and the secret, so Edit on
     * its row (no `placeVolumeId`) renames it and hides the bucket and the per-place
     * switch; a PLACE (a bucket, or the root) keeps its switch and has no name of its
     * own, since a bucket reads as itself. `null` for every other protocol.
     */
    const s3EditScope = $derived.by((): 'account' | 'place' | null => {
        if (request.mode !== 'edit' || request.server.protocol !== 's3') return null
        return request.placeVolumeId === undefined ? 'account' : 'place'
    })
    /**
     * The saved entry an edit reads the Keychain and the store row through. An S3
     * account has no entry of its own (its id is its root's, saved or not), so it
     * reads through one of its places: they share the account's secret, and each
     * carries the account's provider, key, and name.
     */
    /** The two lines under an edit's locked identity fields: what names the thing, and how to change it. */
    const identityHintKey = $derived.by((): MessageKey => {
        if (s3EditScope === 'account') return 'servers.sheet.identityLockedS3Account'
        if (s3EditScope === 'place') return 'servers.sheet.identityLockedS3'
        return 'servers.sheet.accountLocked'
    })
    /**
     * Whether the address takes typing. ❗ In edit mode too for SFTP and WebDAV: a server
     * that MOVED keeps its favorites, tabs, and password, because Save moves it
     * (`src-tauri/src/server_move.rs`). ❌ Not an SMB host, whose share ids come off the
     * mount (`docs/notes/server-address-move.md` § "SMB, deferred").
     */
    const addressEditable = $derived(
        !isEdit || editedServer?.protocol === 'sftp' || editedServer?.protocol === 'webdav',
    )
    const storeId = $derived.by(() => {
        if (request.mode !== 'edit') return null
        if (s3EditScope === 'account') return request.server.places[0]?.volumeId ?? request.server.id
        return editedId
    })

    /**
     * ❗ The listing's `displayName` IS the label already: a name a person typed,
     * or `username@host` for a server nobody named (`nameSource: 'fallback'`).
     * The frontend never derives one.
     */
    const sheetTitle = $derived.by(() => {
        if (request.mode === 'add') return tString('servers.sheet.addTitle')
        if (request.mode === 'edit') {
            const name = s3EditScope === 'place' && editedPlace ? editedPlace.name : request.server.displayName
            return tString('servers.sheet.editTitle', { name })
        }
        return tString('servers.sheet.signInTitle', { name: request.endpoint.displayName })
    })

    /** The saved servers, so an already-saved address reads as its name and brings its account. Add mode only. */
    let savedServers = $state<SavedServer[]>([])

    /**
     * What an empty name field turns into, as a sentence: the stand-in the
     * backend would label the server with (`nameFallbackOf`), so the placeholder
     * reads as a promise rather than as a value someone already typed.
     */
    const namePlaceholder = $derived.by(() => {
        const fallback = nameFallbackOf(form, savedServers)
        return fallback === null ? undefined : tString('servers.sheet.namePlaceholder', { label: fallback })
    })

    /**
     * Edit mode's password placeholder over a stored secret. ❗ The field opens
     * empty every time (a secret is never read back out of the Keychain), and a
     * bare empty box read as "no password saved". It says what an empty field
     * does on Save, which is keep it (`writeTypedSecret`), and goes once Remember
     * is off, since Save then forgets it.
     */
    const secretPlaceholder = $derived(
        isEdit && secretStoredWhenOpened && form.remember ? tString('servers.sheet.secretKeptPlaceholder') : undefined,
    )

    const submitLabel = $derived.by(() => {
        if (request.mode === 'edit') return tString('servers.sheet.save')
        if (request.mode === 'add') return tString('servers.sheet.addAndOpen')
        return tString('servers.sheet.signIn')
    })

    /**
     * "Add anyway", offered once the check couldn't reach the server, and only
     * then (cmdr-reports#6). ❌ Never for `invalid_url`: an address nothing can
     * read is a typo, and saving it on purpose helps nobody.
     */
    const offersAddAnyway = $derived(request.mode === 'add' && (refusal === 'unreachable' || refusal === 'timed_out'))

    /** The subject a refusal's sentence names: the server, and the account on it. */
    const refusalSubject = $derived.by(() => {
        if (request.mode === 'sign-in') {
            // ❗ A round's refusal names the account that round SENT. Where the
            // username is editable, the account the sheet opened with may not be
            // the one that was turned away.
            const username = roundUsername || (request.endpoint.username ?? request.endpoint.displayName)
            return { host: request.endpoint.host, username, protocol: request.endpoint.protocol, region: refusalRegion }
        }
        // S3 has no address: the preset's endpoint host is the server it names.
        const parsed = parseServerAddress(form.address)
        const host =
            form.protocol === 's3'
                ? (s3HostOf(form.s3) ?? '')
                : parsed.kind === 'parsed'
                  ? hostWithPort(parsed.host, parsed.port, form.protocol)
                  : form.address
        return {
            host,
            username: form.username || host,
            protocol: form.protocol,
            region: refusalRegion,
            takenBy: refusalTakenBy,
        }
    })

    /**
     * The sentence under the address when it looks like another protocol than
     * the one selected. ❗ A warning and nothing more: the toggle stays where the
     * person put it, and Connect dials what it says (cmdr-reports#8). Add mode
     * only, since edit mode locks both.
     */
    const addressWarning = $derived.by(() => {
        if (request.mode !== 'add') return undefined
        const looksLike = addressLooksLike(form.address, form.protocol)
        if (looksLike === 'smb') return tString('servers.sheet.addressLooksLikeSmb')
        if (looksLike === 'sftp') return tString('servers.sheet.addressLooksLikeSftp')
        if (looksLike === 'webdav') return tString('servers.sheet.addressLooksLikeWebdav')
        return undefined
    })

    const refusalText = $derived(refusal ? wordConnectRefusal(refusal, refusalSubject) : undefined)
    const refusalWhere = $derived(refusal ? refusalField(refusal) : null)
    /** The softer line under the refusal, only while the refusal it came with is still on screen. */
    const refusalHintText = $derived(
        hinted && refusal === hinted.refusal ? wordRefusalHint(hinted.hint) : undefined,
    )

    /**
     * Whether the form's start folder sits outside its root: the backend's rule
     * (`start_folder_outside_root`), mirrored so the answer comes before a
     * round-trip. The backend stays authoritative.
     */
    const startFolderOutsideRoot = $derived(
        request.mode !== 'sign-in' &&
            form.protocol !== 'smb' &&
            !isStartFolderUnderRoot(form.remoteRoot, form.startFolder),
    )

    /** The sentence under the start folder: the last refusal about it, else the inline check once it has spoken. */
    const startFolderRefusalText = $derived.by(() => {
        if (refusalWhere === 'start_folder') return refusalText
        if (startFolderTouched && startFolderOutsideRoot) {
            return wordConnectRefusal('start_folder_outside_root', refusalSubject)
        }
        return undefined
    })

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
        // S3 can't dial without a key and the one field its preset makes the endpoint from
        // (GCS needs none: one global endpoint).
        if (form.protocol === 's3') {
            const required = s3RequiredFieldOf(form.s3)
            return form.username.trim() !== '' && (required === null || required.trim() !== '')
        }
        return form.address.trim() !== ''
    })

    /**
     * "Use us-east-2", offered once a bucket turned out to live in a region the server
     * named, on a preset that takes a TYPED region. One press switches and tries again.
     */
    const offersUseRegion = $derived(
        request.mode === 'add' &&
            refusal === 'region_mismatch' &&
            refusalRegion !== null &&
            form.protocol === 's3' &&
            (form.s3.provider === 'aws' ||
                form.s3.provider === 'b2' ||
                form.s3.provider === 'wasabi' ||
                form.s3.provider === 'other'),
    )

    onMount(() => {
        void seed()
    })

    /** What the sheet has to ask the backend before it can render honestly. */
    async function seed() {
        if (request.mode === 'add') {
            // A URL handed over from Go to path or ⌘K opens on the protocol its
            // scheme spells out: the person already said it (`formFromPrefill`).
            if (request.prefill !== undefined) form = formFromPrefill(request.prefill)
            void listSavedServers()
                .then((servers) => {
                    savedServers = servers
                    form = withSavedAccount(form, servers)
                })
                .catch(() => {})
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
            // Where the typing starts: an editable Username that's empty, else the secret.
            const usernameField = sheetBody?.querySelector<HTMLInputElement>('input#sign-in-username:not(:disabled)')
            if (usernameField && credentials.username === '') usernameField.focus()
            else secretInput?.focus()
            return
        }
        await seedEditForm(request.server)
    }

    /**
     * Edit mode reads the per-protocol store row, because `SavedServer` carries
     * no key file, remote folder, start folder, or ssh-agent switch.
     *
     * ❗ The store row's name is the RAW one, empty for a server nobody named, so
     * the name field opens holding exactly what the user typed.
     *
     * ❗ Matched on the ADDRESS the listing published rather than on an id: the
     * volume id is minted in Rust from `(host, port, username)` and there is no
     * frontend twin of that hash, so re-deriving one here would be a second
     * spelling of an identity that must have exactly one.
     */
    async function seedEditForm(server: SavedServer) {
        if (server.protocol === 'smb') {
            // An SMB host keeps no secret here and has no store row beyond what the
            // listing already carries, so there is nothing more to ask.
            form = formFromSmbHost(server)
            await tick()
            focusFirstEditableField()
            return
        }
        // An S3 account's buckets are each saved on their own, so a PLACE is what's read.
        const id = storeId ?? server.id
        form = (await savedEditForm(server, id)) ?? form
        form.remember = await hasServerSecret(id)
        rememberWhenOpened = form.remember
        secretStoredWhenOpened = form.remember
        // The warning is about "Reconnect automatically", a per-place switch an account edit doesn't show.
        storedSecretWarning = s3EditScope === 'account' ? null : await unattendedReconnectWarning(id, server.protocol)
        await tick()
        focusFirstEditableField()
    }

    /**
     * Edit locks what can't change (the protocol, the account, and an SMB host's
     * address), so it opens on the first field it lets a person change. ❌ Not
     * `addressInput.focus()`: focusing a disabled field is a silent no-op, and the
     * sheet then opened with nothing taking keys (QA 2026-09-25).
     */
    function focusFirstEditableField() {
        if (sheetBody) focusFirstField(sheetBody)
    }

    function close(result: SignInSheetResult) {
        onDone(result)
    }

    /**
     * Runs the form's round. `intent` is add mode's: "Add and open" (the
     * default, and Enter), "Add", or "Add anyway". The other modes ignore it.
     */
    async function submit(intent: AddIntent = 'open') {
        if (!canSubmit) return
        if (startFolderOutsideRoot) {
            // The backend would refuse the same thing, and says nothing a person
            // can't be told right now.
            startFolderTouched = true
            await refuse('start_folder_outside_root')
            return
        }
        // A region or an endpoint no host name can carry is a typo, and says so before anything dials.
        const s3Problem = request.mode !== 'sign-in' && form.protocol === 's3' ? s3FieldProblem(form.s3) : null
        if (s3Problem) {
            await refuse(s3Problem)
            return
        }
        if (request.mode === 'edit') {
            await save()
            return
        }
        const submission = buildSubmission(intent)
        if (!submission) {
            refusal = 'invalid_url'
            return
        }
        await run(submission)
    }

    /** What this mode is asking the caller to try. */
    function buildSubmission(intent: AddIntent = 'open'): SignInSubmission | null {
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
        if (form.protocol === 'smb') {
            return {
                mode: 'add_smb',
                address: smbAddressFrom(form.address),
                name: form.displayName.trim(),
                username: typedAccount(form.username),
                intent,
            }
        }
        const target = serverTargetFrom(form)
        if (!target) return null
        return {
            mode: 'add',
            target,
            secret: form.secret === '' ? null : { secret: form.secret, remember: form.remember },
            intent,
        }
    }

    /** An account field's value, or `null` when nothing was typed. */
    function typedAccount(username: string): string | null {
        const trimmed = username.trim()
        return trimmed === '' ? null : trimmed
    }

    /** One round-trip, with the refusal put where the reader can act on it. */
    async function run(submission: SignInSubmission) {
        if (!attempt) return
        pendingSubmission = submission
        roundUsername = submission.mode === 'sign-in' ? submission.username : null
        busy = true
        refusal = null
        hinted = null
        refusalRegion = null
        let outcome: SignInAttemptOutcome = { kind: 'refused', refusal: 'unreachable' }
        try {
            outcome = await attempt(submission)
        } catch (e) {
            // ❗ An attempt promises an outcome. One that throws anyway (a broken
            // IPC bridge) must not leave the sheet disabled behind a spinner
            // nothing will stop, so it reads as the connection that didn't happen.
            log.warn('A sign-in round broke down instead of answering: {error}', { error: String(e) })
        }
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
            case 'added':
                close({ kind: 'added', serverId: outcome.serverId })
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
                hinted = outcome.hint ? { refusal: outcome.refusal, hint: outcome.hint } : null
                refusalRegion = outcome.region ?? null
                await refuse(outcome.refusal)
                return
        }
    }

    /**
     * Puts a refusal on screen and the caret in the field it is about, so a retry
     * is one keystroke rather than a hunt.
     *
     * ❗ The root and start folder live inside Advanced, so a refusal about either
     * opens it first: a sentence under a collapsed field is one nobody reads.
     */
    async function refuse(kind: ConnectRefusalKind) {
        refusal = kind
        step = 'form'
        const where = refusalField(kind)
        if (where === 'root' || where === 'start_folder') advancedOpen = true
        await tick()
        focusField(where)
    }

    function focusField(where: RefusalField) {
        if (where === 'secret') secretInput?.focus()
        else if (where === 'address') addressInput?.focus()
        else if (where === 'root') rootInput?.focus()
        else if (where === 'start_folder') startFolderInput?.focus()
        // A preset's region IS the input that makes its endpoint; only Other has a separate one.
        else if (where === 'region') (regionInput ?? addressInput)?.focus()
        else if (where === 'bucket') bucketInput?.focus()
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

    /**
     * Edit mode: write the target, then the Remember flip, then whatever was
     * typed into the password field. Each deliberately, and each once.
     *
     * ❗ A refusal writes NOTHING, the password included: the backend saved none
     * of the edit, and a secret filed beside an edit that didn't land is half a
     * change. The sheet stays open with the sentence under the field it is about.
     *
     * ❗ The typed password lands LAST, so it wins over a box the same visit
     * turned off. A password field with text in it and Save pressed stores that
     * password; anything else is a field that doesn't do what it shows.
     */
    async function save() {
        if (!editedServer) return
        if (editedServer.protocol === 'smb') {
            await saveSmbHost(editedServer.id)
            return
        }
        const target = serverTargetFrom(form)
        if (!target) {
            refusal = 'invalid_url'
            return
        }
        busy = true
        refusal = null
        refusalTakenBy = null
        const answer = s3EditScope === 'account' ? await saveS3Account(editedServer.id) : await saveTarget(target)
        if (answer.kind === 'refused') {
            busy = false
            refusalTakenBy = answer.takenBy ?? null
            await refuse(answer.refusal)
            return
        }
        try {
            // ❗ A save that moved the address left the place under a NEW id, which is what
            // the flip below and any Save again must name: the old one names nothing now.
            if (s3EditScope !== 'account') savedAs = (await savedServerId(target)) ?? savedAs
            await writeRememberFlip(savedAs ?? storeId ?? editedServer.id)
            await writeTypedSecret(target)
            close({ kind: 'saved' })
        } catch (e) {
            // ❗ The edit already landed and no server was contacted, so the
            // sentence says the password is the one thing that didn't: a Keychain
            // refusal (Deny, a locked keychain, no secret service). Save again
            // re-saves the same edit and retries the write.
            log.warn('The edited server saved, but writing its password broke down: {error}', { error: String(e) })
            busy = false
            await refuse('saved_secret_not_updated')
        } finally {
            busy = false
        }
    }

    /** Edit mode's store write for a server with a place: the target, as the backend answered it. */
    async function saveTarget(target: ServerTarget): Promise<SaveOutcome> {
        try {
            return readSavedServerOutcome(await updateSavedServer(target, savedAs ?? editedId))
        } catch (e) {
            // Nothing confirmed the edit, which is exactly what this refusal says.
            log.warn('Saving the edited server broke down: {error}', { error: String(e) })
            return { kind: 'refused', refusal: 'save_unconfirmed' }
        }
    }

    /**
     * Edit mode on an S3 account: its name, by the row's id. ❗ Never through
     * `updateSavedServer`: a target with no bucket would save the account ROOT as a
     * new place. An account whose places all went away meanwhile (a Forget in
     * another pane) reads as the save nobody could confirm. The secret is written
     * after, like any edit's.
     */
    async function saveS3Account(id: string): Promise<SaveOutcome> {
        try {
            if (await updateSavedS3Account(id, form.displayName.trim())) return { kind: 'saved' }
        } catch (e) {
            log.warn('Saving the edited S3 account broke down: {error}', { error: String(e) })
        }
        return { kind: 'refused', refusal: 'save_unconfirmed' }
    }

    /**
     * Edit mode on an SMB host: its name and the account it's used with. ❗
     * Nothing else is written, since the address is the entry's identity and SMB
     * keeps its password per share mount, not here. A host that went away meanwhile (a Forget in another
     * pane) reads as the save nobody could confirm.
     */
    async function saveSmbHost(id: string) {
        busy = true
        refusal = null
        let found = false
        try {
            found = await updateSavedSmbHost(id, form.displayName.trim(), typedAccount(form.username))
        } catch (e) {
            log.warn('Saving the edited SMB host broke down: {error}', { error: String(e) })
        }
        busy = false
        if (found) close({ kind: 'saved' })
        else await refuse('save_unconfirmed')
    }

    /**
     * ❗ Turning Remember OFF forgets the secret NOW. Turning it on can't seed one
     * by itself, so it rides either the typed password below or the next
     * successful sign-in's offer. ❌ Neither ever happens as a side effect of a
     * dial: a Keychain entry that exists because a connect happened to succeed is
     * not the user's choice.
     */
    async function writeRememberFlip(id: string) {
        if (form.remember === rememberWhenOpened) return
        if (!form.remember) await forgetServerSecret(id)
        rememberWhenOpened = form.remember
    }

    /**
     * The password field's own write.
     *
     * ❗ An EMPTY field means "I didn't come here to change the password", ❌
     * never "store an empty one": the field opens empty every time, because a
     * stored secret is never read back out of the Keychain to prefill it.
     *
     * ❗ Keyed on the TARGET's tuple, which is the one Rust mints the volume id
     * from. After a save that moved the address that's the NEW address, where the
     * backend already moved the stored password, so the entry this writes is the
     * one the next dial reads.
     */
    async function writeTypedSecret(target: ServerTarget) {
        if (form.secret === '') return
        await saveTargetSecret(target, form.secret)
        // The store holds one now, which is exactly what the box means.
        form.remember = true
        rememberWhenOpened = true
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

    <div class="sheet-body" bind:this={sheetBody}>
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
            <!-- ❗ Sign-in mode renders no address or folder field, so every refusal that
                 isn't about the password reads here rather than vanishing. -->
            {#if refusalWhere !== 'secret' && refusalText}
                <p class="form-refusal" role="alert">{refusalText}</p>
            {/if}
        {:else}
            <ServerFormFields
                {form}
                disabled={busy}
                protocolEditable={!isEdit}
                identityEditable={!isEdit}
                {addressEditable}
                addressHelp={isEdit ? tString('servers.sheet.addressMoveHelp') : undefined}
                identityHint={isEdit ? tString(identityHintKey) : undefined}
                s3EditScope={s3EditScope ?? undefined}
                addressRefusal={refusalWhere === 'address' ? refusalText : undefined}
                regionRefusal={refusalWhere === 'region' ? refusalText : undefined}
                bucketRefusal={refusalWhere === 'bucket' ? refusalText : undefined}
                suggestedRegion={refusalRegion ?? undefined}
                onUseRegion={offersUseRegion
                    ? () => {
                          form = { ...form, s3: { ...form.s3, region: refusalRegion ?? form.s3.region } }
                          refusal = null
                          void submit()
                      }
                    : undefined}
                bind:regionInput
                bind:bucketInput
                addressRefusalHint={refusalWhere === 'address' ? refusalHintText : undefined}
                {addressWarning}
                onAddAnyway={offersAddAnyway ? () => void submit('save_unchecked') : undefined}
                onTryNextcloudAddress={offersNextcloudRemedy
                    ? () => {
                          form.address = nextcloudAddress(form.address, form.username)
                          refusal = null
                          void submit()
                      }
                    : undefined}
                secretRefusal={refusalWhere === 'secret' ? refusalText : undefined}
                {secretPlaceholder}
                rootRefusal={refusalWhere === 'root' ? refusalText : undefined}
                startFolderRefusal={startFolderRefusalText}
                storedSecretWarning={storedSecretWarning ?? undefined}
                {namePlaceholder}
                bind:advancedOpen
                onStartFolderBlur={() => {
                    startFolderTouched = true
                }}
                bind:addressInput
                bind:secretInput
                bind:rootInput
                bind:startFolderInput
                onChange={(patch: Partial<ServerForm>) => {
                    // A username the person types is theirs: the address stops steering it.
                    form = { ...form, ...patch, ...(patch.username !== undefined ? { usernameFromAddress: false } : {}) }
                    // A refusal describes the attempt that earned it, and its
                    // sentence names the host off the LIVE form. Editing the form
                    // would otherwise leave that sentence on screen accusing a
                    // host Cmdr never contacted, so the edit retires it.
                    refusal = null
                    // The account and the SFTP root follow the address, and follow
                    // the toggle too, since whether a path is a root depends on it.
                    // ❌ The toggle itself never follows the address. ❗ Add mode only:
                    // an edit's account is locked (a new address MOVES the server, it
                    // never re-signs it as someone else), and its root has its own field.
                    if (!isEdit && (patch.address !== undefined || patch.protocol !== undefined)) {
                        // An address already saved with an account brings that account.
                        form = withSavedAccount(applyParsedAddress(form, parseServerAddress(form.address)), savedServers)
                    }
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
        {#if step === 'form' && request.mode === 'add'}
            <!-- ❗ Both check the server before saving; only the primary moves a pane
                 (cmdr-reports#6: the dialog said "Add server" and its one button
                 connected). -->
            <Button variant="secondary" onclick={() => void submit('save')} disabled={!canSubmit}>
                {tString('servers.sheet.add')}
            </Button>
        {/if}
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
