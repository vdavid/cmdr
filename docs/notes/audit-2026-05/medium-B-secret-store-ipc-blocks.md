# Secret-store IPC calls run blocking Keychain work inline

**Severity:** medium **Lens:** B — Concurrency, races, and main-thread responsiveness **Confidence:** medium

## Location

`apps/desktop/src-tauri/src/ai/api_keys.rs:92-120` `apps/desktop/src-tauri/src/commands/network.rs:192-223`
`apps/desktop/src-tauri/src/secrets/keychain_macos.rs:41-56` `apps/desktop/src-tauri/src/network/keychain.rs:92-143`

## What

The AI API-key and SMB credential Tauri commands are synchronous command handlers that call the secret store directly.
On macOS, that reaches the Security framework Keychain calls inline; on other platforms the same abstraction can hit
Secret Service or a file-backed fallback. There is no `spawn_blocking`, timeout, cancellation, or degraded UI path
around these calls.

## Why it matters

If the macOS Keychain stalls, prompts, or blocks behind a locked keychain, a settings or network-connection action can
sit inside the IPC command path indefinitely. From the user's perspective, Cmdr can appear unresponsive exactly when
entering an API key or connecting to a share, and a blocked credential read can also delay recovery from SMB reconnect
flows.

## Evidence

AI key commands synchronously call the secret helpers:

```rust
92	#[tauri::command]
93	#[specta::specta]
94	pub fn save_ai_api_key(provider_id: String, api_key: String) -> Result<(), AiApiKeyError> {
95	    save(&provider_id, &api_key)
96	}
97
98	/// Returns the stored API key for the provider, or an empty string if none is stored.
99	/// Returning empty (rather than an error) on missing keys keeps the call sites simple: they all
100	/// pass the value through to `configure_ai`, which already treats empty-string as "not configured."
101	#[tauri::command]
102	#[specta::specta]
103	pub fn get_ai_api_key(provider_id: String) -> Result<String, AiApiKeyError> {
104	    match get(&provider_id) {
105	        Ok(key) => Ok(key),
106	        Err(AiApiKeyError::NotFound(_)) => Ok(String::new()),
107	        Err(e) => Err(e),
108	    }
```

SMB credential commands do the same:

```rust
192	#[tauri::command]
193	#[specta::specta]
194	pub fn save_smb_credentials(
195	    server: String,
196	    share: Option<String>,
197	    username: String,
198	    password: String,
199	) -> Result<(), KeychainError> {
200	    keychain::save_credentials(&server, share.as_deref(), &username, &password)
201	}
202
203	/// Retrieves SMB credentials from the Keychain.
204	/// Returns the stored username and password if found.
205	#[tauri::command]
206	#[specta::specta]
207	pub fn get_smb_credentials(server: String, share: Option<String>) -> Result<SmbCredentials, KeychainError> {
208	    keychain::get_credentials(&server, share.as_deref())
```

The macOS backend calls Keychain APIs directly:

```rust
41	impl SecretStore for KeychainStore {
42	    fn set(&self, key: &str, value: &[u8]) -> Result<(), SecretStoreError> {
43	        debug!("Keychain: setting secret for key: {}", key);
44	        set_generic_password(service_name(), key, value)
45	            .map_err(|e| SecretStoreError::Other(format!("Failed to save to Keychain: {}", e)))
46	    }
47
48	    fn get(&self, key: &str) -> Result<Vec<u8>, SecretStoreError> {
49	        debug!("Keychain: getting secret for key: {}", key);
50	        get_generic_password(service_name(), key).map_err(|e| classify_security_error(key, e))
51	    }
```

The SMB credential helper reaches the same store before updating the cache:

```rust
92	/// Saves SMB credentials to the secret store.
93	pub fn save_credentials(
94	    server: &str,
95	    share: Option<&str>,
96	    username: &str,
97	    password: &str,
98	) -> Result<(), KeychainError> {
99	    let account = make_account_name(server, share);
100	    let entry = make_password_entry(username, password);
101
102	    debug!("Saving credentials for account: {}", account);
103
104	    crate::secrets::store().set(&account, &entry)?;
```

## Suggested fix

Make secret-store command handlers async and run blocking secret-store backends inside `tokio::task::spawn_blocking`
with a small, explicit timeout and typed timeout error. Keep the credential cache fast path synchronous if desired, but
wrap cache misses and all writes/deletes. The frontend should treat timeout as a recoverable state with visible feedback
rather than waiting forever.

## Notes

This is a responsiveness finding, not a secret-leak finding. The inspected log calls include secret keys/account
identifiers but not the secret values themselves.
