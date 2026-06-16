<script lang="ts">
    import { onDestroy, onMount } from 'svelte'
    import { commands, type SmbDiagnosticsDto, type SmbVolumeRef } from '$lib/ipc/bindings'
    import { tooltip } from '$lib/tooltip/tooltip'
    import Select, { type SelectItem } from '$lib/ui/Select.svelte'
    import { formatInteger } from '$lib/intl/number-format'

    type Loadable = 'idle' | 'loading' | 'ready' | 'error'

    let volumes = $state<SmbVolumeRef[]>([])
    let selectedVolumeId = $state<string | null>(null)
    let diag = $state<SmbDiagnosticsDto | null>(null)
    let loadState: Loadable = $state('idle')
    let errorMessage = $state<string>('')
    let autoRefresh = $state(true)
    let intervalMs = $state(1000)
    let lastPollAt = $state(0)
    let pollHandle: ReturnType<typeof setInterval> | undefined

    onMount(async () => {
        await refreshVolumes()
        startPolling()
    })

    onDestroy(() => { stopPolling(); })

    function startPolling() {
        stopPolling()
        if (!autoRefresh) return
        pollHandle = setInterval(() => void poll(), intervalMs)
    }

    function stopPolling() {
        if (pollHandle) {
            clearInterval(pollHandle)
            pollHandle = undefined
        }
    }

    $effect(() => {
        // Restart polling when toggle or interval changes. The bare reads
        // register reactive dependencies for Svelte 5's $effect; the `void`
        // tells ESLint they're intentional, not orphan expressions.
        void autoRefresh
        void intervalMs
        startPolling()
    })

    async function refreshVolumes() {
        const next = await commands.listSmbVolumes()
        volumes = next
        if (!selectedVolumeId && next.length > 0) {
            selectedVolumeId = next[0].volume_id
        }
        if (selectedVolumeId && !next.some((v) => v.volume_id === selectedVolumeId)) {
            selectedVolumeId = next[0]?.volume_id ?? null
        }
        await poll()
    }

    async function poll() {
        if (!selectedVolumeId) {
            diag = null
            loadState = 'idle'
            return
        }
        loadState = loadState === 'ready' ? 'ready' : 'loading'
        try {
            const result = await commands.getSmbDiagnostics(selectedVolumeId)
            // tauri-specta wraps Result<T, E> as { status: "ok" | "error", data | error }.
            // The typed binding already narrows this, so the type-cast check is unnecessary.
            if (typeof result === 'object' && 'status' in result) {
                if (result.status === 'ok') {
                    diag = result.data
                    loadState = 'ready'
                    errorMessage = ''
                } else {
                    diag = null
                    loadState = 'error'
                    errorMessage = result.error
                }
            } else {
                // Some specta versions return the value directly on Ok and throw on Err.
                diag = result
                loadState = 'ready'
                errorMessage = ''
            }
        } catch (e) {
            diag = null
            loadState = 'error'
            errorMessage = String(e)
        } finally {
            lastPollAt = Date.now()
        }
    }

    const volumeItems = $derived<SelectItem[]>(
        volumes.map((v) => ({
            value: v.volume_id,
            label: `${v.name} (${v.server})${v.disconnected ? ' — disconnected' : ''}`,
        })),
    )

    const intervalItems: SelectItem[] = [
        { value: '250', label: '250 ms' },
        { value: '500', label: '500 ms' },
        { value: '1000', label: '1 s' },
        { value: '2000', label: '2 s' },
        { value: '5000', label: '5 s' },
    ]

    async function handleVolumeChange(volumeId: string) {
        selectedVolumeId = volumeId || null
        await poll()
    }

    function fmtBytes(n: number): string {
        if (n < 1024) return `${String(n)} B`
        const units = ['KiB', 'MiB', 'GiB', 'TiB']
        let v = n / 1024
        let i = 0
        while (v >= 1024 && i < units.length - 1) {
            v /= 1024
            i++
        }
        return `${v.toFixed(1)} ${units[i] ?? 'TiB'}`
    }

    function fmtNum(n: number): string {
        return formatInteger(n)
    }

    function fmtRtt(ms: number | null | undefined): string {
        if (ms === null || ms === undefined) return '—'
        return `${ms.toFixed(1)} ms`
    }

    function fmtAgo(ts: number): string {
        if (!ts) return ''
        const dt = Date.now() - ts
        if (dt < 1000) return 'just now'
        if (dt < 60000) return `${String(Math.floor(dt / 1000))}s ago`
        return `${String(Math.floor(dt / 60000))}m ago`
    }

    // Tick once a second so "Updated Xs ago" stays fresh between polls.
    let nowTick = $state(0)
    let tickInterval: ReturnType<typeof setInterval> | undefined
    onMount(() => {
        tickInterval = setInterval(() => nowTick++, 1000)
    })
    onDestroy(() => {
        if (tickInterval) clearInterval(tickInterval)
    })
    // Reference nowTick so the $effect recomputes "Updated Xs ago" each tick.
    // `void` documents the intentional reactive-dependency read for ESLint.
    $effect(() => {
        void nowTick
    })
</script>

<section class="debug-section">
    <h2>SMB diagnostics</h2>

    <div class="smb-toolbar">
        <label class="smb-control">
            <span class="smb-control-label">Volume</span>
            {#if volumes.length === 0}
                <span class="smb-empty">No SMB volumes mounted</span>
            {:else}
                <div class="smb-select-wrap">
                    <Select
                        items={volumeItems}
                        value={selectedVolumeId ?? ''}
                        onChange={(v: string) => void handleVolumeChange(v)}
                        ariaLabel="SMB volume"
                    />
                </div>
            {/if}
        </label>
        <label class="smb-control smb-control-inline">
            <input type="checkbox" bind:checked={autoRefresh} />
            <span>Auto-refresh</span>
        </label>
        <label class="smb-control smb-control-inline">
            <span>every</span>
            <div class="smb-select-wrap smb-select-narrow">
                <Select
                    items={intervalItems}
                    value={String(intervalMs)}
                    onChange={(v: string) => (intervalMs = Number(v))}
                    disabled={!autoRefresh}
                    ariaLabel="Refresh interval"
                />
            </div>
        </label>
        <button class="index-button" onclick={() => void refreshVolumes()}>Refresh volumes</button>
        <button class="index-button" onclick={() => void poll()}>Snapshot now</button>
        <span class="smb-poll-status">
            {#if loadState === 'loading'}Loading…
            {:else if loadState === 'error'}<span class="smb-poll-err">Error: {errorMessage}</span>
            {:else if loadState === 'ready' && lastPollAt}Updated {fmtAgo(lastPollAt)}
            {:else if loadState === 'idle'}Idle
            {/if}
        </span>
    </div>

    {#if diag}
        {@const p = diag.primary}
        {@const m = p.metrics}
        {@const cm = diag.client.metrics}

        <div class="smb-panel">
            <div class="smb-summary">
                <div class="smb-summary-server">
                    <span class="smb-summary-arrow">→</span>
                    <span class="smb-summary-host">{p.server || '(no host)'}</span>
                    {#if p.disconnected}
                        <span class="smb-badge smb-badge-warn">disconnected</span>
                    {:else}
                        <span class="smb-badge smb-badge-ok">connected</span>
                    {/if}
                </div>
                <div class="smb-summary-line">
                    {#if p.negotiated}
                        <span class="smb-summary-dialect">{p.negotiated.dialect.replace(/^Smb/, 'SMB ').replace('_', '.').replace('_', '.')}</span>
                        <span class="smb-summary-sep">·</span>
                    {/if}
                    <span>RTT {fmtRtt(p.rtt_estimate_ms)}</span>
                    <span class="smb-summary-sep">·</span>
                    <span
                        class="smb-summary-mode"
                        use:tooltip={{
                            text: p.signing.active
                                ? `Outgoing requests are signed with ${p.signing.algorithm ?? 'unknown'}. Server can detect tampering. Signing is required when the server sets SMB2_NEGOTIATE_SIGNING_REQUIRED — otherwise it activates when the session isn't guest/null.`
                                : 'Signing is off. Either guest session, or the server doesn’t require it.',
                        }}
                    >sig {p.signing.active ? 'on' : 'off'}</span>
                    <span class="smb-summary-sep">·</span>
                    <span
                        class="smb-summary-mode"
                        use:tooltip={{
                            text: p.encryption.active
                                ? `End-to-end SMB encryption with ${p.encryption.cipher ?? 'unknown'}. Each request/response is wrapped in a TRANSFORM_HEADER with an AEAD-authenticated payload. Signing is skipped when encrypting (AEAD handles authentication).`
                                : 'Encryption is off. Activated when the session flag or share flag carries SMB2_SHAREFLAG_ENCRYPT_DATA.',
                        }}
                    >enc {p.encryption.active ? 'on' : 'off'}</span>
                    {#if p.compression.requested || p.compression.negotiated}
                        <span class="smb-summary-sep">·</span>
                        <span class="smb-summary-mode">
                            comp {p.compression.negotiated ? 'on' : 'off'}
                        </span>
                    {/if}
                </div>
            </div>

            <div class="smb-grid">
                <!-- Flow gauges -->
                <div class="smb-card">
                    <div class="smb-card-title">Flow</div>
                    <div class="smb-card-grid">
                        <span class="smb-label"
                            >Credits available <span
                                class="info-icon"
                                use:tooltip={{
                                    text: 'Credits the server has granted but we haven’t spent yet. Every request consumes credit_charge credits; the server replenishes per response. When this hits zero, new requests block until responses come back.',
                                }}>i</span
                            ></span
                        >
                        <span class="smb-value">{fmtNum(p.credits.available)}</span>

                        <span class="smb-label"
                            >In flight <span
                                class="info-icon"
                                use:tooltip={{
                                    text: 'Number of MessageIds currently waiting for a response (waiters.len() in smb2). For a parallel download of 7 files via compound CREATE+READ+CLOSE, this peaks at 7×3 = 21.',
                                }}>i</span
                            ></span
                        >
                        <span class="smb-value">{fmtNum(p.credits.in_flight)}</span>

                        <span class="smb-label"
                            >Next message id <span
                                class="info-icon"
                                use:tooltip={{
                                    text: 'The MessageId the next request will be assigned. Monotonic per connection — resets only on reconnect (a new Connection means a fresh Inner with msg_id 0).',
                                }}>i</span
                            ></span
                        >
                        <span class="smb-value">{fmtNum(p.credits.next_message_id)}</span>
                    </div>
                </div>

                <!-- Wire traffic -->
                <div class="smb-card">
                    <div class="smb-card-title">Wire traffic</div>
                    <div class="smb-card-grid">
                        <span class="smb-label"
                            >Sent <span
                                class="info-icon"
                                use:tooltip={{
                                    text: 'Bytes handed to the TCP socket — after any sign/encrypt/compress. The byte count a packet capture would see leaving this client.',
                                }}>i</span
                            ></span
                        >
                        <span class="smb-value" use:tooltip={{ text: `${fmtNum(m.wire_bytes_sent)} B` }}
                            >{fmtBytes(m.wire_bytes_sent)}</span
                        >

                        <span class="smb-label"
                            >Received <span
                                class="info-icon"
                                use:tooltip={{
                                    text: 'Bytes read from the TCP socket — before any decrypt/decompress. The byte count a packet capture would see arriving at this client.',
                                }}>i</span
                            ></span
                        >
                        <span class="smb-value" use:tooltip={{ text: `${fmtNum(m.wire_bytes_received)} B` }}
                            >{fmtBytes(m.wire_bytes_received)}</span
                        >
                    </div>
                </div>

                <!-- Requests -->
                <div class="smb-card">
                    <div class="smb-card-title">Requests</div>
                    <div class="smb-card-grid">
                        <span class="smb-label"
                            >Sent <span
                                class="info-icon"
                                use:tooltip={{
                                    text: 'Every MessageId allocated for a request: negotiate, session-setup, every execute / execute_with_credits / dispatch, plus every sub-op of every execute_compound. (CANCEL reuses an existing id and isn’t counted here.)',
                                }}>i</span
                            ></span
                        >
                        <span class="smb-value">{fmtNum(m.requests_sent)}</span>

                        <span class="smb-label"
                            >Compound chains <span
                                class="info-icon"
                                use:tooltip={{
                                    text: 'Number of execute_compound() calls — the chains themselves, not the sub-ops inside (those tick "Sent"). A 3-op compound is 1 chain + 3 sent.',
                                }}>i</span
                            ></span
                        >
                        <span class="smb-value">{fmtNum(m.compound_requests_sent)}</span>

                        <span class="smb-label"
                            >Returned err <span
                                class="info-icon"
                                use:tooltip={{
                                    text: 'execute/execute_compound returned an outer Err to a caller that polled to completion. Per-call, not per-sub-op. Caller-drop is captured separately as "Late (drop)" below.',
                                }}>i</span
                            ></span
                        >
                        <span class="smb-value">{fmtNum(m.requests_returned_err)}</span>

                        <span class="smb-label"
                            >Explicit cancels <span
                                class="info-icon"
                                use:tooltip={{
                                    text: 'send_cancel() invocations. Cancellation-by-drop (the spawn/abort pattern) is invisible to the wire and counted under "Late (drop)" below instead.',
                                }}>i</span
                            ></span
                        >
                        <span class="smb-value">{fmtNum(m.explicit_cancels_sent)}</span>
                    </div>
                </div>

                <!-- Responses -->
                <div class="smb-card">
                    <div class="smb-card-title">Responses</div>
                    <div class="smb-card-grid">
                        <span class="smb-label"
                            >Routed ok <span
                                class="info-icon"
                                use:tooltip={{
                                    text: 'Sub-frame found the waiter in the map and delivered Ok(frame). The happy path.',
                                }}>i</span
                            ></span
                        >
                        <span class="smb-value">{fmtNum(m.responses_routed_ok)}</span>

                        <span class="smb-label"
                            >Wire err <span
                                class="info-icon"
                                use:tooltip={{
                                    text: 'Sub-frame routed Err(_) to the waiter. Today the only sources are signature_failures + session_expired_events (shown under "Errors" below). The connection survives — only the failed op sees Err.',
                                }}>i</span
                            ></span
                        >
                        <span class="smb-value">{fmtNum(m.responses_routed_err)}</span>

                        <span class="smb-label"
                            >Late (caller dropped) <span
                                class="info-icon"
                                use:tooltip={{
                                    text: 'Response arrived but the caller’s future had been dropped (typical for tokio::spawn + abort()). Credits are still applied so throughput isn’t starved. Should match the cancel-by-drop pattern’s frequency.',
                                }}>i</span
                            ></span
                        >
                        <span class="smb-value">{fmtNum(m.responses_late_after_drop)}</span>

                        <span class="smb-label"
                            >Stray <span
                                class="info-icon"
                                use:tooltip={{
                                    text: 'Server sent a frame for a MessageId we never allocated. Should be near-zero in normal operation. Non-zero means a buggy server or a send-error cleanup race.',
                                }}>i</span
                            ></span
                        >
                        <span class="smb-value">{fmtNum(m.responses_stray)}</span>
                    </div>
                </div>

                <!-- Protocol events -->
                <div class="smb-card">
                    <div class="smb-card-title">Protocol events</div>
                    <div class="smb-card-grid">
                        <span class="smb-label"
                            >STATUS_PENDING <span
                                class="info-icon"
                                use:tooltip={{
                                    text: 'Interim STATUS_PENDING sub-frames the receiver kept the waiter alive on. CHANGE_NOTIFY long-polls and slow IOCTLs typically tick this. Each interim carries fresh credits.',
                                }}>i</span
                            ></span
                        >
                        <span class="smb-value">{fmtNum(m.status_pending_loops)}</span>

                        <span class="smb-label"
                            >Unsolicited <span
                                class="info-icon"
                                use:tooltip={{
                                    text: 'Sub-frames with MessageId::UNSOLICITED (0xFFFF…). Today these are oplock breaks; the spec reserves the magic id for future server-initiated notifications (lease breaks, etc.). We log + skip.',
                                }}>i</span
                            ></span
                        >
                        <span class="smb-value">{fmtNum(m.unsolicited_notifications_received)}</span>
                    </div>
                </div>

                <!-- Errors -->
                <div class="smb-card">
                    <div class="smb-card-title">Errors</div>
                    <div class="smb-card-grid">
                        <span class="smb-label"
                            >Signature <span
                                class="info-icon"
                                use:tooltip={{
                                    text: 'Signature verification failed on an inbound frame. The error is routed to the matching waiter; the connection continues. A non-zero count usually means a key-derivation bug, message corruption, or a server that signed with a different algorithm than we expect.',
                                }}>i</span
                            ></span
                        >
                        <span class="smb-value smb-value-err" class:smb-zero={m.signature_failures === 0}
                            >{fmtNum(m.signature_failures)}</span
                        >

                        <span class="smb-label"
                            >Decrypt <span
                                class="info-icon"
                                use:tooltip={{
                                    text: 'AES-GCM/CCM auth-tag mismatch, missing key, or malformed TRANSFORM_HEADER. Tears down the connection — every pending waiter gets Err(Disconnected). Counted once before the fan-out; survives teardown.',
                                }}>i</span
                            ></span
                        >
                        <span class="smb-value smb-value-err" class:smb-zero={m.decrypt_failures === 0}
                            >{fmtNum(m.decrypt_failures)}</span
                        >

                        <span class="smb-label"
                            >Decompress <span
                                class="info-icon"
                                use:tooltip={{
                                    text: 'LZ4 decompression failed on a COMPRESSION_HEADER frame. Same teardown behaviour as decrypt failures.',
                                }}>i</span
                            ></span
                        >
                        <span class="smb-value smb-value-err" class:smb-zero={m.decompress_failures === 0}
                            >{fmtNum(m.decompress_failures)}</span
                        >

                        <span class="smb-label"
                            >Malformed <span
                                class="info-icon"
                                use:tooltip={{
                                    text: 'Compound split failure or sub-frame header parse failure. Connection tears down. Should be 0 against compliant servers.',
                                }}>i</span
                            ></span
                        >
                        <span class="smb-value smb-value-err" class:smb-zero={m.malformed_frames === 0}
                            >{fmtNum(m.malformed_frames)}</span
                        >

                        <span class="smb-label"
                            >Session expired <span
                                class="info-icon"
                                use:tooltip={{
                                    text: 'STATUS_NETWORK_SESSION_EXPIRED sub-frames. Counted per sub-frame, not per session event — a compound of N expired sub-ops ticks N times. For the event signal "did we reconnect", see Client → Reconnects below.',
                                }}>i</span
                            ></span
                        >
                        <span class="smb-value smb-value-err" class:smb-zero={m.session_expired_events === 0}
                            >{fmtNum(m.session_expired_events)}</span
                        >
                    </div>
                </div>

                <!-- Session -->
                {#if p.session}
                    <div class="smb-card">
                        <div class="smb-card-title">Session</div>
                        <div class="smb-card-grid">
                            <span class="smb-label">Session id</span>
                            <span class="smb-value smb-value-mono">0x{p.session.session_id_hex}</span>
                            <span class="smb-label">Should sign</span>
                            <span class="smb-value">{p.session.should_sign ? 'yes' : 'no'}</span>
                            <span class="smb-label">Should encrypt</span>
                            <span class="smb-value">{p.session.should_encrypt ? 'yes' : 'no'}</span>
                            <span class="smb-label">Algorithm</span>
                            <span class="smb-value">{p.session.signing_algorithm}</span>
                        </div>
                    </div>
                {/if}

                <!-- Negotiated -->
                {#if p.negotiated}
                    {@const n = p.negotiated}
                    <div class="smb-card">
                        <div class="smb-card-title">Negotiated</div>
                        <div class="smb-card-grid">
                            <span class="smb-label">Max read</span>
                            <span class="smb-value">{fmtBytes(n.max_read_size)}</span>
                            <span class="smb-label">Max write</span>
                            <span class="smb-value">{fmtBytes(n.max_write_size)}</span>
                            <span class="smb-label">Max transact</span>
                            <span class="smb-value">{fmtBytes(n.max_transact_size)}</span>
                            <span class="smb-label"
                                >GMAC <span
                                    class="info-icon"
                                    use:tooltip={{
                                        text: 'Whether AES-GMAC signing was negotiated (SMB 3.1.1 with SMB2_SIGNING_CAPABILITIES). Falls back to AES-CMAC otherwise. GMAC is AES-128-GCM with empty plaintext — the auth tag IS the signature.',
                                    }}>i</span
                                ></span
                            >
                            <span class="smb-value">{n.gmac_negotiated ? 'yes' : 'no'}</span>
                            <span class="smb-label">Server GUID</span>
                            <span class="smb-value smb-value-mono smb-value-truncate" use:tooltip={{ text: n.server_guid_hex }}
                                >{n.server_guid_hex}</span
                            >
                        </div>
                    </div>
                {/if}
            </div>

            <!-- Client + DFS -->
            <div class="smb-grid">
                <div class="smb-card">
                    <div class="smb-card-title">Client</div>
                    <div class="smb-card-grid">
                        <span class="smb-label"
                            >Reconnects <span
                                class="info-icon"
                                use:tooltip={{
                                    text: 'SmbClient::reconnect() invocations across the client’s lifetime. Survives reconnect (per-connection counters reset, this one doesn’t).',
                                }}>i</span
                            ></span
                        >
                        <span class="smb-value">{fmtNum(cm.reconnects)}</span>

                        <span class="smb-label"
                            >DFS referrals resolved <span
                                class="info-icon"
                                use:tooltip={{
                                    text: 'Cache misses that resulted in a DFS_GET_REFERRALS IOCTL to the server.',
                                }}>i</span
                            ></span
                        >
                        <span class="smb-value">{fmtNum(cm.dfs_referrals_resolved)}</span>

                        <span class="smb-label"
                            >DFS cache hits <span
                                class="info-icon"
                                use:tooltip={{
                                    text: 'Cache hits served from the in-process DFS referral cache (no server round-trip).',
                                }}>i</span
                            ></span
                        >
                        <span class="smb-value">{fmtNum(cm.dfs_cache_hits)}</span>

                        <span class="smb-label">Auto-reconnect</span>
                        <span class="smb-value">{diag.client.auto_reconnect ? 'on' : 'off'}</span>

                        <span class="smb-label">DFS</span>
                        <span class="smb-value">{diag.client.dfs_enabled ? 'enabled' : 'disabled'}</span>
                    </div>
                </div>

                {#if diag.dfs_cache.length > 0}
                    <div class="smb-card smb-card-wide">
                        <div class="smb-card-title">DFS cache ({diag.dfs_cache.length})</div>
                        <ul class="smb-list">
                            {#each diag.dfs_cache as entry (entry.path_prefix)}
                                <li>
                                    <span class="smb-mono">{entry.path_prefix}</span>
                                    <span class="smb-list-meta"
                                        >{entry.target_count} target{entry.target_count === 1 ? '' : 's'}
                                        ·
                                        {#if entry.expires_in_ms === null}
                                            <span class="smb-expired">expired</span>
                                        {:else}
                                            expires in {(entry.expires_in_ms / 1000).toFixed(0)} s
                                        {/if}
                                    </span>
                                </li>
                            {/each}
                        </ul>
                    </div>
                {/if}
            </div>

            <!-- DFS extra connections -->
            {#if diag.extra_connections.length > 0}
                <div class="smb-card-title smb-extras-title">DFS extra connections ({diag.extra_connections.length})</div>
                <div class="smb-grid">
                    {#each diag.extra_connections as extra (extra.server)}
                        <div class="smb-card">
                            <div class="smb-card-title">↳ {extra.server}</div>
                            <div class="smb-card-grid">
                                <span class="smb-label">Credits</span>
                                <span class="smb-value"
                                    >{fmtNum(extra.credits.available)} avail · {fmtNum(extra.credits.in_flight)} in flight</span
                                >
                                <span class="smb-label">Requests sent</span>
                                <span class="smb-value">{fmtNum(extra.metrics.requests_sent)}</span>
                                <span class="smb-label">Routed ok</span>
                                <span class="smb-value">{fmtNum(extra.metrics.responses_routed_ok)}</span>
                                <span class="smb-label">Wire bytes</span>
                                <span class="smb-value">
                                    ↑{fmtBytes(extra.metrics.wire_bytes_sent)} · ↓{fmtBytes(extra.metrics.wire_bytes_received)}
                                </span>
                            </div>
                        </div>
                    {/each}
                </div>
            {/if}
        </div>
    {:else if loadState === 'error'}
        <div class="smb-panel smb-panel-empty">
            <p>{errorMessage}</p>
            <p class="smb-empty-hint">Pick a different volume above, or open an SMB share first.</p>
        </div>
    {:else if loadState === 'idle' && volumes.length === 0}
        <div class="smb-panel smb-panel-empty">
            <p>No SMB volumes are mounted right now.</p>
            <p class="smb-empty-hint">Open one in the main window — this dashboard will pick it up on the next refresh.</p>
        </div>
    {:else}
        <div class="smb-panel smb-panel-empty">
            <p>Loading…</p>
        </div>
    {/if}
</section>

<style>
    /* stylelint-disable declaration-property-value-disallowed-list -- Dev utility panel */

    .smb-toolbar {
        display: flex;
        flex-wrap: wrap;
        align-items: center;
        gap: 12px;
        margin-bottom: 12px;
        font-size: var(--font-size-sm);
    }

    .smb-control {
        display: inline-flex;
        align-items: center;
        gap: 6px;
        color: var(--color-text-secondary);
    }

    .smb-control-inline {
        gap: 4px;
    }

    .smb-control-label {
        color: var(--color-text-tertiary);
    }

    .smb-select-wrap {
        display: inline-flex;
        min-width: 160px;
    }

    .smb-select-narrow {
        min-width: 90px;
    }

    .smb-empty {
        color: var(--color-text-tertiary);
        font-style: italic;
    }

    .smb-poll-status {
        margin-left: auto;
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
    }

    .smb-poll-err {
        color: var(--color-text-secondary);
    }

    .smb-panel {
        display: flex;
        flex-direction: column;
        gap: 12px;
    }

    .smb-panel-empty {
        padding: 16px;
        background: var(--color-bg-secondary);
        border-radius: var(--radius-md);
        color: var(--color-text-tertiary);
        font-size: var(--font-size-sm);
    }

    .smb-empty-hint {
        margin: 6px 0 0;
        color: var(--color-text-tertiary);
        font-size: var(--font-size-xs);
    }

    .smb-summary {
        background: var(--color-bg-secondary);
        border-radius: var(--radius-md);
        padding: 10px 12px;
        display: flex;
        flex-direction: column;
        gap: 4px;
    }

    .smb-summary-server {
        display: flex;
        align-items: center;
        gap: 8px;
        font-size: var(--font-size-md);
        color: var(--color-text-primary);
    }

    .smb-summary-arrow {
        color: var(--color-text-tertiary);
    }

    .smb-summary-host {
        font-weight: 600;
    }

    .smb-summary-line {
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
        display: flex;
        gap: 6px;
        align-items: baseline;
        flex-wrap: wrap;
    }

    .smb-summary-sep {
        color: var(--color-text-tertiary);
    }

    .smb-summary-dialect {
        font-weight: 600;
        color: var(--color-text-primary);
    }

    .smb-summary-mode {
        font-family: var(--font-mono);
        cursor: help;
    }

    .smb-badge {
        font-size: var(--font-size-xs);
        padding: 1px 6px;
        border-radius: var(--radius-sm);
        font-weight: 600;
        text-transform: uppercase;
        letter-spacing: 0.5px;
    }

    .smb-badge-ok {
        background: var(--color-bg-tertiary);
        color: var(--color-text-secondary);
    }

    .smb-badge-warn {
        background: var(--color-bg-tertiary);
        color: var(--color-text-primary);
        outline: 1px solid var(--color-border-strong);
    }

    .smb-grid {
        display: grid;
        grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
        gap: 8px;
    }

    .smb-card {
        background: var(--color-bg-secondary);
        border-radius: var(--radius-md);
        padding: 10px 12px;
        display: flex;
        flex-direction: column;
        gap: 6px;
    }

    .smb-card-wide {
        grid-column: 1 / -1;
    }

    .smb-card-title {
        font-size: var(--font-size-xs);
        font-weight: 600;
        text-transform: uppercase;
        letter-spacing: 0.5px;
        color: var(--color-text-tertiary);
    }

    .smb-extras-title {
        margin-top: 6px;
    }

    .smb-card-grid {
        display: grid;
        grid-template-columns: 1fr auto;
        gap: 3px 8px;
        font-size: var(--font-size-sm);
        align-items: baseline;
    }

    .smb-label {
        color: var(--color-text-tertiary);
        display: inline-flex;
        align-items: center;
        gap: 4px;
        min-width: 0;
    }

    .smb-value {
        color: var(--color-text-primary);
        font-family: var(--font-mono);
        text-align: right;
    }

    .smb-value-mono {
        font-family: var(--font-mono);
    }

    .smb-value-truncate {
        max-width: 18ch;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .smb-value-err {
        color: var(--color-text-primary);
    }

    .smb-zero {
        color: var(--color-text-tertiary);
    }

    .smb-list {
        list-style: none;
        margin: 0;
        padding: 0;
        font-size: var(--font-size-sm);
        display: flex;
        flex-direction: column;
        gap: 4px;
    }

    .smb-list li {
        display: flex;
        align-items: baseline;
        justify-content: space-between;
        gap: 8px;
    }

    .smb-mono {
        font-family: var(--font-mono);
        color: var(--color-text-secondary);
    }

    .smb-list-meta {
        color: var(--color-text-tertiary);
        font-size: var(--font-size-xs);
    }

    .smb-expired {
        color: var(--color-text-primary);
        font-style: italic;
    }
</style>
