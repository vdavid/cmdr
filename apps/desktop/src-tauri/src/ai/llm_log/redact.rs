//! Defense-in-depth secret scrubbing for LLM log files.
//!
//! The v1 request body (a serialized genai `ChatRequest`) carries no API key — the auth
//! header is applied by genai's web client downstream of what we log. This pass is the belt
//! to that suspenders: it runs over EVERY log file before writing, so if a future
//! wire-capture path ever includes headers or a key-bearing URL (some providers put the key
//! in a `?key=` query param), no key material reaches disk. It also directly satisfies the
//! mandated redaction test: a `WebRequestData`-shaped value with an `Authorization` header
//! and a `?key=` URL comes out with no secret in it.
//!
//! Redaction is by KEY (never by value-substring guessing), so it can't over-redact ordinary
//! prose: a value is scrubbed only when its object key names a credential, or when it is a
//! URL string whose query carries a credential parameter.

use serde_json::Value;

const REDACTED: &str = "<redacted>";

/// Recursively replaces credential-bearing values with `<redacted>`, in place.
pub fn redact_secrets(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, val) in map.iter_mut() {
                if is_secret_key(key) {
                    *val = Value::String(REDACTED.to_string());
                } else if is_url_key(key)
                    && let Value::String(text) = val
                {
                    *text = scrub_url_query(text);
                } else {
                    redact_secrets(val);
                }
            }
        }
        Value::Array(items) => {
            for item in items.iter_mut() {
                redact_secrets(item);
            }
        }
        _ => {}
    }
}

/// Whether an object key names a credential (case- and separator-insensitive).
fn is_secret_key(key: &str) -> bool {
    matches!(
        normalize(key).as_str(),
        "authorization"
            | "proxyauthorization"
            | "apikey"
            | "xapikey"
            | "xgoogapikey"
            | "token"
            | "accesstoken"
            | "refreshtoken"
            | "secret"
            | "clientsecret"
            | "password"
    )
}

/// Whether an object key holds a URL whose query may carry a credential parameter.
fn is_url_key(key: &str) -> bool {
    matches!(normalize(key).as_str(), "url" | "uri" | "endpoint" | "baseurl")
}

/// Lowercase and drop non-alphanumerics, so `X-Api-Key`, `x_api_key`, and `xApiKey` all
/// collapse to `xapikey`.
fn normalize(key: &str) -> String {
    key.chars()
        .filter(char::is_ascii_alphanumeric)
        .flat_map(char::to_lowercase)
        .collect()
}

/// Redacts the value of any credential query parameter in a URL string, leaving the rest
/// intact. Non-URLs (no `?`) pass through unchanged.
fn scrub_url_query(url: &str) -> String {
    let Some((base, query)) = url.split_once('?') else {
        return url.to_string();
    };
    let scrubbed: Vec<String> = query
        .split('&')
        .map(|pair| match pair.split_once('=') {
            Some((name, _)) if is_secret_query_param(name) => format!("{name}={REDACTED}"),
            _ => pair.to_string(),
        })
        .collect();
    format!("{base}?{}", scrubbed.join("&"))
}

fn is_secret_query_param(name: &str) -> bool {
    matches!(normalize(name).as_str(), "key" | "token" | "apikey" | "accesstoken")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn redacts_authorization_header_and_url_key_from_web_request_shape() {
        // The mandated case: a WebRequestData-shaped value carrying the user's key in the
        // Authorization header AND in a `?key=` URL query. After redaction, no key material
        // survives anywhere in the value.
        let secret = "sk-live-SUPERSECRETKEY123";
        let mut value = json!({
            "url": format!("https://generativelanguage.googleapis.com/v1beta/models?key={secret}"),
            "headers": {
                "Authorization": format!("Bearer {secret}"),
                "x-api-key": secret,
                "content-type": "application/json"
            },
            "payload": { "model": "claude-x", "messages": [{ "role": "user", "content": "hi" }] }
        });

        redact_secrets(&mut value);

        let serialized = serde_json::to_string(&value).unwrap();
        assert!(
            !serialized.contains(secret),
            "no key material may remain after redaction: {serialized}"
        );
        // The Authorization + x-api-key values are gone.
        assert_eq!(value["headers"]["Authorization"], json!(REDACTED));
        assert_eq!(value["headers"]["x-api-key"], json!(REDACTED));
        // The URL keeps its shape but drops the key.
        assert_eq!(
            value["url"],
            json!("https://generativelanguage.googleapis.com/v1beta/models?key=<redacted>")
        );
        // Ordinary content is untouched.
        assert_eq!(value["headers"]["content-type"], json!("application/json"));
        assert_eq!(value["payload"]["messages"][0]["content"], json!("hi"));
    }

    #[test]
    fn leaves_ordinary_prose_untouched() {
        // A message that merely mentions "password" or "token" in prose must not be scrubbed —
        // redaction keys on the object key, not on value substrings.
        let mut value = json!({
            "messages": [
                { "role": "user", "content": "what's my authorization token policy?" },
                { "role": "assistant", "content": "Your password rotation is fine." }
            ]
        });
        let before = value.clone();
        redact_secrets(&mut value);
        assert_eq!(value, before, "prose that mentions credentials must survive verbatim");
    }

    #[test]
    fn redacts_nested_and_separator_variant_keys() {
        let mut value = json!({
            "auth": { "api_key": "secret1", "apiKey": "secret2" },
            "nested": [{ "clientSecret": "secret3" }]
        });
        redact_secrets(&mut value);
        assert_eq!(value["auth"]["api_key"], json!(REDACTED));
        assert_eq!(value["auth"]["apiKey"], json!(REDACTED));
        assert_eq!(value["nested"][0]["clientSecret"], json!(REDACTED));
    }
}
