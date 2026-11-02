/**
 * Fixtures for `tool-ask` (`#lib/tool-ask/ToolAskDialog.svelte`): one state per kind of question, plus
 * a value long enough to prove the buttons stay on screen.
 */

import type { AskFacts, Initiator } from '#lib/ipc/bindings.js'

export interface ToolAskFixture {
  facts: AskFacts
  startedBy: Initiator
}

const EXAMPLE_PATH = '/Users/you/Downloads/install.command'

export const toolAskFixtures: Record<string, ToolAskFixture | undefined> = {
  openFile: {
    startedBy: 'agent',
    facts: {
      kind: 'openFile',
      path: EXAMPLE_PATH,
      shownPath: EXAMPLE_PATH,
      name: 'install.command',
      isBundle: false,
      opener: 'Terminal',
    },
  },
  openFileUnknownApp: {
    startedBy: 'aiClient',
    facts: {
      kind: 'openFile',
      path: '/Users/you/Downloads/mystery',
      shownPath: '/Users/you/Downloads/mystery',
      name: 'mystery',
      isBundle: false,
      opener: null,
    },
  },
  openBundle: {
    startedBy: 'agent',
    facts: {
      kind: 'openFile',
      path: '/Applications/Thing.app',
      shownPath: '/Applications/Thing.app',
      name: 'Thing.app',
      isBundle: true,
      opener: null,
    },
  },
  setSetting: {
    startedBy: 'agent',
    facts: {
      kind: 'setSetting',
      id: 'fileOperations.defaultConflictPolicy',
      label: 'When a file already exists',
      oldValue: 'stop',
      newValue: 'overwrite',
    },
  },
  setSettingLongValue: {
    startedBy: 'aiClient',
    facts: {
      kind: 'setSetting',
      id: 'behavior.textEditorApp',
      label: 'Text editor',
      oldValue: 'Zed',
      newValue: `${'a-very-long-value-'.repeat(11)}…${'that-keeps-going-'.repeat(11)}`,
    },
  },
  quit: { startedBy: 'agent', facts: { kind: 'quit' } },
  eject: { startedBy: 'agent', facts: { kind: 'eject', volumeId: 'disk2s1', name: 'Backup', wholeDisk: false } },
  ejectWholeDisk: {
    startedBy: 'agent',
    facts: { kind: 'eject', volumeId: 'disk2s1', name: 'Backup', wholeDisk: true },
  },
  removeServer: { startedBy: 'agent', facts: { kind: 'removeServer', hostId: 'nas', name: 'Home NAS' } },
  indexingEnable: { startedBy: 'agent', facts: { kind: 'indexing', action: 'enable', volumeId: 'v', name: 'Backup' } },
  indexingDisable: {
    startedBy: 'agent',
    facts: { kind: 'indexing', action: 'disable', volumeId: 'v', name: 'Backup' },
  },
  indexingRescan: { startedBy: 'agent', facts: { kind: 'indexing', action: 'rescan', volumeId: 'v', name: 'Backup' } },
  indexingForget: { startedBy: 'agent', facts: { kind: 'indexing', action: 'forget', volumeId: 'v', name: 'Backup' } },
}
