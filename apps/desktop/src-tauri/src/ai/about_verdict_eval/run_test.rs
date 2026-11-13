//! The eval's moving parts: the plan of calls, one verdict call through the shipped
//! prompt, and the records each stage writes.

use std::collections::HashSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use super::corpus_test::{EvalCard, Group};
use crate::about::verdict::prompt::{PromptVariant, user_message};
use crate::agent::llm::AgentLlm;
use crate::agent::llm::genai_impl::GenaiAgentLlm;
use crate::agent::llm::types::{AgentDelta, AgentLlmError, AgentPart, AgentStopReason, CallSettings, ProviderTag};
use crate::ai::client::AiBackend;

/// The languages a card marked `languages` also runs in: two the plan named, Swedish, and
/// one non-Latin script.
pub(super) const OTHER_LANGUAGES: [&str; 4] = ["hu", "de", "sv", "ja"];

/// A model class under test.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) enum Class {
    Haiku,
    Sonnet,
    Opus,
    /// The bundled local model, on a llama-server the eval starts the app's way.
    Local,
}

impl Class {
    pub(super) fn parse(word: &str) -> Option<Self> {
        match word.trim() {
            "haiku" => Some(Class::Haiku),
            "sonnet" => Some(Class::Sonnet),
            "opus" => Some(Class::Opus),
            "local" => Some(Class::Local),
            _ => None,
        }
    }

    pub(super) fn model(self) -> &'static str {
        match self {
            Class::Haiku => super::super::smoke_providers::ANTHROPIC.model,
            Class::Sonnet => super::super::smoke_providers::ANTHROPIC_SONNET_MODEL,
            Class::Opus => super::super::smoke_providers::ANTHROPIC_OPUS_MODEL,
            Class::Local => "local",
        }
    }

    pub(super) fn provider(self) -> ProviderTag {
        match self {
            Class::Local => ProviderTag::Local,
            Class::Haiku | Class::Sonnet | Class::Opus => ProviderTag::Anthropic,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) enum Variant {
    /// `about_verdict.md`: what it is, who owns it, what removing it would cost.
    With,
    /// `about_verdict_no_removal.md` (ships): what it is and who owns it; removal is
    /// Cmdr's row alone.
    Without,
}

impl Variant {
    pub(super) fn parse(word: &str) -> Option<Self> {
        match word.trim() {
            "with" => Some(Variant::With),
            "without" => Some(Variant::Without),
            _ => None,
        }
    }

    pub(super) fn prompt(self) -> PromptVariant {
        match self {
            Variant::With => PromptVariant::WithRemoval,
            Variant::Without => PromptVariant::WithoutRemoval,
        }
    }
}

/// One call to make.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Job {
    pub(super) card: String,
    pub(super) lang: String,
    pub(super) class: Class,
    pub(super) variant: Variant,
    /// Whether the card's look inside rode along. Off only in the usefulness arm.
    pub(super) peek: bool,
    pub(super) run: u32,
}

impl Job {
    pub(super) fn group_key(&self) -> String {
        format!(
            "{}|{}|{:?}|{:?}|{}",
            self.card, self.lang, self.class, self.variant, self.peek
        )
    }

    pub(super) fn key(&self) -> String {
        format!("{}|{}", self.group_key(), self.run)
    }
}

/// How many times a card runs in English, by group, and in each other language. A rare
/// green light is what a small sample misses, so the cards built to tempt one run most.
pub(super) fn runs_for(group: Group) -> u32 {
    match group {
        Group::Ordinary => 3,
        Group::Tempting | Group::Hostile => 10,
    }
}
pub(super) const RUNS_PER_OTHER_LANGUAGE: u32 = 3;
pub(super) const RUNS_WITHOUT_PEEK: u32 = 3;

/// Every call the corpus asks of `classes` and `variants`.
pub(super) fn plan_jobs(cards: &[EvalCard], classes: &[Class], variants: &[Variant]) -> Vec<Job> {
    let mut jobs = Vec::new();
    for card in cards {
        for &class in classes {
            for &variant in variants {
                let mut push = |lang: &str, peek: bool, runs: u32| {
                    for run in 1..=runs {
                        jobs.push(Job {
                            card: card.id.to_string(),
                            lang: lang.to_string(),
                            class,
                            variant,
                            peek,
                            run,
                        });
                    }
                };
                push("en", true, runs_for(card.group));
                if card.languages {
                    for lang in OTHER_LANGUAGES {
                        push(lang, true, RUNS_PER_OTHER_LANGUAGE);
                    }
                }
                // The usefulness arm: the same ordinary card without its look inside, so
                // whether the look earns its egress rests on a number.
                if card.peek.is_some() && card.group == Group::Ordinary {
                    push("en", false, RUNS_WITHOUT_PEEK);
                }
            }
        }
    }
    jobs
}

/// The exact user message the model reads for `job`: the shipped projection, the fence,
/// and the peek, through `prompt::user_message`.
pub(super) fn message_text(card: &EvalCard, job: &Job) -> String {
    let projection = crate::about::model_for_test(&card.card, &card.path);
    let peek = if job.peek { card.peek.as_ref() } else { None };
    let message = user_message(&projection, &card.still_gathering, peek, &job.lang);
    match &message.parts[0] {
        AgentPart::Text(text) => text.clone(),
        _ => panic!("the verdict's message is one text part"),
    }
}

/// What one call produced.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Generation {
    pub(super) job: Job,
    pub(super) group: Group,
    /// The corpus round the card was written in.
    #[serde(default)]
    pub(super) round: u8,
    pub(super) model: String,
    pub(super) text: String,
    pub(super) stop: Option<String>,
    pub(super) prompt_tokens: u32,
    pub(super) completion_tokens: u32,
    pub(super) cost_micros: i64,
    /// From the request to the first word, and to the end.
    pub(super) first_token_ms: Option<u64>,
    pub(super) done_ms: u64,
    /// The call failed for good (after retries): the provider's typed failure.
    pub(super) failure: Option<String>,
    pub(super) attempts: u32,
}

/// One model the eval talks to.
pub(super) struct Caller {
    pub(super) class: Class,
    pub(super) llm: Box<dyn AgentLlm>,
    pub(super) call: CallSettings,
}

impl Caller {
    pub(super) fn cloud(class: Class, key: &str, call: CallSettings) -> Self {
        let backend = AiBackend::remote(
            key.to_string(),
            super::super::smoke_providers::ANTHROPIC.base_url.to_string(),
            class.model().to_string(),
        );
        Self {
            class,
            llm: Box::new(GenaiAgentLlm::new(backend)),
            call,
        }
    }

    pub(super) fn local(port: u16, call: CallSettings) -> Self {
        Self {
            class: Class::Local,
            llm: Box::new(GenaiAgentLlm::new(AiBackend::local(port))),
            call,
        }
    }
}

/// Whether a failure is worth another try.
fn transient(error: &AgentLlmError) -> bool {
    matches!(
        error,
        AgentLlmError::RateLimited(_)
            | AgentLlmError::Overloaded { .. }
            | AgentLlmError::Unavailable
            | AgentLlmError::Timeout
            | AgentLlmError::Provider(_)
    )
}

/// One streamed call, timed. `Err` carries whether it may be retried.
pub(super) struct Streamed {
    pub(super) text: String,
    pub(super) stop: AgentStopReason,
    pub(super) usage: crate::agent::llm::types::AgentUsage,
    pub(super) first_token: Option<Duration>,
    pub(super) done: Duration,
}

pub(super) async fn stream_once(
    llm: &dyn AgentLlm,
    system: &str,
    user: &str,
    call: CallSettings,
) -> Result<Streamed, AgentLlmError> {
    let messages = [crate::agent::llm::types::AgentMessage {
        role: crate::agent::llm::types::AgentRole::User,
        parts: vec![AgentPart::Text(user.to_string())],
        at: 0,
    }];
    let started = Instant::now();
    let mut stream = llm
        .respond(system, &[], &messages, call, CancellationToken::new())
        .await?;
    let mut text = String::new();
    let mut first_token = None;
    while let Some(item) = stream.next().await {
        match item? {
            AgentDelta::Text(piece) => {
                if first_token.is_none() && !piece.is_empty() {
                    first_token = Some(started.elapsed());
                }
                text.push_str(&piece);
            }
            AgentDelta::End { stop, usage, .. } => {
                return Ok(Streamed {
                    text,
                    stop,
                    usage,
                    first_token,
                    done: started.elapsed(),
                });
            }
            AgentDelta::ReasoningTick | AgentDelta::KeepAlive | AgentDelta::ToolCallStarted { .. } => {}
        }
    }
    Err(AgentLlmError::Provider("the stream ended with no End".to_string()))
}

/// [`stream_once`] with retries on a transient failure.
pub(super) async fn stream_with_retries(
    llm: &dyn AgentLlm,
    system: &str,
    user: &str,
    call: CallSettings,
) -> (Result<Streamed, AgentLlmError>, u32) {
    let mut attempt = 0;
    loop {
        attempt += 1;
        match stream_once(llm, system, user, call).await {
            Err(error) if transient(&error) && attempt < 6 => {
                let wait = Duration::from_secs(3 * 2u64.pow(attempt - 1));
                log::info!(target: "verdict_eval", "retrying after {error}, attempt {attempt}");
                // allowed-test-sleep: backoff before retrying a real provider's rate limit or overload
                tokio::time::sleep(wait).await;
            }
            other => return (other, attempt),
        }
    }
}

/// Runs `job` against `caller`.
pub(super) async fn generate(caller: &Caller, card: &EvalCard, job: Job) -> Generation {
    let system = job.variant.prompt().system();
    let user = message_text(card, &job);
    let (result, attempts) = stream_with_retries(caller.llm.as_ref(), &system, &user, caller.call).await;
    let model = caller.class.model().to_string();
    match result {
        Ok(streamed) => {
            let priced = crate::agent::pricing::price_call(caller.class.provider(), &model, streamed.usage);
            Generation {
                group: card.group,
                round: card.round,
                model,
                text: streamed.text,
                stop: Some(format!("{:?}", streamed.stop)),
                prompt_tokens: streamed.usage.prompt_tokens,
                completion_tokens: streamed.usage.completion_tokens,
                cost_micros: priced.cost_micros,
                first_token_ms: streamed.first_token.map(|d| d.as_millis() as u64),
                done_ms: streamed.done.as_millis() as u64,
                failure: None,
                attempts,
                job,
            }
        }
        Err(error) => Generation {
            group: card.group,
            round: card.round,
            model,
            text: String::new(),
            stop: None,
            prompt_tokens: 0,
            completion_tokens: 0,
            cost_micros: 0,
            first_token_ms: None,
            done_ms: 0,
            failure: Some(format!("{error:?}")),
            attempts,
            job,
        },
    }
}

/// An append-only JSON-lines file, written one record at a time so an interrupted run
/// keeps what it finished and a rerun skips it.
pub(super) struct Lines {
    file: Mutex<std::fs::File>,
}

impl Lines {
    pub(super) fn open(path: &Path) -> Self {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .expect("open the results file");
        Self { file: Mutex::new(file) }
    }

    pub(super) fn push<T: Serialize>(&self, record: &T) {
        let line = serde_json::to_string(record).expect("a record serializes");
        let mut file = self
            .file
            .lock()
            .expect("results file lock poisoned: a writer panicked mid-line");
        writeln!(file, "{line}").expect("write a result");
    }
}

pub(super) fn read_lines<T: for<'de> Deserialize<'de>>(path: &Path) -> Vec<T> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("a results line parses"))
        .collect()
}

/// The generations already on disk that succeeded, by job key.
pub(super) fn done_keys(generations: &[Generation]) -> HashSet<String> {
    generations
        .iter()
        .filter(|g| g.failure.is_none())
        .map(|g| g.job.key())
        .collect()
}

/// The output folder: `CMDR_VERDICT_EVAL_OUT`, else one under the system temp folder.
pub(super) fn out_dir() -> PathBuf {
    let dir = std::env::var("CMDR_VERDICT_EVAL_OUT")
        .map(PathBuf::from)
        // allowed-fixed-temp-dir: the results must outlive the test process (they're read, resumed, and rerun into)
        .unwrap_or_else(|_| std::env::temp_dir().join("cmdr-about-verdict-eval"));
    std::fs::create_dir_all(&dir).expect("make the output folder");
    dir
}

/// A comma-separated env list, parsed, or `default`.
pub(super) fn env_list<T>(name: &str, parse: fn(&str) -> Option<T>, default: Vec<T>) -> Vec<T> {
    match std::env::var(name) {
        Ok(value) if !value.trim().is_empty() => value
            .split(',')
            .map(|word| parse(word).unwrap_or_else(|| panic!("{name}: unknown entry `{word}`")))
            .collect(),
        _ => default,
    }
}
