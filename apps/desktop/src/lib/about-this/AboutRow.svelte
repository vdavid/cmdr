<!--
  One planned row of the card, in its section's `<dl>`: a label and the answer as it stands (a static bar while
  pending, the words for a known value, or the absence line). Identity draws its own labeled lines.

  ❗ Every string that came from the item (a host, a sender, an app name, a signer, a branch, a path) renders as a
  text node through `t()` params or `{…}`, ❌ never `{@html}`, and never as a link built from it. Self-reported
  facts are worded as recorded ("macOS recorded…", "Carries a signature from…").
-->
<script lang="ts">
    import type { Snippet } from 'svelte'
    import type { AboutTarget, WhereFrom } from '#lib/ipc/bindings.js'
    import { t } from '#lib/intl/messages.svelte.js'
    import Trans from '#lib/intl/Trans.svelte'
    import { formatInteger } from '#lib/intl/number-format.js'
    import { formatConjunctionList } from '#lib/intl/list-format.js'
    import { tString } from '#lib/intl/messages.svelte.js'
    import Size from '#lib/ui/Size.svelte'
    import DateLabel from '#lib/ui/DateLabel.svelte'
    import CopyButton from '#lib/ui/CopyButton.svelte'
    import Icon from '#lib/ui/Icon.svelte'
    import AboutAbsence from './AboutAbsence.svelte'
    import AboutPath from './AboutPath.svelte'
    import AboutDigest from './AboutDigest.svelte'
    import AboutAppData from './AboutAppData.svelte'
    import type { AboutGoTarget } from './about-card'
    import { copiedPath, displayPath } from '#lib/utils/display-path.js'
    import { lastName } from '#lib/path/grammar.js'
    import { tooltip } from '#lib/tooltip/tooltip.js'
    import { showsOnDisk, type RowView } from './about-card-view'
    import { KIND_LABEL, ROW_LABEL, importanceFloorKey, importanceReasonKey, shortCommitId } from './about-words'

    interface Props {
        row: RowView
        target: AboutTarget
        /**
         * The facts have settled. Until then a row keeps the height its answer usually takes, so a late answer
         * lands into space already there; at the settle the reservations go, with the section collapse.
         */
        settled: boolean
        /** The card's item, so What's inside can say where under it a deep file sits. */
        subjectPath: string
        /** Takes the pane to a place a row names, closing the card. */
        onGoTo: (target: AboutGoTarget) => void
    }

    const { row, target, settled, subjectPath, onGoTo }: Props = $props()

    /** Lines a row's usual answer takes, reserved while the card fills. */
    const RESERVED_LINES: Partial<Record<RowView['kind'], number>> = {
        download: 2,
        gitSnapshot: 2,
        cmdrOperation: 2,
        signature: 2,
        owner: 2,
        git: 2,
        importance: 2,
        // A folder's digest and an app's data are lists; they land inside the grace on a local disk, and the rest of
        // their height goes at the settle with every other reservation.
        folderDigest: 4,
        appData: 3,
    }
    const reserved = $derived(settled ? 1 : (RESERVED_LINES[row.kind] ?? 1))
    const state = $derived(row.fact?.fact ?? ({ state: 'pending' } as const))
    const pending = $derived(state.state === 'pending')

    /** The first web source, which the card names, and a second one on another site, which it names after. */
    function webSources(sources: WhereFrom[]): { first: WhereFrom & { type: 'web' }; page: (WhereFrom & { type: 'web' }) | null } | null {
        const web = sources.filter((s): s is WhereFrom & { type: 'web' } => s.type === 'web')
        if (web.length === 0) return null
        const [first, ...rest] = web
        const firstHost = first.site.type === 'domain' ? first.site.host : first.site.address
        const page = rest.find((s) => (s.site.type === 'domain' ? s.site.host : s.site.address) !== firstHost) ?? null
        return { first, page }
    }

    function siteName(source: WhereFrom & { type: 'web' }): string {
        return source.site.type === 'domain' ? source.site.host : source.site.address
    }
</script>

{#snippet bar()}
    <span class="skeleton" aria-hidden="true"></span><span class="sr-only">{t('aboutThis.dialog.lookingUp')}</span>
{/snippet}

{#snippet absence()}
    {#if state.state === 'unknown'}
        <AboutAbsence kind={row.kind} unknown={state.value} />
    {:else if state.state === 'notApplicable'}
        <AboutAbsence kind={row.kind} notApplicable={state.value} />
    {/if}
{/snippet}

{#if row.kind === 'identity'}
    {@const fact = row.fact?.kind === 'identity' ? row.fact.fact : null}
    {@const identity = fact?.state === 'known' ? fact.value : null}
    {@const isFolderLike = target === 'folder' || target === 'appBundle' || target === 'package'}
    {#if !isFolderLike}
        <div class="row" aria-busy={pending} data-row-kind="size">
            <dt>{t('aboutThis.label.size')}</dt>
            <dd>
                {#if identity}
                    {#if identity.logicalSize !== null}
                        <Size bytes={identity.logicalSize} />{#if showsOnDisk(identity)}
                            {#snippet onDisk(_content: Snippet)}<Size bytes={identity.physicalSize} />{/snippet}
                            <span class="quiet"> · <Trans key="aboutThis.identity.onDisk" snippets={{ size: onDisk }} /></span
                            >{/if}
                    {:else}
                        <AboutAbsence kind="identity" unknown={{ why: 'volumeCantTell', prominence: 'routine' }} />
                    {/if}
                {:else if pending}{@render bar()}{:else}{@render absence()}{/if}
            </dd>
        </div>
    {/if}
    <div class="row" aria-busy={pending}>
        <dt>{t('aboutThis.label.modified')}</dt>
        <dd>
            {#if identity}
                {#if identity.modified !== null}<DateLabel modifiedAt={identity.modified} />{:else}<AboutAbsence
                        kind="identity"
                        unknown={{ why: 'volumeCantTell', prominence: 'routine' }}
                    />{/if}
            {:else if pending}{@render bar()}{:else}{@render absence()}{/if}
        </dd>
    </div>
    {#if !identity || identity.created !== null}
        <div class="row" aria-busy={pending}>
            <dt>{t('aboutThis.label.created')}</dt>
            <dd>
                {#if identity?.created != null}<DateLabel modifiedAt={identity.created} />{:else if pending}{@render bar()}{:else}{@render absence()}{/if}
            </dd>
        </div>
    {/if}
    {#if identity && identity.hardLinks !== null && identity.hardLinks > 1}
        {@const others = identity.hardLinks - 1}
        <div class="row">
            <dt>{t('aboutThis.label.otherNames')}</dt>
            <dd>{t('aboutThis.identity.otherNames', { count: others, countText: formatInteger(others) })}</dd>
        </div>
    {/if}
    {#if identity?.symlinkTarget}
        <div class="row">
            <dt>{t('aboutThis.label.pointsTo')}</dt>
            <dd class="with-copy">
                <span class="line-text"><AboutPath path={identity.symlinkTarget} /></span><CopyButton
                    text={identity.symlinkTarget}
                    label={tString('aboutThis.identity.copyLinkTarget')}
                />
            </dd>
        </div>
    {/if}
{:else if row.fact?.kind === 'folderDigest' && row.fact.fact.state === 'known'}
    <!-- What's inside draws its own rows: the count, the biggest children, the largest files, the untouched share. -->
    <AboutDigest digest={row.fact.fact.value} folderPath={subjectPath} {onGoTo} />
{:else}
    <div
        class="row"
        aria-busy={pending}
        style:min-height={reserved > 1 ? `calc(${String(reserved)} * var(--font-line-height-normal) * 1em)` : null}
        data-row-kind={row.kind}
    >
        <dt>{t(ROW_LABEL[row.kind])}</dt>
        <dd>
            {#if pending}
                {@render bar()}
            {:else if state.state !== 'known'}
                {@render absence()}
            {:else if row.fact?.kind === 'download' && row.fact.fact.state === 'known'}
                {@const download = row.fact.fact.value}
                {@const by = download.downloadedBy}
                {@const at = download.downloadedAt}
                {#snippet downloadDate(_content: Snippet)}<DateLabel modifiedAt={at} />{/snippet}
                {@const sources = download.sources.state === 'known' ? download.sources.value : []}
                {@const web = webSources(sources)}
                {@const other = sources.find((s) => s.type !== 'web')}
                {#if download.archive}
                    <!-- An entry carries no record of its own: the row names its archive, then the archive's record. -->
                    <span class="line">{t('aboutThis.download.insideArchive', { archive: download.archive })}</span>
                    {#if download.sources.state !== 'known' && by === null && at === null}
                        <span class="line"><AboutAbsence kind="download" unknown={download.sources.state === 'unknown'
                                ? download.sources.value
                                : { why: 'volumeCantTell', prominence: 'routine' }} /></span>
                    {:else}
                        <span class="line quiet">{t('aboutThis.download.archiveRecord')}</span>
                    {/if}
                {/if}
                {#if download.archive && download.sources.state !== 'known' && by === null && at === null}
                    <!-- Said above. -->
                {:else if web}
                    <span class="line">
                        {#if web.first.site.type === 'domain'}
                            <Trans
                                key="aboutThis.download.fromSite"
                                params={{ host: web.first.site.host, app: by ?? '', hasApp: by ? 'yes' : 'no', hasDate: at !== null ? 'yes' : 'no' }}
                                snippets={{ date: downloadDate }}
                            />
                        {:else}
                            {t('aboutThis.download.fromAddress', { address: web.first.site.address })}
                        {/if}
                        {#if web.first.url}<CopyButton text={web.first.url} label={tString('aboutThis.download.copyLink')} />{/if}
                    </span>
                    {#if web.page}
                        <span class="line quiet">
                            {t('aboutThis.download.linkedFrom', { host: siteName(web.page) })}
                            {#if web.page.url}<CopyButton text={web.page.url} label={tString('aboutThis.download.copyPageLink')} />{/if}
                        </span>
                    {/if}
                {:else if other?.type === 'mailAttachment'}
                    <span class="line">{t('aboutThis.download.mail', { sender: other.sender ?? '', hasSender: other.sender ? 'yes' : 'no' })}</span>
                    {#if other.subject}<span class="line quiet">{t('aboutThis.download.mailSubject', { subject: other.subject })}</span>{/if}
                {:else if other?.type === 'airDrop'}
                    <span class="line">{t('aboutThis.download.airDrop', { sender: other.sender })}</span>
                {:else if other?.type === 'other'}
                    <span class="line">
                        {#if other.kind === 'inlineData'}{t('aboutThis.download.inlineData')}{:else if other.kind === 'localFile'}{t(
                                'aboutThis.download.localFile',
                            )}{:else}{t('aboutThis.download.unrecognized', { text: other.text })}{/if}
                    </span>
                {:else}
                    <span class="line">
                        <Trans
                            key="aboutThis.download.noSource"
                            params={{ app: by ?? '', hasApp: by ? 'yes' : 'no', hasDate: at !== null ? 'yes' : 'no' }}
                            snippets={{ date: downloadDate }}
                        />
                    </span>
                    {#if download.sources.state === 'unknown' && download.sources.value.prominence === 'notable'}
                        <span class="line"><AboutAbsence kind="download" unknown={download.sources.value} /></span>
                    {/if}
                {/if}
            {:else if row.fact?.kind === 'gitSnapshot' && row.fact.fact.state === 'known'}
                {@const snapshot = row.fact.fact.value}
                {@const name = snapshot.kind === 'commit' ? shortCommitId(snapshot.name) : snapshot.name}
                {#snippet repo(_content: Snippet)}<span use:tooltip={displayPath(snapshot.repoRoot)}
                        >{lastName(snapshot.repoRoot)}</span
                    >{/snippet}
                <span class="line with-copy">
                    <span class="line-text"
                        ><Trans key="aboutThis.gitSnapshot.from" params={{ kind: snapshot.kind, name }} snippets={{ repo }} /></span
                    >{#if snapshot.kind === 'commit'}<CopyButton
                            text={snapshot.name}
                            label={tString('aboutThis.gitSnapshot.copyCommit')}
                        />{/if}
                </span>
                {#if snapshot.lastChanged !== null}
                    {@const changed = snapshot.lastChanged}
                    {#snippet changedDate(_content: Snippet)}<DateLabel modifiedAt={changed} />{/snippet}
                    <span class="line quiet"><Trans key="aboutThis.gitSnapshot.lastChanged" snippets={{ date: changedDate }} /></span>
                {/if}
            {:else if row.fact?.kind === 'cmdrOperation' && row.fact.fact.state === 'known'}
                {@const op = row.fact.fact.value}
                {#snippet opDate(_content: Snippet)}<DateLabel modifiedAt={op.at} />{/snippet}
                <span class="line">
                    <Trans
                        key="aboutThis.cmdrOperation.done"
                        params={{ who: op.initiator, kind: op.kind }}
                        snippets={{ date: opDate }}
                    />
                </span>
                {#if op.sourcePath}
                    {@const sourcePath = op.sourcePath}
                    {#snippet source(_content: Snippet)}<AboutPath path={sourcePath} />{/snippet}
                    <span class="line quiet with-copy">
                        <span class="line-text"
                            ><Trans
                                key="aboutThis.cmdrOperation.from"
                                params={{ volume: op.sourceVolume ?? '', hasVolume: op.sourceVolume ? 'yes' : 'no' }}
                                snippets={{ source }}
                            /></span
                        ><CopyButton text={copiedPath(sourcePath)} label={tString('aboutThis.cmdrOperation.copySource')} />
                    </span>
                {/if}
            {:else if row.fact?.kind === 'realType' && row.fact.fact.state === 'known'}
                {@const type = row.fact.fact.value}
                {#if type.agreement === 'mismatch'}
                    <span class="notable">
                        <span class="mark" aria-hidden="true"><Icon name="triangle-alert" size={12} /></span>
                        {t('aboutThis.realType.mismatch', {
                            kind: tString(KIND_LABEL[type.sniffed.kind]),
                            extension: type.claimed.extension ?? '',
                        })}
                    </span>
                {:else if type.agreement === 'matches'}
                    {t('aboutThis.realType.matches', { kind: tString(KIND_LABEL[type.sniffed.kind]) })}
                {:else if type.sniffed.kind !== 'unrecognized'}
                    {t(KIND_LABEL[type.sniffed.kind])}
                {:else if type.claimed.kind}
                    {t('aboutThis.realType.byName', { kind: tString(KIND_LABEL[type.claimed.kind]) })}
                {:else}
                    {t('aboutThis.realType.unrecognized')}
                {/if}
            {:else if row.fact?.kind === 'bundle' && row.fact.fact.state === 'known'}
                {@const bundle = row.fact.fact.value}
                {@const versionParams = { version: bundle.version ?? '', hasVersion: bundle.version ? 'yes' : 'no' }}
                <span class="line">
                    {#if bundle.displayName}{t('aboutThis.bundle.named', { name: bundle.displayName, ...versionParams })}{:else}{t(
                            'aboutThis.bundle.unnamed',
                            versionParams,
                        )}{/if}
                </span>
                {#if bundle.bundleId}<span class="line quiet">{bundle.bundleId}</span>{/if}
            {:else if row.fact?.kind === 'signature' && row.fact.fact.state === 'known'}
                {@const signature = row.fact.fact.value}
                {@const matches = signature.checked.type === 'executableMatches' ? 'yes' : 'no'}
                <span class="line">
                    {#if signature.signer.type === 'unsigned'}{t('aboutThis.signature.unsigned')}
                    {:else if signature.signer.type === 'adHoc'}{t('aboutThis.signature.adHoc')}
                    {:else if signature.signer.type === 'developerId'}{t('aboutThis.signature.developerId', {
                            org: signature.signer.org,
                            team: signature.signer.team,
                            matches,
                        })}
                    {:else if signature.signer.type === 'appStore'}{t('aboutThis.signature.appStore', { matches })}
                    {:else if signature.signer.type === 'apple'}{t('aboutThis.signature.apple', { matches })}
                    {:else}{t('aboutThis.signature.unrecognized')}{/if}
                </span>
                {#if signature.checked.type === 'executableDiffers'}
                    <span class="line notable">
                        <span class="mark" aria-hidden="true"><Icon name="triangle-alert" size={12} /></span>
                        {t('aboutThis.signature.executableDiffers')}
                    </span>
                {:else if signature.checked.type === 'notChecked' && signature.checked.why === 'timedOut'}
                    <span class="line quiet">{t('aboutThis.signature.checkTimedOut')}</span>
                {:else if signature.checked.type === 'notChecked' && signature.checked.why === 'checkUnavailable'}
                    <span class="line quiet">{t('aboutThis.signature.checkUnavailable')}</span>
                {/if}
                {#if signature.ticket === 'present'}
                    <span class="line quiet">{t('aboutThis.signature.ticketPresent')}</span>
                {:else if signature.ticket === 'absent'}
                    <span class="line quiet">{t('aboutThis.signature.ticketAbsent')}</span>
                {/if}
            {:else if row.fact?.kind === 'lastOpened' && row.fact.fact.state === 'known'}
                {@const usage = row.fact.fact.value}
                {#snippet openedDate(_content: Snippet)}<DateLabel modifiedAt={usage.lastOpened} />{/snippet}
                <Trans
                    key="aboutThis.lastOpened.known"
                    params={{
                        count: usage.useCount ?? 0,
                        countText: formatInteger(usage.useCount ?? 0),
                        hasCount: usage.useCount !== null ? 'yes' : 'no',
                    }}
                    snippets={{ date: openedDate }}
                />
            {:else if row.fact?.kind === 'defaultApp' && row.fact.fact.state === 'known'}
                {row.fact.fact.value.name}
            {:else if row.fact?.kind === 'importance' && row.fact.fact.state === 'known'}
                {@const importance = row.fact.fact.value}
                {@const reasons = [...new Set(importance.reasons.map((token) => tString(importanceReasonKey(token))))]}
                <span class="line">{t('aboutThis.importance.band', { band: importance.band })}</span>
                {#if importance.band === 'floored'}
                    <!-- What Cmdr's ranking leaves out, said as Cmdr's choice: "low" about a folder of app data must
                         never read as "this doesn't matter". -->
                    {#each importance.reasons as token (token)}
                        <span class="line quiet">{t(importanceFloorKey(token))}</span>
                    {/each}
                {:else if reasons.length > 0}
                    <span class="line quiet">{t('aboutThis.importance.because', { reasons: formatConjunctionList(reasons) })}</span>
                {/if}
                {#if importance.visits !== null && importance.visits > 0}
                    <span class="line quiet"
                        >{t('aboutThis.importance.visits', {
                            count: importance.visits,
                            countText: formatInteger(importance.visits),
                        })}</span
                    >
                {/if}
                {#if importance.stale}<span class="line quiet">{t('aboutThis.importance.stale')}</span>{/if}
            {:else if row.fact?.kind === 'owner' && row.fact.fact.state === 'known'}
                {@const owner = row.fact.fact.value}
                {#if owner.type === 'byBundleId'}
                    {#if owner.app.type === 'installed'}
                        <span class="line">{t('aboutThis.owner.byBundleId', { app: owner.app.name })}</span>
                        <span class="line quiet">{t('aboutThis.owner.byBundleIdHow', { bundleId: owner.bundleId })}</span>
                    {:else}
                        <span class="line">{t('aboutThis.owner.notInstalled', { bundleId: owner.bundleId })}</span>
                    {/if}
                {:else if owner.type === 'byName'}
                    <span class="line">{t('aboutThis.owner.byName', { app: owner.app })}</span>
                    <span class="line quiet">{t('aboutThis.owner.byNameHow')}</span>
                {:else if owner.type === 'installedBy'}
                    <span class="line"
                        >{owner.partOfOs
                            ? t('aboutThis.owner.partOfOs', { pkgId: owner.pkgId })
                            : t('aboutThis.owner.installedBy', { pkgId: owner.pkgId })}</span
                    >
                {:else if owner.type === 'homebrew'}
                    <span class="line">{t('aboutThis.owner.homebrew', { name: owner.name, kind: owner.kind })}</span>
                {:else}
                    <span class="line">{t('aboutThis.owner.insideApp', { app: owner.app })}</span>
                {/if}
            {:else if row.fact?.kind === 'git' && row.fact.fact.state === 'known'}
                {@const git = row.fact.fact.value}
                <!-- The repo by its folder's name ("In cmdr, on main"), the whole path in its tooltip and behind Copy:
                     a long root cut mid-sentence broke the line around it. -->
                {#snippet repo(_content: Snippet)}<span use:tooltip={displayPath(git.repoRoot)}
                        >{lastName(git.repoRoot)}</span
                    >{/snippet}
                <span class="line with-copy">
                    <span class="line-text"
                        ><Trans
                            key="aboutThis.git.repo"
                            params={{ branch: git.branch ?? '', hasBranch: git.branch ? 'yes' : 'no' }}
                            snippets={{ repo }}
                        /></span
                    ><CopyButton text={copiedPath(git.repoRoot)} label={tString('aboutThis.git.copyRepo')} />
                </span>
                <span class="line">
                    {#if git.status.state === 'known'}
                        {@const status = git.status.value}
                        {#if status.type === 'tracked'}{status.state === 'clean'
                                ? t('aboutThis.git.clean')
                                : t('aboutThis.git.modified')}{:else if status.type === 'untracked'}{t(
                                'aboutThis.git.untracked',
                            )}{:else}{t('aboutThis.git.ignored')}{/if}
                    {:else if git.status.state === 'unknown'}
                        <AboutAbsence kind="git" unknown={git.status.value} />
                    {:else if git.status.state === 'notApplicable'}
                        <AboutAbsence kind="git" notApplicable={git.status.value} />
                    {:else}
                        {@render bar()}
                    {/if}
                </span>
            {:else if row.fact?.kind === 'recursiveSize' && row.fact.fact.state === 'known'}
                {@const size = row.fact.fact.value}
                {#snippet folderBytes(_content: Snippet)}<Size bytes={size.logical} />{/snippet}
                <span class="line">
                    <Trans
                        key="aboutThis.folderSize.known"
                        params={{
                            coverage: size.coverage,
                            files: size.files,
                            filesText: formatInteger(size.files),
                            folders: size.folders,
                            foldersText: formatInteger(size.folders),
                        }}
                        snippets={{ size: folderBytes }}
                    />
                </span>
                {#if size.updating}
                    <span class="line quiet">{t('aboutThis.folderSize.updating')}</span>
                {:else if size.coverage === 'exactButStale'}
                    <span class="line quiet">{t('aboutThis.folderSize.stale')}</span>
                {/if}
                {#if size.hasSymlinks}<span class="line quiet">{t('aboutThis.folderSize.symlinks')}</span>{/if}
            {:else if row.fact?.kind === 'appData' && row.fact.fact.state === 'known'}
                <AboutAppData data={row.fact.fact.value} {onGoTo} />
            {/if}
        </dd>
    </div>
{/if}

<style>
    .row {
        display: grid;
        grid-template-columns: 8.5rem minmax(0, 1fr);
        column-gap: var(--spacing-md);
    }

    dt {
        color: var(--color-text-secondary);
    }

    dd {
        margin: 0;
        min-width: 0;
        overflow-wrap: break-word;
    }

    .line {
        display: block;
    }

    /* A line whose Copy button sits at its end, after the words wrap. */
    .with-copy {
        display: flex;
        gap: var(--spacing-xs);
        align-items: flex-start;
    }

    .line-text {
        min-width: 0;
    }

    .quiet {
        color: var(--color-text-tertiary);
    }

    .notable {
        display: flex;
        gap: var(--spacing-xs);
        align-items: baseline;
    }

    .mark {
        flex: none;
        color: var(--color-warning-text);
        position: relative;
        top: 1px; /* An optical nudge onto the text baseline. */
    }

    .skeleton {
        display: inline-block;
        width: 40%;
        height: 0.8em;
        border-radius: var(--radius-xs);
        background: var(--color-bg-tertiary);
        vertical-align: middle;
    }
</style>
