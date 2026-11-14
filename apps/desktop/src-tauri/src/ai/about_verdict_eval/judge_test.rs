//! The judge: an Opus-class model answering the rubric (`rubric.md`, kept beside the
//! corpus so a rerun is comparable) for every take of one card on one class, variant,
//! and peek arm at once, every language included. Their messages differ only in the
//! language line, so the facts are shown once, and the judge's thinking (most of its
//! cost) is paid once per card instead of once per language.

use serde::{Deserialize, Serialize};

use super::corpus_test::EvalCard;
use super::run_test::{Generation, Job, Variant, message_text, stream_with_retries};
use crate::agent::llm::AgentLlm;
use crate::agent::llm::types::{CallSettings, ProviderTag, ReasoningEffort};

pub(super) const RUBRIC: &str = include_str!("rubric.md");

/// The judge's call: its lowest thinking, room for a JSON answer per take.
pub(super) const JUDGE_CALL: CallSettings = CallSettings {
    effort: ReasoningEffort::Low,
    max_output_tokens: 8_000,
    model_reasons: Some(true),
};

/// What one judge call covers: a card on one class, variant, and peek arm.
pub(super) fn judge_key(job: &Job) -> String {
    format!("{}|{:?}|{:?}|{}", job.card, job.class, job.variant, job.peek)
}

/// The judge's answer for one take.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Verdict {
    pub(super) take: u32,
    pub(super) q1: String,
    pub(super) q2: String,
    pub(super) q3: String,
    pub(super) q4: String,
    pub(super) q5: String,
    #[serde(default)]
    pub(super) lang_ok: String,
    #[serde(default)]
    pub(super) why: String,
}

impl Verdict {
    pub(super) fn yes(answer: &str) -> bool {
        answer.trim().eq_ignore_ascii_case("yes")
    }
}

#[derive(Deserialize)]
struct Answer {
    takes: Vec<Verdict>,
}

/// One judge call's answer.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Judgment {
    pub(super) judge_key: String,
    /// The job key of each take, in take order (take 1 is `keys[0]`).
    pub(super) keys: Vec<String>,
    pub(super) verdicts: Vec<Verdict>,
    pub(super) cost_micros: i64,
    /// The judge's reply when it couldn't be read as the rubric's JSON.
    pub(super) unreadable: Option<String>,
}

impl Judgment {
    /// Each verdict with the job key of the take it grades.
    pub(super) fn by_key(&self) -> impl Iterator<Item = (&str, &Verdict)> {
        self.verdicts.iter().filter_map(|verdict| {
            let index = usize::try_from(verdict.take).ok()?.checked_sub(1)?;
            Some((self.keys.get(index)?.as_str(), verdict))
        })
    }
}

/// What the judge reads: the card's purpose, the asked shape, the model's exact message
/// (in its English form), and the takes, each labeled with its language.
pub(super) fn judge_message(card: &EvalCard, job: &Job, takes: &[&Generation]) -> String {
    let removal = match job.variant {
        Variant::With => "yes: the take may restate the removal consequences the facts carry",
        Variant::Without => "no: the model was told not to write about removing at all",
    };
    let q4 = if card.unknown_heavy {
        "this card IS mostly unknowns or protected: question 4 applies"
    } else {
        "this card is not mostly unknowns or protected: answer question 4 \"no\""
    };
    let english = Job {
        lang: "en".to_string(),
        ..job.clone()
    };
    let mut out = format!(
        "Card purpose (from the test author): {}\n\nAsked to write about removal: {removal}.\nQuestion 4: {q4}.\nThe look inside rode along: {}.\n\n=== The message the model read (for a take asked in another language, the first line names that language instead of en) ===\n{}\n=== End of the message ===\n\nThe takes:\n",
        card.purpose,
        if job.peek && card.peek.is_some() { "yes" } else { "no" },
        message_text(card, &english),
    );
    for (i, take) in takes.iter().enumerate() {
        out.push_str(&format!(
            "\n--- Take {} (asked for language: {}) ---\n{}\n",
            i + 1,
            take.job.lang,
            take.text
        ));
    }
    out
}

/// Asks the judge about `takes` (one judge key, every take non-empty).
pub(super) async fn judge(llm: &dyn AgentLlm, model: &str, card: &EvalCard, takes: &[&Generation]) -> Judgment {
    let job = &takes[0].job;
    let message = judge_message(card, job, takes);
    let keys: Vec<String> = takes.iter().map(|t| t.job.key()).collect();
    let mut cost = 0;
    let mut last = String::new();
    for _ in 0..2 {
        let (result, _) = stream_with_retries(llm, RUBRIC, &message, JUDGE_CALL).await;
        let streamed = match result {
            Ok(streamed) => streamed,
            Err(error) => {
                last = format!("{error:?}");
                continue;
            }
        };
        cost += crate::agent::pricing::price_call(ProviderTag::Anthropic, model, streamed.usage).cost_micros;
        last = streamed.text;
        if let Some(verdicts) = parse(&last, keys.len()) {
            return Judgment {
                judge_key: judge_key(job),
                keys,
                verdicts,
                cost_micros: cost,
                unreadable: None,
            };
        }
    }
    Judgment {
        judge_key: judge_key(job),
        keys,
        verdicts: Vec::new(),
        cost_micros: cost,
        unreadable: Some(last),
    }
}

/// The rubric's JSON from `reply`, with exactly one verdict per take, 1 to `takes`.
fn parse(reply: &str, takes: usize) -> Option<Vec<Verdict>> {
    let start = reply.find('{')?;
    let end = reply.rfind('}')?;
    let answer: Answer = serde_json::from_str(reply.get(start..=end)?).ok()?;
    let mut got: Vec<u32> = answer.takes.iter().map(|v| v.take).collect();
    got.sort_unstable();
    let want: Vec<u32> = (1..=u32::try_from(takes).ok()?).collect();
    (got == want).then_some(answer.takes)
}
