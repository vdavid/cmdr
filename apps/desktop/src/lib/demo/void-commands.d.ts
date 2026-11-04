/**
 * Every command the generated bindings declare as answering nothing, by its snake_case wire name:
 * `scripts/vite-demo-void-commands.ts` derives it from `bindings.ts` at build time.
 */
declare module 'virtual:cmdr-demo-void-commands' {
  const voidCommands: ReadonlySet<string>
  export default voidCommands
}
