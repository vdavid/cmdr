//! Local, on-disk logging of every LLM request and response.
//!
//! Answers David's three questions for both the agent and the legacy one-shot AI
//! features: what did we send, was the model set up to succeed, and what came back.
//! The tap lives inside [`crate::ai::client::AiBackend`]'s dispatch methods (the choke
//! point ALL LLM traffic flows through), so no call path can skip it: the agent's
//! streaming turns and the legacy prompt helpers (folder suggestions, translate) log
//! through the same seam.
//!
//! ## What gets written
//!
//! `{app data dir}/llm-logs/{session}/{NNN}_{request|response}_{slug}.json`, where `NNN`
//! is a zero-padded per-session counter reflecting call order, and `slug` is a
//! deterministic label built from the job kind plus a few sanitized words of the latest
//! user message (never model-generated). Each file is `{ "metadata": {...}, "body": {...} }`;
//! metadata borrows OpenTelemetry GenAI semantic-convention field names where they fit
//! (`gen_ai.request.model`-style) so external tooling can consume the files, with no OTel
//! dependency added.
//!
//! ## Capture fidelity
//!
//! v1 logs the serialized genai [`ChatRequest`] as the request body, marked
//! `fidelity: "request_struct"`. The byte-identical per-adapter wire payload lives behind
//! genai's `AdapterDispatcher::to_web_request_data`, whose types
//! (`AdapterDispatcher`/`WebRequestData`/`ServiceType`/`ChatOptionsSet`) are re-exported
//! `pub(crate)` in `genai =0.6.0-beta.19` and are therefore unreachable from Cmdr — see
//! `DETAILS.md`. The `ChatRequest` serialization still carries the full assembled prompt:
//! system, tools, history, and the context envelope. Responses log the assembled final
//! (streamed text plus stop reason and usage), marked `fidelity: "assembled"`, or the
//! parsed non-stream response, `fidelity: "parsed"`.
//!
//! ## Privacy
//!
//! These files contain everything the provider saw (names, paths, the app-state envelope) —
//! they are LOCAL ONLY and never transmitted. No API key material is ever written: the auth
//! header is applied by genai's web client downstream of the logged `ChatRequest`, and every
//! file is defensively passed through [`redact::redact_secrets`] before writing.
//!
//! ## Failure isolation
//!
//! A logging problem (disk full, unwritable dir) NEVER breaks or delays the LLM call. The
//! file write is offloaded to a detached thread; the counter and slug are assigned
//! synchronously so `NNN` reflects call order regardless of when the writes land, and any
//! write error is swallowed with a single `log::warn!`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex, OnceLock};

use serde_json::{Value, json};

use crate::ignore_poison::IgnorePoison;

mod redact;

#[cfg(test)]
mod tests;

const LOG_TARGET: &str = "ai::llm_log";

/// The `llm-logs` subdirectory name under the app data dir.
const LOG_DIR_NAME: &str = "llm-logs";

/// Whether logging is on. Defaults to the build mode: ON in dev (`debug_assertions`), OFF in
/// release, so a developer's runs are inspectable out of the box while shipped builds write
/// nothing until the user opts in. The frontend pushes the persisted setting over
/// `set_log_llm_calls` on startup and on every toggle, so this reflects the user's choice
/// once settings load; the build-mode default only governs the pre-settings window.
static ENABLED: AtomicBool = AtomicBool::new(cfg!(debug_assertions));

/// The resolved `llm-logs` directory, set once at app setup. Logging no-ops until it is set
/// (there is no app data dir to write into before then).
static LOG_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Per-session next-counter cache. Keyed by session directory so `NNN` is monotonic per
/// session and reflects call order. Seeded from the max existing `NNN_` prefix on first use
/// so numbering stays continuous across app restarts instead of clobbering old files.
static COUNTERS: LazyLock<Mutex<HashMap<PathBuf, u32>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Records the `llm-logs` directory (under `app_data_dir`). Call once at app setup.
pub fn init(app_data_dir: &Path) {
    let _ = LOG_DIR.set(app_data_dir.join(LOG_DIR_NAME));
}

/// Turns logging on or off at runtime (no restart). Driven by the `logLlmCalls` setting.
pub fn set_enabled(enabled: bool) {
    ENABLED.store(enabled, Ordering::Relaxed);
    log::debug!(target: LOG_TARGET, "LLM call logging {}", if enabled { "on" } else { "off" });
}

/// Whether logging is currently on.
pub fn is_enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// Which AI feature made the call. Drives the session directory (for the one-shot helpers)
/// and the slug prefix + `gen_ai.operation`-style metadata. A typed enum, never a matched
/// string (`no-string-matching`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobKind {
    /// An Ask Cmdr chat turn (grouped per conversation).
    AgentChat,
    /// Folder-name suggestions.
    FolderSuggestions,
    /// Natural-language search translation.
    TranslateSearch,
    /// "Ask about selection" translation.
    TranslateSelection,
}

impl JobKind {
    /// The slug prefix that leads every file name for this job.
    fn slug_prefix(self) -> &'static str {
        match self {
            JobKind::AgentChat => "agent-chat",
            JobKind::FolderSuggestions => "folder-suggestions",
            JobKind::TranslateSearch => "translate-search",
            JobKind::TranslateSelection => "translate-selection",
        }
    }

    /// The default session directory name for a job that has no per-conversation session
    /// (the one-shot helpers). The agent overrides this with its conversation id.
    fn default_session(self) -> &'static str {
        self.slug_prefix()
    }
}

/// Identifies where a call's logs go (session directory) and what job produced it.
#[derive(Debug, Clone)]
pub struct LlmLogContext {
    session: String,
    job: JobKind,
}

impl LlmLogContext {
    /// An Ask Cmdr chat turn, grouped under its conversation.
    pub fn agent_chat(conversation_id: i64) -> Self {
        Self {
            session: format!("thread-{conversation_id}"),
            job: JobKind::AgentChat,
        }
    }

    /// A folder-name-suggestions call.
    pub fn folder_suggestions() -> Self {
        Self::one_shot(JobKind::FolderSuggestions)
    }

    /// A natural-language-search translation call.
    pub fn translate_search() -> Self {
        Self::one_shot(JobKind::TranslateSearch)
    }

    /// An "ask about selection" translation call.
    pub fn translate_selection() -> Self {
        Self::one_shot(JobKind::TranslateSelection)
    }

    fn one_shot(job: JobKind) -> Self {
        Self {
            session: job.default_session().to_string(),
            job,
        }
    }
}

/// Where a captured request came from and how faithful the body is.
#[derive(Debug, Clone)]
pub struct RequestInfo {
    /// `gen_ai.system` — the provider, e.g. `"anthropic"` / `"openai"` / `"gemini"` / `"local"`.
    pub provider: String,
    /// `gen_ai.request.model` — the model name sent.
    pub model: String,
    /// `cmdr.adapter_kind` — the genai adapter the call routed through.
    pub adapter_kind: String,
    /// How faithful the logged request body is (v1: [`Fidelity::RequestStruct`]).
    pub fidelity: Fidelity,
    /// The latest user message, for the deterministic slug (never written verbatim into the
    /// file name; sanitized to a few words).
    pub user_message: String,
}

/// The outcome side of a call: token usage, why it stopped, how long it took.
#[derive(Debug, Clone, Default)]
pub struct ResponseInfo {
    /// `gen_ai.usage.input_tokens`, when the provider returned usage.
    pub prompt_tokens: Option<u32>,
    /// `gen_ai.usage.output_tokens`, when the provider returned usage.
    pub completion_tokens: Option<u32>,
    /// `gen_ai.response.finish_reasons` — the stop reason, when known.
    pub stop_reason: Option<String>,
    /// `cmdr.latency_ms` — wall time from request dispatch to the logged response.
    pub latency_ms: u64,
    /// How faithful the logged response body is.
    pub fidelity: Fidelity,
}

/// How closely a logged body matches the real wire bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Fidelity {
    /// The serialized genai `ChatRequest` — the full assembled prompt, but not the
    /// per-adapter wire payload (unreachable at the pinned genai; see the module docs).
    RequestStruct,
    /// The assembled streamed response (text + stop + usage).
    #[default]
    Assembled,
    /// The parsed non-stream response.
    Parsed,
}

impl Fidelity {
    fn as_str(self) -> &'static str {
        match self {
            Fidelity::RequestStruct => "request_struct",
            Fidelity::Assembled => "assembled",
            Fidelity::Parsed => "parsed",
        }
    }
}

/// A handle returned by [`log_request`] that ties the response file to the same session and
/// slug, so a request/response pair shares a base name and consecutive counters.
#[must_use = "call `log_response` to record the matching response"]
pub struct CallLog {
    session_dir: PathBuf,
    slug: String,
}

/// Records an outgoing request. Returns a [`CallLog`] to record the matching response, or
/// `None` when logging is off or no directory is configured (in which case the response side
/// is a no-op too). Never blocks the caller: the counter and slug are assigned synchronously
/// and the file write is offloaded to a detached thread.
pub fn log_request(ctx: &LlmLogContext, info: RequestInfo, body: Value) -> Option<CallLog> {
    if !is_enabled() {
        return None;
    }
    let root = LOG_DIR.get()?;
    Some(record_request(root, ctx, info, body))
}

/// The gate-free core of [`log_request`]: always records, under an explicit `root`
/// directory. Split out so the counter/slug/metadata/file behavior is testable without the
/// `LOG_DIR`/`ENABLED` globals.
fn record_request(root: &Path, ctx: &LlmLogContext, info: RequestInfo, body: Value) -> CallLog {
    let session_dir = root.join(sanitize_component(&ctx.session));
    let slug = build_slug(ctx.job, &info.user_message);

    let seq = next_seq(&session_dir);
    let metadata = json!({
        "gen_ai.operation.name": "chat",
        "gen_ai.system": info.provider,
        "gen_ai.request.model": info.model,
        "cmdr.adapter_kind": info.adapter_kind,
        "cmdr.job": ctx.job.slug_prefix(),
        "cmdr.session": ctx.session,
        "cmdr.seq": seq,
        "cmdr.direction": "request",
        "cmdr.fidelity": info.fidelity.as_str(),
        "cmdr.timestamp": now_iso8601(),
    });
    dispatch_write(session_dir.clone(), file_name(seq, "request", &slug), metadata, body);

    CallLog { session_dir, slug }
}

impl CallLog {
    /// Records the response for the request this handle came from. Never blocks the caller.
    pub fn log_response(&self, info: ResponseInfo, body: Value) {
        let seq = next_seq(&self.session_dir);
        let mut metadata = json!({
            "cmdr.job": "chat",
            "cmdr.direction": "response",
            "cmdr.seq": seq,
            "cmdr.fidelity": info.fidelity.as_str(),
            "cmdr.latency_ms": info.latency_ms,
            "cmdr.timestamp": now_iso8601(),
        });
        let obj = metadata.as_object_mut().expect("json! object is an object");
        if let Some(p) = info.prompt_tokens {
            obj.insert("gen_ai.usage.input_tokens".into(), json!(p));
        }
        if let Some(c) = info.completion_tokens {
            obj.insert("gen_ai.usage.output_tokens".into(), json!(c));
        }
        if let Some(reason) = info.stop_reason {
            obj.insert("gen_ai.response.finish_reasons".into(), json!([reason]));
        }
        dispatch_write(
            self.session_dir.clone(),
            file_name(seq, "response", &self.slug),
            metadata,
            body,
        );
    }
}

// region: --- Pure helpers (TDD core)

/// Assembles the on-disk file JSON: `{ "metadata": {...}, "body": {...} }`, with the whole
/// value passed through the secret redactor as a defense-in-depth belt (the `ChatRequest`
/// body carries no key, but a future wire-capture path would).
fn build_log_json(metadata: Value, body: Value) -> Value {
    let mut value = json!({ "metadata": metadata, "body": body });
    redact::redact_secrets(&mut value);
    value
}

/// `{NNN}_{direction}_{slug}.json`, `NNN` zero-padded to three digits (four+ when a session
/// somehow exceeds 999 calls — the width floor is three, never a truncation).
fn file_name(seq: u32, direction: &str, slug: &str) -> String {
    format!("{seq:03}_{direction}_{slug}.json")
}

/// A deterministic slug: the job prefix plus a few sanitized words of the latest user
/// message. Lowercased, non-alphanumeric runs collapsed to single dashes, capped to a few
/// words and a bounded length so it stays a tidy file-name fragment.
fn build_slug(job: JobKind, user_message: &str) -> String {
    const MAX_WORDS: usize = 6;
    const MAX_LEN: usize = 48;

    let words = sanitize_component(user_message);
    let trimmed = words
        .split('-')
        .filter(|w| !w.is_empty())
        .take(MAX_WORDS)
        .collect::<Vec<_>>()
        .join("-");

    let mut slug = job.slug_prefix().to_string();
    if !trimmed.is_empty() {
        slug.push('-');
        slug.push_str(&trimmed);
    }
    if slug.len() > MAX_LEN {
        slug.truncate(MAX_LEN);
        slug = slug.trim_end_matches('-').to_string();
    }
    slug
}

/// Lowercase, replace every run of non-alphanumeric characters with a single dash, and trim
/// leading/trailing dashes. Keeps ASCII alphanumerics only, so any user text (or session
/// label) becomes a safe path component with no separators, dots, or Unicode surprises.
fn sanitize_component(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut last_dash = true; // treat the start as a boundary so we never lead with a dash
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            out.extend(ch.to_lowercase());
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

/// The next per-session counter value. Monotonic per session directory; seeded from the max
/// existing `NNN_` prefix so numbering continues across restarts rather than overwriting.
fn next_seq(session_dir: &Path) -> u32 {
    let mut map = COUNTERS.lock_ignore_poison();
    let counter = map
        .entry(session_dir.to_path_buf())
        .or_insert_with(|| scan_max_seq(session_dir));
    *counter += 1;
    *counter
}

/// The highest `NNN` already used in `session_dir` (0 when the dir is absent or empty), so a
/// fresh process continues numbering instead of clobbering earlier files.
fn scan_max_seq(session_dir: &Path) -> u32 {
    let Ok(entries) = std::fs::read_dir(session_dir) else {
        return 0;
    };
    let mut max = 0;
    for entry in entries.flatten() {
        if let Some(name) = entry.file_name().to_str()
            && let Some(seq) = leading_seq(name)
        {
            max = max.max(seq);
        }
    }
    max
}

/// Parses the leading `NNN` of a log file name (`"007_request_x.json"` → `Some(7)`).
fn leading_seq(name: &str) -> Option<u32> {
    let digits: String = name.chars().take_while(char::is_ascii_digit).collect();
    if digits.is_empty() {
        return None;
    }
    digits.parse().ok()
}

fn now_iso8601() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

// endregion: --- Pure helpers

/// Offloads the actual file write to a detached thread so the calling (LLM) path is never
/// blocked by disk I/O. Ordering across files is already fixed by the synchronously assigned
/// `NNN`, so out-of-order write completion is harmless. Any error is swallowed with a single
/// warn — a logging problem must never surface to the caller.
fn dispatch_write(session_dir: PathBuf, file_name: String, metadata: Value, body: Value) {
    let json = build_log_json(metadata, body);
    std::thread::spawn(move || {
        if let Err(e) = write_log_file(&session_dir, &file_name, &json) {
            log::warn!(target: LOG_TARGET, "writing an LLM log file failed (continuing): {e}");
        }
    });
}

/// Creates the session directory if needed and writes one pretty-printed JSON file.
fn write_log_file(session_dir: &Path, file_name: &str, json: &Value) -> std::io::Result<()> {
    std::fs::create_dir_all(session_dir)?;
    let path = session_dir.join(file_name);
    let text = serde_json::to_string_pretty(json).unwrap_or_else(|_| "{}".to_string());
    std::fs::write(path, text)
}
