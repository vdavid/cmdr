//! The one place every real-API smoke test gets its provider endpoint and model id.
//!
//! Providers decommission models on their own schedule, so these ids go stale without any
//! change on our side. Keeping them here means a decommission is a **one-line fix in this
//! file**, not a hunt across a test, a doc comment, and a unit-test assertion. The
//! `<provider>-smoke` check lanes exist to notice that day; [`expect_ok`] makes the failure
//! say which constant to edit.
//!
//! ## Refreshing a model id
//!
//! 1. Ask the provider what it actually serves today (`model_list_url` below); never pick
//!    from memory, which is how the previous pin rotted.
//! 2. Cross-check the provider's deprecation page: a *preview* model buys you weeks, a
//!    *production* one buys you months.
//! 3. Prefer the smallest production chat model. These run nightly against real billing.
//! 4. Re-anchor the `(verified …)` note on the constant you touched.

use std::fs::OpenOptions;
use std::io::Write;

use super::client::AiError;

/// Endpoint + model identity for one provider's real-API smoke.
pub(super) struct SmokeProvider {
    /// Human name, for failure messages.
    pub name: &'static str,
    /// Env var the check lane fills from the sops `secret` helper.
    pub env_var: &'static str,
    /// Base URL. Trailing slash required (see the `genai` `Url::join` gotcha in `client.rs`).
    pub base_url: &'static str,
    /// The model the smoke calls.
    pub model: &'static str,
    /// Where to look up what this provider serves today, quoted in the failure message.
    pub model_list_url: &'static str,
    /// This constant's own name, so a failure can name the line to edit.
    pub const_name: &'static str,
    /// The `pnpm check <nickname>` lane that runs this provider's smoke.
    pub check_nickname: &'static str,
}

/// Groq. OpenAI-compatible, free tier, and the cheapest always-available real-provider gate.
///
/// `openai/gpt-oss-20b` is Groq's smallest production chat model and its own documented
/// replacement for the retired `llama-3.1-8b-instant`. It's a reasoning model (every Groq
/// production chat model is, since the Llama line went), so the smoke leans on
/// `chat_completion_with_empty_retry` to survive a draw where reasoning eats the budget.
///
/// (verified on 2026-09-04: present and `active` in `GET /openai/v1/models`; listed as
/// Production on console.groq.com/docs/models. `llama-3.1-8b-instant` and
/// `llama-3.3-70b-versatile` were shut down 2026-08-16 per console.groq.com/docs/deprecations,
/// which names this model as the 8B replacement.)
pub(super) const GROQ: SmokeProvider = SmokeProvider {
    name: "Groq",
    env_var: "GROQ_API_KEY",
    base_url: "https://api.groq.com/openai/v1/",
    model: "openai/gpt-oss-20b",
    model_list_url: "https://api.groq.com/openai/v1/models",
    const_name: "GROQ",
    check_nickname: "groq-smoke",
};

/// Fireworks AI. A second OpenAI-compatible host, and the one whose model ids carry an
/// account path (`accounts/fireworks/models/…`) — a shape that must survive
/// `remote_model_iden`'s `openai::` namespacing intact.
///
/// (verified on 2026-09-04: present in `GET /inference/v1/models` and answers a 200-token
/// chat completion with non-empty text.)
pub(super) const FIREWORKS: SmokeProvider = SmokeProvider {
    name: "Fireworks AI",
    env_var: "FIREWORKS_AI_API_KEY",
    base_url: "https://api.fireworks.ai/inference/v1/",
    model: "accounts/fireworks/models/glm-5p3-flash",
    model_list_url: "https://api.fireworks.ai/inference/v1/models",
    const_name: "FIREWORKS",
    check_nickname: "fireworks-smoke",
};

/// Anthropic. The native (non-OpenAI) streaming protocol: event names like
/// `content_block_delta` rather than `data:` JSON envelopes. Without this lane we'd only ever
/// exercise the OpenAI lineage despite shipping Anthropic support.
///
/// (verified on 2026-09-04: `claude-haiku-4-5-20251001` is Active in `GET /v1/models` and on
/// platform.claude.com/docs/en/about-claude/model-deprecations, which gives it a tentative
/// retirement "not sooner than 2026-10-15" — the nearest horizon of any Active model, so
/// expect to refresh this one first. It replaced `claude-3-5-haiku-*`, retired 2026-02-19.)
///
/// **Gotcha**: Anthropic returns HTTP 400 for a request carrying both `temperature` and
/// `top_p`, and that is not limited to the newest models. Verified live on 2026-09-04:
/// `claude-haiku-4-5-20251001` and `claude-sonnet-4-5-20250929` both 400 with the pair and
/// both 200 with temperature alone. `adjust_for_model` drops `top_p` for the Anthropic
/// adapter, so any model here is safe; don't reintroduce `top_p` on this path.
pub(super) const ANTHROPIC: SmokeProvider = SmokeProvider {
    name: "Anthropic",
    env_var: "ANTHROPIC_API_KEY",
    base_url: "https://api.anthropic.com/v1/",
    model: "claude-haiku-4-5-20251001",
    model_list_url: "https://api.anthropic.com/v1/models",
    const_name: "ANTHROPIC",
    check_nickname: "anthropic-smoke",
};

/// Google Gemini. The SECOND native (non-OpenAI) protocol we ship: `remote_model_iden` leaves
/// `gemini-*` un-namespaced, so `genai` picks its Gemini adapter, which POSTs
/// `{base_url}models/{model}:generateContent` (and `:streamGenerateContent?alt=sse`) instead of
/// `/chat/completions`. Nothing else in the suite touches that path.
///
/// **Generation lives on `v1`, listing on either.** `genai 0.6.5`'s own default endpoint is
/// `v1beta/`, and a `gemini-3.x` id there doesn't route at all — so this base URL is
/// load-bearing rather than cosmetic, and a wrong one looks exactly like an outage (see
/// `client_real_gemini_test.rs`).
///
/// `gemini-3.5-flash-lite` is the smallest current non-preview model, and Google's own named
/// replacement for the withdrawn `gemini-2.5-flash-lite`. ❌ Never repoint this at
/// `gemini-flash-latest` / `gemini-flash-lite-latest`: a moving alias changes what it serves
/// underneath us, which is the exact change this lane exists to notice.
///
/// (verified on 2026-09-04: present in `GET /v1/models` with `generateContent` among its
/// `supportedGenerationMethods`, and answers a 300-token request with visible text.
/// `gemini-2.5-flash-lite` answers 404 with "no longer available to new users", naming this
/// model as its replacement.)
pub(super) const GEMINI: SmokeProvider = SmokeProvider {
    name: "Google Gemini",
    env_var: "GEMINI_API_KEY",
    base_url: "https://generativelanguage.googleapis.com/v1/",
    model: "gemini-3.5-flash-lite",
    model_list_url: "https://generativelanguage.googleapis.com/v1/models",
    const_name: "GEMINI",
    check_nickname: "gemini-smoke",
};

/// OpenAI, plain chat-completions leg. `gpt-4.1-mini` is also the app's shipped OpenAI
/// default (`cloud-providers.ts`), so this pins what users actually hit.
///
/// The other two OpenAI models below are not interchangeable with this one: each exists to
/// exercise a different branch of `adjust_for_model`. Read their docs before swapping any.
///
/// (verified on 2026-09-04: present in `GET /v1/models`, no entry on
/// developers.openai.com/api/docs/deprecations.)
pub(super) const OPENAI: SmokeProvider = SmokeProvider {
    name: "OpenAI",
    env_var: "OPENAI_API_KEY",
    base_url: "https://api.openai.com/v1/",
    model: "gpt-4.1-mini",
    model_list_url: "https://api.openai.com/v1/models",
    const_name: "OPENAI",
    check_nickname: "openai-smoke",
};

/// OpenAI model that must route through `/v1/responses` instead of chat-completions.
///
/// The replacement has to keep the `gpt-5` prefix: that's what `genai 0.6.5` matches on to
/// pick `AdapterKind::OpenAIResp` (`adapter_kind.rs`), and routing it anywhere else silently
/// guts `smoke_openai_responses_api_routing`.
///
/// (verified on 2026-09-04: `gpt-5.4-mini` present in `GET /v1/models`, no deprecation entry.
/// The older `gpt-5-mini` still resolves, but its only snapshot retires 2026-12-11.)
pub(super) const OPENAI_RESPONSES_MODEL: &str = "gpt-5.4-mini";

/// OpenAI model that stays on `/v1/chat/completions` yet rejects a custom `temperature` —
/// the case `is_openai_chat_reasoning_model` exists to catch. Needs an `o1`/`o3`/`o4`/
/// `chatgpt-` prefix; a `gpt-5*` id would route to Responses and test the wrong branch.
///
/// (verified on 2026-09-04: present in `GET /v1/models`. **Retires 2026-10-23** together with
/// `o4-mini`, per developers.openai.com/api/docs/deprecations — OpenAI's named replacement is
/// `gpt-5.6-terra`, a Responses model. When this lane goes red on that date there is no
/// o-series successor left: repoint it at a live `chatgpt-*` id, or retire the test and the
/// `o1`/`o3`/`o4` arms of `is_openai_chat_reasoning_model` with it.)
pub(super) const OPENAI_CHAT_REASONING_MODEL: &str = "o3-mini";

/// Reads the provider's key from the environment.
///
/// The check lane only starts the test process once it has resolved a key, so a missing one
/// here means someone ran `cargo nextest` by hand: panic with the recipe rather than pass
/// vacuously.
pub(super) fn api_key(provider: &SmokeProvider) -> String {
    match std::env::var(provider.env_var) {
        Ok(key) if !key.trim().is_empty() => key,
        _ => panic!(
            "{env} is not set. Run this through the check lane, which resolves the key from env or sops:\n\
             \x20 pnpm check {nickname}\n\
             or set it yourself: {env}=$(secret {env}) cargo nextest run --lib --run-ignored only ai::",
            env = provider.env_var,
            nickname = provider.check_nickname,
        ),
    }
}

/// Unwraps a smoke call, turning a failure into a message that names the fix.
///
/// A decommissioned model is the failure this whole family of tests exists to catch, and it
/// arrives as HTTP 404. We branch on the typed [`AiError::NotFound`] variant (classified from
/// the numeric status in `client.rs`), never on the provider's prose — wording differs per
/// provider and changes without notice.
///
/// 404 is honestly ambiguous: it's also what a wrong base-URL path returns (the `genai`
/// trailing-slash gotcha), so the message names both suspects.
#[track_caller]
pub(super) fn expect_ok<T>(provider: &SmokeProvider, model: &str, result: Result<T, AiError>) -> T {
    match result {
        Ok(value) => value,
        Err(AiError::NotFound(detail)) => panic!(
            "{name} no longer serves `{model}`.\n\
             \n\
             HTTP 404 means the model was decommissioned, or `{const_name}.base_url` has the wrong path.\n\
             Fix: pick a live production model from {list} and update the `{const_name}` block in\n\
             apps/desktop/src-tauri/src/ai/smoke_providers.rs (that file's header has the full recipe).\n\
             \n\
             {name} said: {detail}",
            name = provider.name,
            const_name = provider.const_name,
            list = provider.model_list_url,
        ),
        Err(err) => panic!(
            "{name} smoke call against `{model}` failed: {err}\n\
             \n\
             If the status is 404 the model is gone: pick a replacement from {list} and update the\n\
             `{const_name}` block in apps/desktop/src-tauri/src/ai/smoke_providers.rs.\n\
             Any other status is {name}'s side (billing, quota, an outage) rather than a stale pin.",
            name = provider.name,
            const_name = provider.const_name,
            list = provider.model_list_url,
        ),
    }
}

/// Env var naming the file a smoke writes an "inconclusive" report into. The check lane
/// creates the path, passes it here, and turns a non-empty file into a WARN
/// (`scripts/check/checks/desktop-rust-provider-smoke.go`).
pub(super) const STATUS_FILE_ENV: &str = "CMDR_SMOKE_STATUS_FILE";

/// Records that a smoke reached NO verdict — it never got a usable answer, and it never got
/// the provider to say the model is gone either — then returns so the test can finish green.
///
/// Why a third outcome exists at all: Gemini's free tier answered the identical request with
/// 200, then 503, then a bodyless 404, inside a few minutes (observed 2026-09-04). Failing on
/// that teaches everyone to ignore the nightly, and passing quietly is how the Groq lane sat
/// green for months without calling Groq. The lane's WARN is the middle: yellow, printed even
/// in quiet mode, and impossible to mistake for coverage.
///
/// It travels through a FILE because nextest discards a passing test's stdout
/// (`success-output = "never"` in `.config/nextest.toml`), so a `println!` would vanish
/// exactly when we need it. Appending keeps two concurrent test processes (nextest forks one
/// per test) from truncating each other's line.
///
/// ❗ With no status file set, this PANICS instead of returning. That's the hand-run case
/// (`cargo nextest run …` straight), where there's nothing to read the file back, and a
/// vacuous pass would be the same silent-skip trap in a new costume.
pub(super) fn report_inconclusive(provider: &SmokeProvider, reason: &str) {
    let line = format!("{}: {reason}\n", provider.name);
    let path = match std::env::var(STATUS_FILE_ENV) {
        Ok(path) if !path.trim().is_empty() => path,
        _ => panic!(
            "The {name} smoke reached no verdict, and {env} isn't set to report it through.\n\
             \n\
             {line}\n\
             This is NOT a stale model pin: {name} never answered clearly enough to say either way.\n\
             Run it through the check lane, which collects this as a warning:\n\
             \x20 pnpm check {nickname}",
            name = provider.name,
            env = STATUS_FILE_ENV,
            nickname = provider.check_nickname,
        ),
    };

    let written = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .and_then(|mut file| file.write_all(line.as_bytes()));
    if let Err(err) = written {
        panic!(
            "couldn't record the inconclusive {} smoke to {path}: {err}\n{line}",
            provider.name
        );
    }
}
