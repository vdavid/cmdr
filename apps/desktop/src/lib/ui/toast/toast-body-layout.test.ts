/**
 * Guards the toast body contract (`ui/DETAILS.md` § Toast system). `ToastItem` floats the age
 * label and close-button corner at the top right of its content box, and a body's text wraps
 * around that float only when two things hold:
 *
 * - The float comes BEFORE the body in the content box (a float only pushes aside what follows it).
 * - The body renders ONE root element, in block flow. A flex or grid root is its own formatting
 *   context, which a float can't reach into: the whole body would sit narrowed beside the corner,
 *   and nothing else would say so.
 *
 * Toast bodies are found by name (`*ToastContent.svelte`, `*ToastBody.svelte`), so a component
 * passed to `addToast` keeps that suffix.
 */

import { describe, it, expect, vi } from 'vitest'
import { mount, tick } from 'svelte'
import { globSync, readFileSync } from 'node:fs'
import { parse } from 'svelte/compiler'
import ToastItem from './ToastItem.svelte'

interface AstNode {
  type: string
  name?: string
  attributes?: { type: string; name?: string; value?: unknown }[]
  consequent?: { nodes: AstNode[] }
  alternate?: { nodes: AstNode[] } | null
  fragment?: { nodes: AstNode[] }
}

const bodyFiles = globSync(['src/**/*ToastContent.svelte', 'src/**/*ToastBody.svelte'])

/**
 * Every set of top-level elements the fragment can render at once, one set per mutually exclusive
 * `{#if}` path. Snippets, comments, and text don't count: they add no box of their own.
 */
function rootSets(nodes: AstNode[]): AstNode[][] {
  let sets: AstNode[][] = [[]]
  for (const node of nodes) {
    if (node.type === 'RegularElement' || node.type === 'Component' || node.type === 'EachBlock') {
      sets = sets.map((set) => [...set, node])
    } else if (node.type === 'IfBlock') {
      const branches = [
        ...rootSets(node.consequent?.nodes ?? []),
        ...(node.alternate ? rootSets(node.alternate.nodes) : [[]]),
      ]
      sets = sets.flatMap((set) => branches.map((branch) => [...set, ...branch]))
    } else if (node.type === 'KeyBlock') {
      const inner = rootSets(node.fragment?.nodes ?? [])
      sets = sets.flatMap((set) => inner.map((branch) => [...set, ...branch]))
    }
  }
  return sets
}

function staticClasses(element: AstNode): string[] {
  const classAttribute = element.attributes?.find((a) => a.type === 'Attribute' && a.name === 'class')
  if (!classAttribute || !Array.isArray(classAttribute.value)) return []
  return (classAttribute.value as { type: string; data?: string }[])
    .filter((part) => part.type === 'Text')
    .flatMap((part) => (part.data ?? '').split(/\s+/))
    .filter(Boolean)
}

/** The `display` a component's own `.name { … }` rule gives the class, if any. */
function declaredDisplay(source: string, className: string): string | null {
  const style = /<style>([\s\S]*?)<\/style>/.exec(source)?.[1] ?? ''
  const rule = new RegExp(`(?:^|\\n)\\s*\\.${className}\\s*\\{([^}]*)\\}`).exec(style)
  return rule ? (/display:\s*([a-z-]+)/.exec(rule[1])?.[1] ?? null) : null
}

describe('toast body layout contract', () => {
  it('renders the corner float before the body', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ToastItem, {
      target,
      props: {
        id: 'corner-order',
        content: 'Copied 12 items.',
        level: 'info',
        dismissal: 'persistent',
        timeoutMs: 0,
        postedAt: Date.now(),
        onTimeout: vi.fn(),
        onUserDismiss: vi.fn(),
      },
    })
    await tick()

    expect(target.querySelector('.toast-content')?.firstElementChild?.classList.contains('toast-corner')).toBe(true)
  })

  it('finds the toast bodies', () => {
    expect(bodyFiles.length).toBeGreaterThan(25)
  })

  it.each(bodyFiles)('%s renders one block-flow root', (file) => {
    const source = readFileSync(file, 'utf8')
    const ast = parse(source, { modern: true })
    for (const set of rootSets(ast.fragment.nodes)) {
      expect(
        set.length,
        `${file}: renders ${String(set.length)} root elements in one state, not one`,
      ).toBeLessThanOrEqual(1)
      const root = set.at(0)
      if (root === undefined || root.type !== 'RegularElement') continue
      for (const className of staticClasses(root)) {
        const display = declaredDisplay(source, className)
        expect(
          display === null || !/flex|grid/.test(display),
          `${file}: root .${className} is display: ${String(display)}, so its text can't wrap around the toast's corner`,
        ).toBe(true)
      }
    }
  })
})
