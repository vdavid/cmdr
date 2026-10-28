# Suggested-ops tools — details

Pull-tier docs for `agent/tools/suggestions/`. Must-knows: `CLAUDE.md`. The rows and the lifecycle:
`../../store/proposals/DETAILS.md`. Selector resolution and the metric: `../../suggested_ops/DETAILS.md`.

## The three tools, and why the access classes fall where they do

- `list_suggestions` — `Read`. Sweeps and groups as summaries with `COUNT(*)` counts.
- `get_suggestion_group` — `Read`. One group's ops, paged and budget-cut.
- `propose_suggestions` — `Propose`. A staged sweep, or an amended pending group.

`Propose` means "stages a proposal and opens a review surface, mutates nothing"; `Write` means "mutates the filesystem
OR app state" and is never reachable from the agent view. Staging rows in `main.db` IS the proposal, which is why it
stays `Propose` — but a standalone `amend` tool would be a stored-state mutation with no review surface of its own, so
the enum's own tiebreaker makes it `Write` and `test_agent_tool_view_never_writes` would refuse it. Folding the amend
into `propose_suggestions` is what keeps the surface honest.

Only `propose_suggestions` joins `EXPECTED_PROPOSE_TOOL_NAMES` (`mcp/tests/tool_registry_tests/access.rs`), a hand-authored
allowlist by design: no structural check can prove a handler doesn't mutate.

## `propose_suggestions`: place, resolve, check, then write

The handler runs in that order, and the order is the contract:

1. **Place** every location every group names (`plan_sweep`, over one `VolumeFacts` snapshot; § Where a proposal may
   point). Before the snapshot stats a source, so a re-addressed group freezes its facts from the volume it will run on.
2. **Resolve** every group's ops (`apply_planned_sweep` from here on): a selector against the drive index, an explicit
   list as given.
3. **Check** every amendment's target: the group exists, it belongs to the named sweep, and it is still `pending`.
4. **Write**: create the sweep (or reuse the named one), then each group through `suggested_ops::add_group` /
   `repropose`.

A refusal anywhere in 1–3 leaves the store exactly as it was. The alternative (write what validates, report the rest)
was rejected because a sweep is what the user reads in one sitting: a partly-applied amendment shows some groups as the
agent revised them and some as it left them, with nothing on screen to say which is which.

Between the check and the write there is still a race, the user answering a group in that instant. It can't be closed
without one transaction spanning every group, and it doesn't need to be: `repropose_group` is itself conditional on
`pending`, so the loser reports `GroupOutcome::AlreadyAnswered` with `opCount: 0` and the user's answer stands. The
report names what landed, so a partial application is a stated outcome rather than a silent one.

### Why the ownership check exists at all

`repropose_group` already refuses a non-pending group, so the pre-check looks redundant. It isn't:

- It makes the refusal ATOMIC across the call. Without it, group 1 lands and group 2 refuses.
- It carries the group's actual status into the refusal (`GroupNotPending { status }`), so the model can tell "the user
  already approved this" from "there's no such group" and say the right thing.
- The sweep-membership half has no equivalent downstream at all. Without it an agent could rewrite any group by number,
  including one from another conversation's sweep.

### The refusal vocabulary

Every refusal crosses the wire as `{ readyForReview: false, refusal: <token>, group: <index>, problem: <sentence> }`,
except a location miss, which speaks the read tools' miss shape instead (§ Where a proposal may point).
The token is the typed classification (nothing downstream matches on the sentence, per the repo's no-error-string-match
rule); the sentence is what the model relays, and it says what to send instead. The group index is the position in the
call, because the model's only recovery is to send the whole call again and it needs to know which group to fix.

## Where a proposal may point

A proposal names only places that can run now, so approval never refuses for a reason the model could have been told
while it was still talking (`place.rs`, over the location resolver: `../DETAILS.md` § The location contract).

- **Every place resolves**: `destination`, `selector.root`, a rename's `parent`, and each hand-listed source in `paths`
  or `renames`. A source resolves NAMED, with `sourceVolumeId` (one volume per group is the spine's rule, so it stays
  required): a bare resolve of a path on a sleeping share is `noVolumeAtPath`, which loses `sameShareAs`, and naming the
  volume is also what catches a hallucinated volume/path pair (`pathOutsideVolume`). `volumeId` is optional in
  `destination` and `selector.root`: a bare path resolves to the live volume holding it, or misses.
- **One snapshot per call**: `VolumeFacts::now()` once, in memory, before anything else; 200 pure resolves per group
  cost nothing. A path under a root the volume held earlier stores the rebased path.
- **A miss refuses the whole call** in the read tools' shape, flattened beside the envelope:
  `{ readyForReview: false, refusal: "location", group, field, miss, location, detail, nearby?, sameShareAs?, root? }`.
  `field` is `destination` / `selector.root` / `paths` / `renames` / `parent`. A place on a volume that isn't live, with
  no sibling to take it, is `volumeNotConnected` (`location::miss::volume_not_connected`), carrying `nearby` (each with
  its `candidate`) and `sameShareAs`, which is how the model offers `connect_volume`.
- **A proven sibling re-addresses instead**: when the named volume isn't live, the first live member `readers` offers
  (same share, same account, BOTH GUIDs recorded by a sign-in: `location/same_share.rs`) takes the place, and the group
  is STORED there: its volume id, its rebased paths. The stored group is what runs, and the dead id would only refuse at
  approval. The report's group entry carries one `via: { volumeId }` naming the volume asked, and the dialog's
  `destinationVolumeName` derives from the stored destination, so it names the volume written to
  (`commands/agent/suggested_ops_test.rs` pins that). A probe-filled or guest GUID, or another account, is only a
  `sameShareAs` hint: re-addressing a WRITE onto whatever answered a probe of a saved address would send the user's
  files there.
- **One `via` per group**: a group whose places would re-address onto two members, or re-address one place while
  another names a member of that share directly, refuses as `mixedReaddress`. Simpler and safer than a `via` per field.
- **A selector re-addressed** resolves against the sibling's index; `notIndexed` there is the selector's own refusal.
- **`WritableDestination` applies to the placed path**, where the group will really write.
- **Amendments place again**: an amend is a fresh proposal of the group.
- The review's `DestinationState::NotConnected` stays: a volume can leave after the proposal.

## Moving or copying under a new name

A move or copy names its sources one of two ways: `paths` (every file keeps its name) or `renames`
(`[{ path, newName }]`, the same list a rename takes). Under move or copy the model lists EVERY file in `renames`, and a
file given its own name keeps it. That's the shape the model already knows for "this file becomes that name", it costs
almost no prefix, and `proposal_ops.destination` already means exactly this per op. The group stays ONE operation: the
bridge hands the engine one per-source name map (§ The approval bridge in `../../suggested_ops/DETAILS.md`), so 12
renamed invoices are one approval, one queue row, and one undo, which puts every original name back.

- **Shapes**: `renames` under move or copy plans as `PlannedOps::Named` (a `NamedShape` plus `TransferOp`s), the only
  way a name reaches `GroupIntent::Move` / `Copy`. A selector's ops enter through `SourceShape::into_intent`, nameless,
  so a pattern can never mint a name. Extract, compress, trash, and delete still refuse `renames`
  (`renamesVerbMismatch`); their ops have no field to hold one.
- **Normalization**: under move or copy, a name equal to the source's own is stored as `NULL`, so the dialog draws an
  arrow only where a name changes. ❌ Never under rename: the rename review reads `op.destination.unwrap_or_default()`,
  and a `NULL` there renames the file to an empty name. A group whose names all normalize away is a plain move or copy
  and gets no name checks, the same as one sent as `paths`.
- **The checks are the engine's, asked early** (`names.rs`), on the PLACED group (§ Where a proposal may point), so they
  judge where the names really land: `cmdr_operations::TransferLeaves::check_landings`, the rules the transfer start enforces
  (`crates/cmdr-operations/src/transfer/DETAILS.md` § Named destinations). A plain leaf
  (`cmdr_fs::validation::validate_plain_leaf`, which a rename's names pass too) is `notABareName` with the rule broken;
  two landings one under `fold_name` is `duplicateLeaf`; a landing on another source of the group is `chainOverwrite`;
  any source already in the destination is `useRename` ("split the group"), because the same-volume move's identity
  guard drops a case-only name there and the rename verb is the one that works. The engine adds `dev+ino` where it can
  stat; the proposal can't, so the folded comparison is its check.
- **Two sources sharing an ORIGINAL name** (`paths` or `renames`) are the route's question, asked after the plan by
  `input::check_own_names` through `cmdr_operations::TransferLeaves::check_own_names` on the live source and destination
  volumes (`names::own_names`): where both ends sit on a local path the native engines run, key each subtree on
  `<destination>/<own name>`, and refuse the pair at the start, named apart or not. So the proposal refuses the whole
  call as `sharedName` ("separate groups, different folders"); a route whose engine keys landings on the source path
  takes them. A separate step because the engine is the live volumes' to say and `plan_sweep` stays pure over its
  facts. A selector's sources resolve at the write, so a selector group isn't asked.
- **A phone's names**: onto an MTP or ADB destination a name is also at most 254 UTF-16 code units and holds none of
  FAT's reserved `" * : < > ? \ |`. Proposal-time only: nothing below refuses them first (`crates/cmdr-mtp/DETAILS.md` §
  Names a phone takes).
- **A collision with a file already at the destination that isn't a source is not refused**: it meets the ordinary
  conflict policy at run time, as a clicked move would. The review dialog warns on the row before the click (#774,
  `apps/desktop/src/lib/suggested-ops/DETAILS.md` § The file list).
- Each name is bound by the claim's digest with no change (the digest already hashes `destination` per op), so an amend
  that only renames is a new approval.

## Proposing a server

`propose_server` stages one zero-op `SaveServer` group (`../../store/proposals/DETAILS.md` § A server group): a server to
add, or, with `editing`, a change to a saved one. It saves nothing and contacts nothing; the user's approval saves
through the servers hub's own cores (`saved_servers/`). It reuses `propose_suggestions`' sweep, amend (`sweepId` plus
`groupId`, `pending` only), and report shape, one group long. Order: origin, shape, values, saved-server checks, then
the write; any refusal writes nothing.

- **Only on a turn the user started.** The handler reads the turn's origin (`CallContext`, routed by `view.rs`) and
  answers `refusedInBackground` on a wake or a follow-up, before it reads a store. ❌ Not a tool-name check in `admit`:
  the gate stays tier-only and this rule lives with the one tool it belongs to. A badge saying "AI suggestion: add
  server naspi-backup" arriving unprompted is the social-engineering shape an injection wants. The registry path
  (`execute_propose_server`) knows no turn, so it refuses the same way.
- **No secret, ever.** The schema has no field for one and is closed like every agent schema, so the dispatch gate
  refuses a `password` key by name first; the handler refuses `password`, `secret`, `secretAccessKey`, `keyFile`, and
  their kin again as `secretRefused` (any path that skips the gate), and a URL carrying a password too. `useAgent`,
  `autoReconnect`, and a pin aren't fields: an add takes the add form's defaults, an edit keeps the stored values.
- **An approved SFTP add saves with the SSH agent OFF** (`useAgent: false`), the one default it doesn't take. On, the
  first connect to the host would offer every key in the user's agent to it, and the AI picked that host: a host an
  injected file name chose would learn which public keys the user holds, and could be one that logs or relays them.
  The user turns the agent on in the servers hub when they trust the host. The card lists no switches, so it shows
  none for this either.
- **The field map** (a field outside its row is `unboundField`; an add without a required one is `missingField`;
  defaults are the add form's, `apps/desktop/src/lib/servers/server-form.ts`):
  - `smb`. Add: `host` (required), `port` (445), `username`, `share`, `name` (the HOST's). Edit (`editing` is any of
    the host's share volume ids): `name` (the host's, ❌ never a share's). The draft records how many saved shares the
    host has, for the card.
  - `sftp`. Add: `host`, `username` (both required), `port` (22), `path` (the remote root, `/`), `startFolder`, `name`.
    Edit: `name`, `path`, `startFolder`.
  - `webdav`. Add: `url`, `username` (both required), `path` (`/`), `startFolder`, `name`. Edit: `name`, `path`,
    `startFolder`.
  - `nfs`. Add: `host`, `export` (both required), `port` (absent asks the portmapper), `uid`, `gid`, `startFolder`,
    `name`. Edit: `name`, `startFolder`. An add without `uid` / `gid` stores none: the add form's guess
    asks the server, which an approval mustn't, so the approval decides.
  - `s3`. Add: a BUCKET place under a saved account, `account` (its listing id) and `bucket`, plus `name`. Edit: `name`.
    No account at all is a new account, `protocolNotProposable` (the user adds one in Servers). Why so narrow: a new
    account needs the provider-and-endpoint union in the schema, paid on every turn for a rare case, and its key id
    can only come from the user, who is better served by the add sheet.
  - ❗ An NFS or S3 `name` is the SERVER's or the ACCOUNT's (the stores keep no per-export or per-bucket name), so it
    renames the row every place under it shares.
- **Identity fields** (`host`, `port`, `url`, `export`, `account`, `bucket`, the protocol, and who the server signs
  in as: every protocol's `username`, NFS's `uid` and `gid`) are immutable in an edit: an edit may restate one in the
  store's own spelling (a host's case, a URL's trailing slash, an export's `//`), and the draft keeps the SAVED
  spelling; any other value is `addressChange`. **Why**: the app's edit MOVES the server and its stored password to a
  new address, and auto-reconnect would then send it there with nobody watching, so one approved click would hand a
  credential to whatever answers at an address an injected file name chose. A new account is the same shape the other
  way round: an SMB host signing in as someone else, or an NFS export acting as uid 0, is a power the user never picked.
- **Every value one plain thing.** `name`, `share`, `username`, and `bucket` pass `validate_plain_leaf`; `path`,
  `export`, and `startFolder` are absolute, each folder a plain leaf; a host is a plain ASCII host name or IP literal
  (a look-alike of a saved host, `nаspolya` with a Cyrillic `а`, can't stand in for it; an internationalized name goes
  as `xn--`); a URL is ASCII `http` or `https` with a host and no account in it; every string is capped. A start folder
  outside the remote root refuses here rather than at the save. One token, `badField`, with the rule's sentence.
- **Against what's saved** (`SavedServers` and its checks, in `../../suggested_ops/server/` so the approval runs the
  same ones: one in-memory read per call through `saved_servers::saved_now` and `saved_by_id`, ❌ never the network):
  - `alreadySaved` (an add): the draft's place id through `target_ids::place_id`, the listing's own id funnel, so a
    host's case or a URL's path can't dodge it; an SMB host by its address (any case) and port. It carries the id to
    send as `editing` instead. An add never silently replaces a saved entry's settings.
  - `nameTaken`: a `name` equal under `fold_name` to ANOTHER saved row's listed name (a stand-in address included), so
    an attacker host can't wear the NAS's name. The row the draft joins or edits doesn't count.
  - `unknownServer`: an `editing` id (or an S3 `account`) nothing saved has.
  - An edit stores `previous`, the saved entry's editable fields AND its address, read here; the approval re-reads the
    entry and re-runs `alreadySaved` and `nameTaken`, because a server saved since can make either true
    (`../../suggested_ops/DETAILS.md` § Approving a server).
  - An NFS add that names no `uid` / `gid` takes this Mac's (`saved_servers::nfs_places::this_mac_identity`), the add
    form's own no-contact fallback, so the card shows the identity the click saves.
- **The report** is `propose_suggestions`' `ProposeReport` with one `opCount: 0` group; the rail line counts the card,
  not its ops (`../line/DETAILS.md`).

## The selector schema, and the one predicate that isn't there

The model writes predicates in units it states reliably: whole **days ago** (converted to unix seconds against `now`,
injected so a test is deterministic) and whole **bytes**. `OpSelector` stores seconds, which is what the index compares.

Two ages BAND a range: "older than 30 and newer than 90" means 30–90 days old and is legitimate; the reverse is empty
and is refused as `ImpossibleWindow`, as is an inverted size window. Proposing over an empty window costs the user a
review that can't contain anything.

**"Last opened" is not expressible**, and it is the feature's flagship example ("installers you've already opened"). The
drive index carries size, modification time, and inode; there is no access time, and `importance.db`'s visit counts are
per-FOLDER, not per-file (verified against M1's selector resolver and the importance schema, 2026-08-18). A predicate
that compiled and matched nothing would be worse than its absence: the agent would propose over it and the user would
review an empty group. So the schema omits it and the system prompt carries the positive instruction — say when the
file last CHANGED, never imply Cmdr knows what the user opened. Wiring an access-time source is its own effort.

## The two reads

**`list_suggestions`** maps rows to `GroupSummaryOut` FIRST, then cuts to the result budget, then reads only the sweeps
the surviving groups belong to (`get_sweep`, one header at a time). A long backlog therefore costs a handful of header
reads rather than one per group. Nesting is by first appearance, so the newest-first order the store returns survives.
A group whose sweep can't be read is dropped rather than shown parentless: the sweep is what dates a suggestion, and an
undated one reads as new.

Every group summary carries BOTH ids (`groupId`, `sweepId`), because an amendment needs the pair, and both counts
(`opCount` live, `excludedOpCount` for what the user deselected), since a deselected op keeps its row. A server group
(`addServer` / `editServer`) has `opCount: 0` and carries `server`: its draft (protocol, name, address, account, and the
rest it holds; there's no field for a secret) plus `editing` for an edit. `propose_suggestions` refuses a server verb as
`notAFileVerb`: the verb type is the store's, so it deserializes, and a file sweep never builds a server group.

**`get_suggestion_group`** reports `truncated` when the size cut took rows off the page OR the page doesn't reach the
end of the group: both mean "there's more", and the model has to say "returned of total" either way. Its op fields are
named `snapshotSize` / `snapshotModified` because they are what the index held AT CREATION, not what the file is now; a
size relayed as current would be a claim nothing here can back. An unknown size gets no human string at all, because
`"0 B"` reads as an empty file.

An id that names no group answers `{ found: false, groupId }` rather than erroring: the group may simply be one the
user already dealt with, which is an answer, not a fault.

## The conversation link

`view.rs` routes `propose_suggestions` through `propose_in_thread` with `EvidenceScope::conversation_id()`, so a sweep
records the thread it came out of. The registry path (an external MCP client) has no thread and stores `NULL`, which is
what a background wake will store in M5 too. The column is `ON DELETE SET NULL`, so tidying a chat thread away later
leaves the decision record whole.

## Caps, and what they're for

`MAX_GROUPS` (16) is a sitting's worth of review. `MAX_PATHS` (200) matches `image_facts`' cap, and past it the answer
is a selector: a list the model can't hold is a list it starts inventing. Both are INPUT caps. What a selector resolves
to has no cap at all, because 60,000 ops in one group is a legitimate group.

## Testing

`tests/` runs without a Tauri app: the handlers are thin shells over `plan_sweep` (fixture `VolumeFacts`),
`apply_planned_sweep` (a `Connection` plus a `SelectorIndex`), `shape_list`, and `shape_group`. One file per concern
(`input`, `names`, `place`, `propose`, `read`; `place` reuses the conversation-179 facts from `../transcript_179_tests.rs`), with the shared
fixtures in `tests/mod.rs`. The fake index counts how often it was asked, which is what pins
"resolved exactly once". The write-path tests run against a real migrated in-memory `main.db`, so the store's own
guards are in the loop rather than mocked away: removing the pre-check's status guard turns an amendment into
`AlreadyAnswered` instead of a rewrite, which is how the two layers were confirmed to be independent.
