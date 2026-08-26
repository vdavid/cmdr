/**
 * ESLint rule: ban callback types whose positional parameters are confusable
 * with each other.
 *
 * Rationale: TypeScript lets a callback implementation declare FEWER
 * parameters than the type it's assigned to (that's what makes `arr.map(x =>
 * …)` compile). Given `listener: (id: string, value: string) => void`, a
 * caller writing `(value) => { email = value }` type-checks fine while
 * silently binding the id into what its author thought was the value.
 * Positional parameters of the same type are also trivially swappable at the
 * call site: `(volumeId: string, volumePath: string, targetPath: string) =>
 * void` has six orderings and TypeScript accepts all of them. Both shapes have
 * shipped as real bugs in this codebase.
 *
 * The fix wherever this rule fires: replace the positional parameters with a
 * single object payload, e.g. `(args: { volumeId: string; volumePath:
 * string; targetPath: string }) => void`. Dropping or reordering a field then
 * becomes a compile error, and call sites read self-documenting.
 *
 * ## What it flags
 *
 * A `TSFunctionType` node, considering only its non-rest parameters, and only
 * when there are two or more of them, when EITHER:
 *
 *   1. **Duplicate types**: two or more parameters have the same type
 *      annotation source text, compared after stripping all whitespace.
 *      Catches `(a: string, b: string)`, `(x: number, y?: number)`,
 *      `(a: Foo | null, b: Foo | null)`.
 *   2. **Unresolved generic**: a parameter's type annotation is a bare type
 *      reference (a plain identifier, no type arguments) whose name matches a
 *      type parameter declared on the `TSFunctionType` itself or on any
 *      enclosing node that declares type parameters. A generic `K` could be
 *      instantiated as the same type as its neighbour, so it's confusable
 *      with anything. This is what catches the original incident's signature,
 *      `(id: K, value: SettingsValues[K]) => void`: `id` and `value` have
 *      DIFFERENT type text, so clause 1 alone would miss it, but `K` is a
 *      bare reference to the enclosing function's own type parameter.
 *
 * Rest parameters are excluded from both the count and the comparison: they
 * can't be silently misbound. A parameter with no type annotation is skipped
 * (nothing to compare), not treated as a wildcard match.
 *
 * Out of scope on purpose: `TSMethodSignature` and `TSConstructorType` aren't
 * visited directly, though a `TSFunctionType` nested inside one (e.g. a
 * callback parameter of a generic method) is still checked, and still sees
 * that enclosing declaration's type parameters when resolving clause 2.
 *
 * Opt out per-line when the parameters really are fine as positional (a
 * fixed, well-known pair like `(width, height)` that's never reordered in
 * practice):
 *
 *   // eslint-disable-next-line cmdr/no-confusable-callback-params -- <reason>
 */

/** Non-rest params whose type annotation resolved to something comparable. */
function collectTypedParams(context, node) {
  const typed = []
  node.params.forEach((param, index) => {
    if (param.type === 'RestElement') return
    const typeNode = param.typeAnnotation?.typeAnnotation
    if (!typeNode) return
    typed.push({ index, param, typeNode, text: context.sourceCode.getText(typeNode).replace(/\s+/g, '') })
  })
  return typed
}

/** A human-readable name for a parameter, falling back to its position. */
function describeParam(param, index) {
  if (param.type === 'Identifier') return `\`${param.name}\``
  return `parameter ${index + 1}`
}

/**
 * Type-parameter names visible to `node`: its own `typeParameters` plus those
 * of every enclosing node that declares them (an enclosing function,
 * interface, type alias, class, or function type).
 */
function collectVisibleTypeParamNames(node) {
  const names = new Set()
  for (let current = node; current; current = current.parent) {
    for (const typeParam of current.typeParameters?.params ?? []) {
      if (typeParam.type === 'TSTypeParameter' && typeParam.name?.type === 'Identifier') {
        names.add(typeParam.name.name)
      }
    }
  }
  return names
}

/** Is `typeNode` a bare type reference (`K`, `SettingsValues`) with no type arguments? */
function isBareTypeReference(typeNode) {
  return typeNode.type === 'TSTypeReference' && typeNode.typeName.type === 'Identifier' && !typeNode.typeArguments
}

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: {
      description:
        'Forbid callback types whose positional parameters are confusable (same type, or an unresolved generic); use a single object payload instead.',
      recommended: true,
    },
    messages: {
      confusableParams:
        'This callback type has confusable parameters: {{ reasons }}. A caller could drop, swap, or ' +
        'misbind them without a compile error. Replace the positional parameters with a single object payload ' +
        '(e.g. `(args: { name: Type; … }) => void`), so a missing or reordered field fails to compile.',
    },
    schema: [],
  },
  create(context) {
    return {
      TSFunctionType(node) {
        const nonRestCount = node.params.filter((param) => param.type !== 'RestElement').length
        if (nonRestCount < 2) return

        const typedParams = collectTypedParams(context, node)

        // Clause 1: two or more params share the same (whitespace-stripped) type text.
        const byText = new Map()
        for (const entry of typedParams) {
          if (!byText.has(entry.text)) byText.set(entry.text, [])
          byText.get(entry.text).push(entry)
        }
        const duplicateGroups = [...byText.values()].filter((group) => group.length >= 2)

        // Clause 2: a bare type reference to a type parameter in scope.
        const visibleTypeParamNames = collectVisibleTypeParamNames(node)
        const genericParams = typedParams.filter(
          (entry) => isBareTypeReference(entry.typeNode) && visibleTypeParamNames.has(entry.typeNode.typeName.name),
        )

        if (duplicateGroups.length === 0 && genericParams.length === 0) return

        const reasons = []
        for (const group of duplicateGroups) {
          const names = group.map((entry) => describeParam(entry.param, entry.index))
          reasons.push(`${names.join(' and ')} share the type \`${group[0].text}\``)
        }
        for (const entry of genericParams) {
          reasons.push(
            `${describeParam(entry.param, entry.index)}'s type \`${entry.typeNode.typeName.name}\` could resolve to the same type as a sibling parameter`,
          )
        }

        context.report({ node, messageId: 'confusableParams', data: { reasons: reasons.join('; ') } })
      },
    }
  },
}
