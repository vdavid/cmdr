/**
 * The words a tool line says: its label with the subject in it, what came back, and the turn's
 * stats line. English, the source catalog.
 */

import { describe, expect, it } from 'vitest'
import type {
  ConnectOutcome,
  MemoryRefusalKind,
  MissToken,
  RefusalReason,
  ToolOutcome,
  ToolSubject,
} from '#lib/ipc/bindings.js'
import { lineLabelText, toolLineLabel, toolOutcomeText, turnStatsText } from './ask-cmdr-labels'
import type { RailToolCall } from './ask-cmdr-messages'

function call(tool: string, subject: ToolSubject | null, overrides: Partial<RailToolCall> = {}): RailToolCall {
  return {
    callId: 'c1',
    tool,
    running: false,
    subject,
    outcome: { kind: 'answered' },
    repeatOf: null,
    ...overrides,
  }
}

function said(tool: string, subject: ToolSubject | null, overrides: Partial<RailToolCall> = {}): string {
  return lineLabelText(toolLineLabel(call(tool, subject, overrides)))
}

describe('toolLineLabel', () => {
  it('names the folder a listing looked at, running and done, as a place to go', () => {
    const folder: ToolSubject = { kind: 'folder', path: { text: '/Users/me/Downloads', linkable: true } }
    expect(said('list_dir', folder, { running: true, outcome: null })).toBe('Listing Downloads')
    expect(said('list_dir', folder)).toBe('Listed Downloads')
    expect(toolLineLabel(call('list_dir', folder)).place).toEqual({
      path: '/Users/me/Downloads',
      kind: 'folder',
      shown: 'Downloads',
      linked: true,
    })
    expect(said('folder_importance', folder)).toBe('Checked how important Downloads is')
  })

  it('shows the root as itself, since it has no last segment', () => {
    expect(said('list_dir', { kind: 'folder', path: { text: '/', linkable: true } })).toBe('Listed /')
  })

  it('falls back to today’s words when the subject is unknown or unreadable', () => {
    expect(said('list_dir', null)).toBe('Listed a folder')
    expect(said('list_dir', { kind: 'unparsed' }, { running: true, outcome: null })).toBe('Listing a folder')
    expect(said('search', { kind: 'unparsed' })).toBe('Searched your files')
  })

  it('shows "Working" for a tool the rail has no label for', () => {
    expect(said('some_new_tool', null, { running: true, outcome: null })).toBe('Working')
    expect(said('some_new_tool', null)).toBe('Used a tool')
  })

  it('names the pane’s folder once the call says which, and whether it read the selection', () => {
    expect(said('list_pane_files', { kind: 'focusedPane', folder: null, selectionOnly: null })).toBe('Listed a folder')
    expect(
      said('list_pane_files', {
        kind: 'focusedPane',
        folder: { text: '/Users/me/Desktop', linkable: true },
        selectionOnly: false,
      }),
    ).toBe('Listed Desktop')
    expect(
      said('list_pane_files', {
        kind: 'focusedPane',
        folder: { text: '/Users/me/Desktop', linkable: true },
        selectionOnly: true,
      }),
    ).toBe('Listed your selection in Desktop')
  })

  it('names the first file, and how many more the call asked about', () => {
    expect(said('inspect_file', { kind: 'files', firstPath: { text: '/a/notes.txt', linkable: true }, count: 1 })).toBe(
      'Looked inside notes.txt',
    )
    expect(said('inspect_file', { kind: 'files', firstPath: { text: '/a/notes.txt', linkable: true }, count: 3 })).toBe(
      'Looked inside notes.txt and 2 more',
    )
    expect(said('image_facts', { kind: 'files', firstPath: { text: '/a/IMG_1.jpg', linkable: true }, count: 1 })).toBe(
      'Read what’s in IMG_1.jpg',
    )
    expect(
      toolLineLabel(
        call('inspect_file', { kind: 'files', firstPath: { text: '/a/notes.txt', linkable: true }, count: 1 }),
      ).place?.kind,
    ).toBe('file')
  })

  it('words a search by what it has: a pattern, folders to look in, both, or neither', () => {
    const search = (query: string | null, scopePaths: string[]): string =>
      said('search', { kind: 'search', query, scopePaths: scopePaths.map((text) => ({ text, linkable: true })) })
    expect(search('*.pdf', ['/Users/me/Documents'])).toBe('Searched for “*.pdf” in Documents')
    expect(search('*.pdf', ['/Users/me/Documents', '/Users/me/Desktop'])).toBe(
      'Searched for “*.pdf” in Documents and Desktop',
    )
    expect(search('*.pdf', [])).toBe('Searched for “*.pdf”')
    expect(search(null, ['/Users/me/Documents'])).toBe('Searched in Documents')
    expect(search(null, [])).toBe('Searched your files')
    const label = toolLineLabel(
      call('search', {
        kind: 'search',
        query: 'x',
        scopePaths: [
          { text: '/a', linkable: true },
          { text: '/b', linkable: true },
        ],
      }),
    )
    expect(label.places.map((place) => place.path)).toEqual(['/a', '/b'])
    expect(label.places.every((place) => place.kind === 'folder' && place.linked)).toBe(true)
  })

  it('passes a search’s folder count, so a case language can tell one folder from several', () => {
    const label = (query: string | null, scopePaths: string[]) =>
      toolLineLabel(
        call('search', { kind: 'search', query, scopePaths: scopePaths.map((text) => ({ text, linkable: true })) }),
      )
    expect(label('x', ['/a', '/b'])).toMatchObject({
      key: 'askCmdr.tool.search.queryInFolders.done',
      params: { folders: 2 },
    })
    expect(label(null, ['/a'])).toMatchObject({ key: 'askCmdr.tool.search.inFolders.done', params: { folders: 1 } })
  })

  it('names a photo search’s words, and counts what a plan carries', () => {
    expect(said('search_photos', { kind: 'photoSearch', query: 'beach' })).toBe('Searched your photos for “beach”')
    expect(said('propose_rename_plan', { kind: 'renamePlan', count: 1 })).toBe('Prepared a plan to rename 1 file')
    expect(said('propose_rename_plan', { kind: 'renamePlan', count: 12 })).toBe('Prepared a plan to rename 12 files')
    expect(said('propose_suggestions', { kind: 'suggestionSweep', count: 2 })).toBe('Prepared 2 suggestions')
  })

  it('links a note only when it lands inside the notes folder', () => {
    const inside: ToolSubject = { kind: 'memoryFile', path: 'prefs.md', absolute: '/data/memory/prefs.md' }
    const outside: ToolSubject = { kind: 'memoryFile', path: '../etc/x.md', absolute: null }
    expect(said('memory_write', inside)).toBe('Saved a note to prefs.md')
    expect(said('memory_edit', inside)).toBe('Updated prefs.md')
    expect(toolLineLabel(call('memory_write', inside)).place).toEqual({
      path: '/data/memory/prefs.md',
      kind: 'file',
      shown: 'prefs.md',
      linked: true,
    })
    expect(toolLineLabel(call('memory_write', outside)).place?.linked).toBe(false)
    expect(toolLineLabel(call('memory_write', outside)).place?.shown).toBe('../etc/x.md')
  })

  it('never claims a refused note was saved', () => {
    const refused: ToolOutcome = { kind: 'refused', reason: { kind: 'memory', refusal: 'outsideMemory' } }
    const inside: ToolSubject = { kind: 'memoryFile', path: 'prefs.md', absolute: '/data/memory/prefs.md' }
    expect(said('memory_write', inside, { outcome: refused })).toBe('Tried to save a note to prefs.md')
    expect(said('memory_edit', inside, { outcome: refused })).toBe('Tried to update prefs.md')
    expect(said('memory_write', null, { outcome: refused })).toBe('Tried to save a note')
    expect(said('memory_edit', null, { outcome: refused })).toBe('Tried to update its notes')
  })

  it('never claims a server connected when it didn’t', () => {
    const volume: ToolSubject = { kind: 'volume', id: 'smb-testshare' }
    expect(said('connect_volume', volume, { running: true, outcome: null })).toBe('Connecting to your server')
    expect(said('connect_volume', volume, { outcome: { kind: 'connection', outcome: 'connected' } })).toBe(
      'Connected to your server',
    )
    expect(said('connect_volume', volume, { outcome: { kind: 'connection', outcome: 'alreadyConnected' } })).toBe(
      'Connected to your server',
    )
    expect(said('connect_volume', volume, { outcome: { kind: 'connection', outcome: 'needsCredentials' } })).toBe(
      'Tried connecting to your server',
    )
  })

  it('names what a call outside the tool table asked for, and keeps the generic words for a nameless one', () => {
    expect(said('format_drive', { kind: 'unknownTool', name: 'format_drive' })).toBe('Asked for “format_drive”')
    expect(said('', { kind: 'unknownTool', name: '' })).toBe('Used a tool')
    expect(said('', { kind: 'unknownTool', name: '' }, { running: true, outcome: null })).toBe('Working')
  })

  it.each([
    'select_volume',
    'nav_to_path',
    'nav_to_parent',
    'nav_back',
    'nav_forward',
    'scroll_to',
    'move_cursor',
    'open_under_cursor',
    'select',
    'copy',
    'move',
    'compress',
    'delete',
    'rename',
    'multi_rename',
    'mkdir',
    'mkfile',
    'refresh',
    'tag',
    'toggle_hidden',
    'set_view_mode',
    'sort',
    'tab',
    'dialog',
    'open_search_dialog',
    'quit',
    'switch_pane',
    'swap_panes',
    'ai_search',
    'set_setting',
    'indexing',
    'queue',
    'resolve_conflict',
    'unlock',
    'favorites',
    'connect_to_server',
    'remove_manual_server',
    'upgrade_smb_to_direct',
    'eject',
    'await',
    'go_to_latest_download',
    'operations_rollback',
    'memory_diagnostics',
    'read_resource',
    'cpu_diagnostics',
  ])('gives %s its own words, running and done', (tool) => {
    const running = said(tool, { kind: 'nothing' }, { running: true, outcome: null })
    const done = said(tool, { kind: 'nothing' })
    expect(running).not.toBe('Working')
    expect(done).not.toBe('Used a tool')
    expect(done).not.toBe(running)
  })

  it('never claims a write happened when the call was refused', () => {
    const refused: ToolOutcome = { kind: 'refused', reason: { kind: 'toolUnavailable' } }
    expect(said('copy', { kind: 'nothing' })).toBe('Opened the copy dialog')
    expect(said('copy', { kind: 'nothing' }, { outcome: refused })).toBe('Tried to open the copy dialog')
    expect(said('sort', { kind: 'pane', pane: 'left' }, { outcome: refused })).toBe('Tried to sort a pane')
    expect(said('set_setting', { kind: 'setting', id: 'appearance.theme' }, { outcome: refused })).toBe(
      'Tried to change a setting',
    )
    expect(said('eject', { kind: 'volume', id: 'disk-1' }, { outcome: refused })).toBe('Tried to detach a drive')
  })

  it('names the place a pane went to or a password was for, as a place to go', () => {
    const place: ToolSubject = { kind: 'place', path: { text: '/Users/me/Downloads', linkable: true } }
    expect(said('nav_to_path', place, { running: true, outcome: null })).toBe('Going to Downloads')
    expect(said('nav_to_path', place)).toBe('Went to Downloads')
    expect(said('nav_to_path', place, { outcome: { kind: 'refused', reason: { kind: 'problem' } } })).toBe(
      'Tried to go to Downloads',
    )
    expect(toolLineLabel(call('nav_to_path', place)).place).toEqual({
      path: '/Users/me/Downloads',
      kind: 'folder',
      shown: 'Downloads',
      linked: true,
    })
    expect(said('unlock', place)).toBe('Sent the password for Downloads')
    expect(said('nav_to_path', { kind: 'unparsed' })).toBe('Went to a folder')
  })

  it('says a naming dialog opened when mkdir or mkfile left the name to a person', () => {
    const waiting: ToolOutcome = { kind: 'awaitingPerson', dialog: 'mkdir-confirmation' }
    const pane: ToolSubject = { kind: 'pane', pane: 'left' }
    expect(said('mkdir', pane)).toBe('Created a folder')
    expect(said('mkdir', pane, { outcome: waiting })).toBe('Opened the new folder dialog')
    expect(said('mkfile', pane, { outcome: waiting })).toBe('Opened the new file dialog')
    expect(said('copy', { kind: 'nothing' }, { outcome: waiting })).toBe('Opened the copy dialog')
  })

  /** Exhaustive by type: a new subject kind fails to compile here until it has a sample. */
  it('says something for every subject kind, running and done', () => {
    const samples: Record<ToolSubject['kind'], [string, ToolSubject]> = {
      panes: ['app_state', { kind: 'panes' }],
      nothing: ['nav_back', { kind: 'nothing' }],
      place: ['nav_to_path', { kind: 'place', path: { text: '/a', linkable: true } }],
      pane: ['sort', { kind: 'pane', pane: 'left' }],
      setting: ['set_setting', { kind: 'setting', id: 'appearance.theme' }],
      resource: ['read_resource', { kind: 'resource', view: 'logs' }],
      focusedPane: [
        'list_pane_files',
        { kind: 'focusedPane', folder: { text: '/a', linkable: true }, selectionOnly: false },
      ],
      folder: ['list_dir', { kind: 'folder', path: { text: '/a', linkable: true } }],
      files: ['inspect_file', { kind: 'files', firstPath: { text: '/a/b', linkable: true }, count: 1 }],
      search: ['search', { kind: 'search', query: 'x', scopePaths: [] }],
      photoSearch: ['search_photos', { kind: 'photoSearch', query: 'x' }],
      importantFolders: ['important_folders', { kind: 'importantFolders' }],
      drives: ['list_volumes', { kind: 'drives' }],
      volume: ['connect_volume', { kind: 'volume', id: 'smb-testshare' }],
      operationLog: ['operations_list', { kind: 'operationLog' }],
      operation: ['operations_get', { kind: 'operation', id: 'op-1' }],
      renamePlan: ['propose_rename_plan', { kind: 'renamePlan', count: 2 }],
      suggestions: ['list_suggestions', { kind: 'suggestions' }],
      suggestionGroup: ['get_suggestion_group', { kind: 'suggestionGroup' }],
      suggestionSweep: ['propose_suggestions', { kind: 'suggestionSweep', count: 2 }],
      serverSuggestion: ['propose_server', { kind: 'serverSuggestion' }],
      nothingToSuggest: ['nothing_to_suggest', { kind: 'nothingToSuggest' }],
      memoryFile: ['memory_write', { kind: 'memoryFile', path: 'a.md', absolute: null }],
      unknownTool: ['nope', { kind: 'unknownTool', name: 'nope' }],
      unparsed: ['list_dir', { kind: 'unparsed' }],
    }
    for (const [tool, subject] of Object.values(samples)) {
      expect(said(tool, subject, { running: true, outcome: null }).length).toBeGreaterThan(0)
      expect(said(tool, subject).length).toBeGreaterThan(0)
    }
  })
})

describe('toolOutcomeText', () => {
  const settled = (outcome: ToolOutcome | null): string | null =>
    toolOutcomeText(call('list_dir', null, { outcome, running: false }))

  it('counts items, and says how many of how many when the model saw part of a list', () => {
    expect(settled({ kind: 'items', returned: 1, total: 1, lowerBound: false })).toBe('1 item')
    expect(settled({ kind: 'items', returned: 42, total: null, lowerBound: false })).toBe('42 items')
    expect(settled({ kind: 'items', returned: 200, total: 4812, lowerBound: false })).toBe('showed 200 of 4,812')
    expect(settled({ kind: 'items', returned: 200, total: 4812, lowerBound: true })).toBe(
      'showed 200 of at least 4,812',
    )
  })

  it('counts matches, and never says "none" while the search was still covering ground', () => {
    expect(settled({ kind: 'matches', returned: 20, matchCount: 1234, lowerBound: false })).toBe('1,234 matches')
    expect(settled({ kind: 'matches', returned: 1, matchCount: 1, lowerBound: true })).toBe('1 match so far')
    expect(settled({ kind: 'nothingFound', lowerBound: false })).toBe('nothing found')
    expect(settled({ kind: 'nothingFound', lowerBound: true })).toBe('nothing found yet')
  })

  it('says nothing more for a plain answer, a saved note, or a fresh connection: the label already said it', () => {
    expect(settled({ kind: 'answered' })).toBeNull()
    expect(settled({ kind: 'saved' })).toBeNull()
    expect(settled({ kind: 'connection', outcome: 'connected' })).toBeNull()
  })

  it('says why a place couldn’t be read', () => {
    expect(settled({ kind: 'locationMiss', miss: 'volumeNotConnected' })).toBe('that drive isn’t connected')
    expect(settled({ kind: 'locationMiss', miss: 'needsDiskAccess' })).toBe('needs Full Disk Access')
  })

  it('says nothing more when the count only repeats the paths the label names, and keeps a partial answer', () => {
    const files: ToolSubject = { kind: 'files', firstPath: { text: '/a/notes.txt', linkable: true }, count: 2 }
    const outcomeOf = (tool: string, outcome: ToolOutcome): string | null =>
      toolOutcomeText(call(tool, files, { outcome }))
    const all: ToolOutcome = { kind: 'items', returned: 2, total: 2, lowerBound: false }
    expect(outcomeOf('inspect_file', all)).toBeNull()
    expect(outcomeOf('image_facts', all)).toBeNull()
    expect(outcomeOf('inspect_file', { kind: 'items', returned: 1, total: 2, lowerBound: false })).toBe('showed 1 of 2')
    // A folder's count is news: the label names the folder, not how much it holds.
    expect(
      toolOutcomeText(call('list_dir', { kind: 'folder', path: { text: '/a', linkable: true } }, { outcome: all })),
    ).toBe('2 items')
  })

  it('says a dialog is waiting, that a person said no, and that contents stayed private', () => {
    expect(settled({ kind: 'awaitingPerson', dialog: null })).toBe('waiting for you')
    expect(settled({ kind: 'outcomeUnknown' })).toBe('may still be going')
    expect(settled({ kind: 'refused', reason: { kind: 'inBackground' } })).toBe('waits for you to ask')
    expect(settled({ kind: 'refused', reason: { kind: 'declined' } })).toBe('you said no')
    expect(settled({ kind: 'refused', reason: { kind: 'protected' } })).toBe('kept private')
  })

  it('says nothing while the call runs, and that no answer was recorded for a settled one without', () => {
    expect(toolOutcomeText(call('list_dir', null, { running: true, outcome: null }))).toBeNull()
    expect(settled(null)).toBe('no answer recorded')
  })

  /** Exhaustive by type: a new outcome kind or refusal reason fails to compile until it has copy. */
  it('has words for every outcome, refusal reason, and memory refusal', () => {
    const outcomes: Record<ToolOutcome['kind'], ToolOutcome[]> = {
      items: [{ kind: 'items', returned: 3, total: 3, lowerBound: false }],
      matches: [{ kind: 'matches', returned: 3, matchCount: 3, lowerBound: false }],
      nothingFound: [{ kind: 'nothingFound', lowerBound: false }],
      answered: [],
      indexUnavailable: [
        { kind: 'indexUnavailable', reason: 'imageIndexingOff' },
        { kind: 'indexUnavailable', reason: 'semanticModelNotInstalled' },
      ],
      staged: [{ kind: 'staged', count: 4 }],
      notStaged: [{ kind: 'notStaged' }],
      awaitingPerson: [{ kind: 'awaitingPerson', dialog: null }],
      outcomeUnknown: [{ kind: 'outcomeUnknown' }],
      saved: [],
      refused: [],
      locationMiss: [],
      connection: [],
    }
    const misses: Record<MissToken, true> = {
      unknownVolume: true,
      pathOutsideVolume: true,
      noVolumeAtPath: true,
      volumeNotConnected: true,
      needsDiskAccess: true,
      unreachable: true,
      notFound: true,
      excluded: true,
      permissionDenied: true,
      cancelled: true,
    }
    // `connected` says nothing more: the label already says "Connected to your server".
    const connections: Record<Exclude<ConnectOutcome, 'connected'>, true> = {
      alreadyConnected: true,
      stillReconnecting: true,
      needsCredentials: true,
      needsApproval: true,
      rejected: true,
      unreachable: true,
      notSaved: true,
      cancelled: true,
    }
    const reasons: Record<Exclude<RefusalReason['kind'], 'memory'>, RefusalReason> = {
      declined: { kind: 'declined' },
      protected: { kind: 'protected' },
      toolUnavailable: { kind: 'toolUnavailable' },
      inBackground: { kind: 'inBackground' },
      repeatedCall: { kind: 'repeatedCall' },
      problem: { kind: 'problem' },
    }
    const memory: Record<MemoryRefusalKind, true> = {
      outsideMemory: true,
      notMarkdown: true,
      noPath: true,
      directoryFull: true,
      noSuchFile: true,
      noMatch: true,
      notUnique: true,
      unwritable: true,
      unavailable: true,
      other: true,
    }
    const all: ToolOutcome[] = [
      ...Object.values(outcomes).flat(),
      ...Object.values(reasons).map((reason): ToolOutcome => ({ kind: 'refused', reason })),
      ...(Object.keys(memory) as MemoryRefusalKind[]).map((refusal): ToolOutcome => ({
        kind: 'refused',
        reason: { kind: 'memory', refusal },
      })),
      ...(Object.keys(misses) as MissToken[]).map((miss): ToolOutcome => ({ kind: 'locationMiss', miss })),
      ...(Object.keys(connections) as ConnectOutcome[]).map((outcome): ToolOutcome => ({
        kind: 'connection',
        outcome,
      })),
    ]
    const texts = all.map((outcome) => settled(outcome))
    for (const text of texts) expect(text?.length ?? 0).toBeGreaterThan(0)
    // Each refusal says which one it was.
    expect(new Set(texts).size).toBe(texts.length)
  })
})

describe('turnStatsText', () => {
  it('words what a turn spent: calls, time, and tokens', () => {
    expect(turnStatsText({ toolCalls: 3, wallMs: 4200, promptTokens: 1000, completionTokens: 234 })).toBe(
      '3 tool calls · 4.2s · 1,234 tokens',
    )
    expect(turnStatsText({ toolCalls: 1, wallMs: 800, promptTokens: 1, completionTokens: 0 })).toBe(
      '1 tool call · 800ms · 1 token',
    )
  })

  it('leaves the time out for a turn recorded before it was measured', () => {
    expect(turnStatsText({ toolCalls: 2, wallMs: null, promptTokens: 10, completionTokens: 5 })).toBe(
      '2 tool calls · 15 tokens',
    )
  })

  it('leads with "Stopped" for a turn the user stopped', () => {
    expect(turnStatsText({ toolCalls: 2, wallMs: 4200, promptTokens: 10, completionTokens: 5, stopped: true })).toBe(
      'Stopped · 2 tool calls · 4.2s · 15 tokens',
    )
    expect(turnStatsText({ toolCalls: 2, wallMs: 4200, promptTokens: 10, completionTokens: 5, stopped: false })).toBe(
      '2 tool calls · 4.2s · 15 tokens',
    )
  })

  it('says nothing for a turn that ran no tool, or has no stats', () => {
    expect(turnStatsText(null)).toBeNull()
    expect(turnStatsText({ toolCalls: 0, wallMs: 10, promptTokens: 1, completionTokens: 1 })).toBeNull()
  })
})
