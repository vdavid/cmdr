# Testing rules

- ❌ When testing the running Tauri app, don't use a browser: use the MCP servers (see `docs/tooling/mcp.md`).
- ❌ No bare `await pollUntil(...)` (or other `Promise<boolean>` poll helper) in E2E: it returns `false` on timeout, so
  the test passes silently. Use `expect.poll(...).toBeTruthy()`, or `expect(await pollUntil(...)).toBe(true)`. Enforced
  by `bare-poll`.
- Coverage allowlist is a last resort: extract pure functions and test them, and only allowlist a genuinely untestable
  API, naming the specific one in the reason.
