/** Each kind of question, said out loud: what a person reads before they answer. */

import { describe, expect, it } from 'vitest'
import type { AskFacts, IndexingAction } from '#lib/ipc/bindings.js'
import { askSentence } from './tool-ask-sentence'

function open(facts: Partial<Extract<AskFacts, { kind: 'openFile' }>>): AskFacts {
  return {
    kind: 'openFile',
    path: '/Users/me/run.command',
    shownPath: '/Users/me/run.command',
    name: 'run.command',
    isBundle: false,
    opener: 'Terminal',
    ...facts,
  }
}

describe('askSentence', () => {
  it('a `.command` file names the app that runs it, and where it is', () => {
    const sentence = askSentence(open({}))
    expect(sentence.body).toContain('run.command')
    expect(sentence.body).toContain('Terminal')
    expect(sentence.detail).toBe('/Users/me/run.command')
  })

  it('a file nothing claims says so, rather than leaving a blank', () => {
    const sentence = askSentence(open({ opener: null }))
    expect(sentence.body).toContain('run.command')
    expect(sentence.body).not.toContain('null')
    expect(sentence.body).toContain('can’t tell')
  })

  it('a bundle says it runs: nothing opens it', () => {
    const sentence = askSentence(open({ name: 'Thing.app', isBundle: true, opener: null }))
    expect(sentence.body).toContain('Thing.app')
    expect(sentence.body).toContain('runs')
  })

  it('a setting shows its label, its old value, and its new value', () => {
    const { body } = askSentence({
      kind: 'setSetting',
      id: 'behavior.textEditorApp',
      label: 'Text editor',
      oldValue: 'Zed',
      newValue: 'Vim',
    })
    expect(body).toContain('Text editor')
    expect(body).toContain('Zed')
    expect(body).toContain('Vim')
  })

  it('an empty value reads as nothing, so the sentence never says "from to"', () => {
    const { body } = askSentence({
      kind: 'setSetting',
      id: 'askCmdr.interactiveModel',
      label: 'Ask Cmdr model',
      oldValue: '',
      newValue: 'some-model',
    })
    expect(body).not.toContain('from  to')
    expect(body).toContain('from nothing to some-model')
  })

  it('quit says what happens to what is running', () => {
    expect(askSentence({ kind: 'quit' }).body).toContain('running')
  })

  it('eject names the volume, and says when it is the whole disk', () => {
    const one = askSentence({ kind: 'eject', volumeId: 'd', name: 'Backup', wholeDisk: false }).body
    const whole = askSentence({ kind: 'eject', volumeId: 'd', name: 'Backup', wholeDisk: true }).body
    expect(one).toContain('Backup')
    expect(whole).toContain('Backup')
    expect(whole).toContain('whole disk')
    expect(one).not.toContain('whole disk')
  })

  it('removing a server names it', () => {
    expect(askSentence({ kind: 'removeServer', hostId: 'h', name: 'Naspolya' }).body).toContain('Naspolya')
  })

  it.each<IndexingAction>(['enable', 'disable', 'rescan', 'forget'])('indexing `%s` names the volume', (action) => {
    const { title, body } = askSentence({ kind: 'indexing', action, volumeId: 'v', name: 'Backup' })
    expect(title.length).toBeGreaterThan(0)
    expect(body).toContain('Backup')
  })
})
