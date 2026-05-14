# Move AI API keys out of `settings.json` into the OS secret store

## Context

Today, cloud AI provider API keys are persisted in plaintext inside the user's `settings.json` under
`ai.cloudProviderConfigs` (a JSON blob of `{apiKey, model, baseUrl?}` per provider). The file lives in the app's private
data dir, but it's still plaintext on disk; it ends up in Time Machine, cloud-sync, and any backup tool that mirrors
`~/Library/Application Support`. The settings/CLAUDE.md explicitly flags this as a "simpler first version, Keychain
integration can come later." This is that "later."

We already built a generic cross-platform secret store at `apps/desktop/src-tauri/src/secrets/`:

- macOS → Keychain via `security-framework`
- Linux + Secret Service → `keyring-core` + zbus
- Linux without Secret Service → `cocoon`-encrypted file (ChaCha20-Poly1305, key from `/etc/machine-id`)
- Dev (`CMDR_SECRET_STORE=file`) → plain JSON file

SMB credentials already flow through it (`network/keychain.rs`). We just need a second consumer for AI keys.

No migration: the app is pre-launch, so dropping any keys currently in `settings.json` is acceptable. Users re-enter
once.

## Approach

- Add a thin `ai/api_keys.rs` module that wraps `secrets::store()` with a per-provider key namespace.
- Expose 4 Tauri commands: `save_ai_api_key`, `get_ai_api_key`, `delete_ai_api_key`, `has_ai_api_key`.
- Stop writing the `apiKey` field into `ai.cloudProviderConfigs`. Keep `model` and `baseUrl` there.
- On the frontend, the API-key input reads/writes via the new commands instead of `setSetting`.
- `pushConfigToBackend` resolves `apiKey` from the secret store before calling `configureAi`.
- Old `apiKey` fields lingering in user's `settings.json` are simply ignored. No migration code.

### Key format

Per-provider entries in the store (one entry per provider, not one big blob):

```
ai.apiKey.openai
ai.apiKey.anthropic
ai.apiKey.google-gemini
…
```

On macOS, each shows up as a distinct entry in Keychain Access.app under service `Cmdr`, easy to inspect and revoke.

### macOS Keychain prompts

`set_generic_password` from `security-framework` creates the item owned by the calling app; subsequent reads from the
same codesigned app are silent (no prompt). Dev builds re-sign on every rebuild → different identity → re-prompts, which
is why `CMDR_SECRET_STORE=file` already exists. No additional ACL code needed. Same pattern as SMB credentials today.

### Linux UX

Inherits from the existing secret store. Already exposes `is_using_credential_file_fallback()` for the one-time toast on
Secret-Service-less systems. No new UX needed for the AI flow; the SMB-side toast already covers the same store.

## Files to change

### Rust

- **NEW** `apps/desktop/src-tauri/src/ai/api_keys.rs`: thin wrapper around `crate::secrets::store()`:
  - `save(provider_id: &str, key: &str)`, `get(provider_id: &str) -> Result<String, _>`, `delete(...)`,
    `has(...) -> bool`
  - Key format: `format!("ai.apiKey.{provider_id}")`
  - Reuse `SecretStoreError` mapped via a small local error enum (mirror of `network::keychain::KeychainError`)
- **EDIT** `apps/desktop/src-tauri/src/ai/mod.rs`: register the new submodule.
- **EDIT** `apps/desktop/src-tauri/src/commands/network.rs` or **NEW**
  `apps/desktop/src-tauri/src/commands/ai_api_keys.rs`: register the 4 Tauri commands. Prefer a new
  `commands/ai_api_keys.rs` file so it doesn't pollute the SMB-specific `commands/network.rs`. (Per
  `commands/CLAUDE.md`, AI commands typically register directly from `ai::manager`, but the established Keychain-command
  pattern lives in `commands/`. Mirroring `commands/network.rs`'s SMB Keychain section keeps the pattern consistent. The
  4 functions are non-generic so they go cleanly through specta.)
- **EDIT** `apps/desktop/src-tauri/src/ipc.rs`: add the 4 functions to both the specta `collect_functions![]` block
  (around line 360 area, in the new commands module section) and the runtime `tauri::generate_handler![]` block. Both
  the macOS+Linux path and the stubs path (other OSes) need a registration; secrets is currently file-backed for
  "other," which is fine, so stubs can just forward to the real impl. Skip the stubs split if the module is
  cross-platform (it is, since `secrets` works on all targets).
- **NEW** unit tests in `api_keys.rs`: round-trip via `CMDR_SECRET_STORE=file` (set in `#[test]` setup via a temp
  `CMDR_DATA_DIR`).

### Frontend (TypeScript / Svelte)

- **REGEN** `apps/desktop/src/lib/ipc/bindings.ts` via `cd apps/desktop && pnpm bindings:regen` (CI's `bindings-fresh`
  check enforces this).
- **EDIT** `apps/desktop/src/lib/settings/cloud-providers.ts`:
  - Drop `apiKey` from `CloudProviderConfig` (keep `model`, `baseUrl?`).
  - `resolveCloudConfig` no longer returns `apiKey`; callers fetch it separately.
  - `setProviderConfig` no longer accepts `apiKey`.
- **EDIT** `apps/desktop/src/lib/settings/sections/AiCloudSection.svelte`:
  - `loadCloudProviderConfig` reads the API key via `commands.getAiApiKey(providerId)` instead of the JSON blob.
  - The API-key `SettingPasswordInput`'s `onchange` calls `commands.saveAiApiKey(providerId, value)` and then
    `pushConfigToBackend()` so the backend sees the new key.
  - `saveCloudProviderField('apiKey', ...)` is removed; `'model'` and `'baseUrl'` paths stay.
  - Connection check still debounces on API-key change; wire from the same `onchange`.
- **EDIT** `apps/desktop/src/lib/settings/sections/ai-settings-utils.ts`:
  - `pushConfigToBackend` resolves the API key by calling `commands.getAiApiKey(getSetting('ai.cloudProvider'))` instead
    of pulling it out of `resolveCloudConfig`. Empty string if not found / errored.
- **EDIT** `apps/desktop/src/routes/(main)/+layout.svelte`:
  - Initial `configureAi(...)` call resolves the API key via `commands.getAiApiKey(...)` the same way.
- **EDIT** `apps/desktop/src/lib/settings/mcp-main-bridge.ts`:
  - Drop the special-case redaction of `ai.cloudProviderConfigs` apiKey field; the blob no longer contains secrets.
  - Keep the password-input redaction list for any other secret fields.
- **EDIT** `apps/desktop/src/lib/settings/cloud-providers.test.ts`: update tests to reflect the new
  `CloudProviderConfig` shape and `resolveCloudConfig` return shape.

### Docs

- **EDIT** `apps/desktop/src-tauri/src/ai/CLAUDE.md`: add an Architecture entry for `api_keys.rs`; update the startup
  flow snippet to show the apiKey lookup step.
- **EDIT** `apps/desktop/src/lib/settings/CLAUDE.md`: replace the "Why store OpenAI API key in `settings.json`, not
  keychain?" decision with a follow-up decision: "Moved to OS secret store via `crate::secrets`." Note that
  `ai.cloudProviderConfigs` no longer contains `apiKey`.
- **EDIT** `apps/desktop/src-tauri/src/secrets/CLAUDE.md`: add AI keys as a second consumer alongside SMB credentials.

## Files to leave alone

- `apps/desktop/src-tauri/src/secrets/*`: already does everything we need.
- `apps/desktop/src-tauri/src/network/keychain.rs`: SMB-specific, untouched.
- `ai.cloudProviderConfigs` setting itself stays in the registry (still holds `model` and `baseUrl`).

## Verification

1. `./scripts/check.sh`: full suite (Rust, Svelte, oxfmt, file-length, etc.).
2. New Rust unit tests in `api_keys.rs` pass round-trip via plain-file backend.
3. Manual:
   - `pnpm dev` → open Settings → AI → switch provider to OpenAI → paste a key → quit and relaunch → key is still there
     and AI suggestions still work (use MCP `cmdr` server to trigger a folder suggestion).
   - `settings.json` no longer contains `apiKey` strings after a save.
   - On macOS, `security find-generic-password -s Cmdr -a ai.apiKey.openai` returns the entry.
4. Bindings regen: `cd apps/desktop && pnpm bindings:regen` is a no-op on a clean tree after the regen step.

## Out of scope

- Migration of existing keys (deliberately skipped; app is pre-launch).
- Hardening the macOS Keychain ACL (default behavior already silent for same-app reads on a codesigned build).
- Any change to SMB credential storage.
- Removing the `cloud-providers.ts` `apiKey` field from any external JSON that might exist in user-exported settings
  (no such export exists).
