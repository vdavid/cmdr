# Cloud-AI base URL is unrestricted; API key can be sent over plaintext HTTP

**Severity:** medium
**Lens:** F — Security
**Confidence:** high

## Location

- `apps/desktop/src-tauri/src/ai/manager.rs` (`configure_ai`, `check_ai_connection` around lines 463–490, 616–625)
- `apps/desktop/src-tauri/src/ai/client.rs` (`AiBackend::remote`, `build_client`)
- `apps/desktop/src/lib/settings/cloud-providers.ts` (Ollama / LM Studio defaults at `http://localhost:11434/v1`, `http://localhost:1234/v1`)

## What

`configure_ai(cloud_base_url: String, ...)` accepts any string. No scheme check, no host check, no warning when the URL is `http://` but the API key is non-empty. The same URL is passed to the `genai` client, which will happily POST the API key in an `Authorization: Bearer` header over plaintext HTTP.

The defaults for Ollama / LM Studio are `http://localhost:*` (legitimate — local loopback, no key needed). But the same input field accepts `http://203.0.113.5:8080` and silently sends the BYOK OpenAI key there.

## Why it matters

A user who copies an OpenAI-compatible endpoint from a tutorial / phishing page (e.g., "Save money: route through our free proxy at `http://api-free.tld/v1`") loses their cloud API key the first time they trigger a folder suggestion. Cloud API keys are valuable: $20–$200 of credits, plus they often grant the attacker billable usage that hits the user later.

This isn't a Cmdr-specific bug class (every BYOK chat client has it) but the fix is cheap and the impact is real.

## Evidence

```rust
// manager.rs:463
pub async fn configure_ai(
    provider: String,
    context_size: u32,
    cloud_api_key: String,
    cloud_base_url: String,
    cloud_model: String,
) -> Result<(), String> {
    log::info!(
        "AI configure: provider={provider}, context_size={context_size}, base_url={cloud_base_url}, model={cloud_model}"
    );
    ...
    m.cloud_base_url = cloud_base_url;  // no validation
```

`check_ai_connection` (line 620) also takes raw `base_url` and `api_key` and constructs the request without scheme/host checks.

## Suggested fix

1. **Backend `configure_ai`**: parse the URL, require `https://` UNLESS host is `localhost` / `127.0.0.1` / `::1` (the local-LLM presets). On `http://` to a non-loopback host with a non-empty API key, refuse with a typed error variant the FE renders as "We only send your API key over HTTPS. Use `https://` or remove the key first."
2. **Frontend**: in the settings UI for "Custom OpenAI-compatible endpoint," surface a small inline warning the moment the user types `http://` + non-loopback host while the key field is non-empty. Cheaper UX than backend rejection alone.
3. The log line at `manager.rs:467` prints `base_url=` — fine, it's not a secret. Make sure no future log line includes the api_key alongside the base_url; right now it doesn't.

## Notes

- The defaults `http://localhost:11434/v1` (Ollama) and `http://localhost:1234/v1` (LM Studio) are correct as-is; the validation above carves them out via loopback hosts.
- `genai` itself doesn't warn on plaintext-HTTP-with-bearer-token (verified at version `=0.6.0-beta.19`).
- Same advice applies to the AI streaming path — same client, same URL.
