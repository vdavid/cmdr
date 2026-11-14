//! The verdict end to end over a recorded channel, with the fake LLM and a counting
//! reader: what goes out, in what order, and what every refusal, stop, and reuse does.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use cmdr_about::actions::ActionCaps;
use cmdr_about::card::AboutCard;
use cmdr_about::fact::UnknownWhy;
use cmdr_about::text::ItemText;
use futures_util::future::BoxFuture;
use tokio_util::sync::CancellationToken;

use super::gate::Ready;
use super::limits::{Limits, MAX_STARTS};
use super::prompt::{FACTS_BEGIN, FACTS_END, PromptVariant};
use super::*;
use crate::about::event::Derived;
use crate::about::jobs::Sources;
use crate::about::probe::probe;
use crate::about::route::local_gathered;
use crate::about::session::{AboutSink, Gathered};
use crate::about::tests::Recorder;
use crate::agent::llm::fake::{FakeAgentLlm, Pace, ScriptedTurn};
use crate::agent::llm::types::{AgentMessage, AgentPart, AgentUsage, CallSettings, ProviderTag, ToolDeclaration};
use crate::agent::llm::{AgentDeltaStream, AgentLlm};
use crate::agent::pricing::PricedCost;
use crate::test_support::{TestDir, wait_until_async};
use crate::tools::read::inspect::{FileRow, TextAsk};

const LONG: Duration = Duration::from_secs(10);
const VERDICT: [&str; 3] = ["A plain text file. ", "Cmdr can't tell ", "what depends on it."];

/// Every prompt a fake was handed: its system string and its messages.
type Prompts = Arc<std::sync::Mutex<Vec<(String, Vec<AgentMessage>)>>>;

/// An LLM that counts its calls and keeps every prompt, over a fake.
struct Counting {
    fake: FakeAgentLlm,
    calls: Arc<AtomicUsize>,
    prompts: Prompts,
}

impl AgentLlm for Counting {
    fn respond<'a>(
        &'a self,
        system: &'a str,
        tools: &'a [ToolDeclaration],
        messages: &'a [AgentMessage],
        call: CallSettings,
        cancel: CancellationToken,
    ) -> BoxFuture<'a, Result<AgentDeltaStream, AgentLlmError>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.prompts
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push((system.to_string(), messages.to_vec()));
        assert!(tools.is_empty(), "the verdict calls no tools");
        self.fake.respond(system, tools, messages, call, cancel)
    }
}

/// What a test can see of one verdict run.
struct World {
    calls: Arc<AtomicUsize>,
    prompts: Prompts,
    reads: Arc<AtomicUsize>,
    metered: Arc<AtomicUsize>,
}

impl World {
    fn new() -> Self {
        Self {
            calls: Arc::new(AtomicUsize::new(0)),
            prompts: Arc::default(),
            reads: Arc::new(AtomicUsize::new(0)),
            metered: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn ready(&self, fake: FakeAgentLlm) -> Ready {
        let metered = Arc::clone(&self.metered);
        Ready {
            llm: Box::new(Counting {
                fake,
                calls: Arc::clone(&self.calls),
                prompts: Arc::clone(&self.prompts),
            }),
            provider: ProviderTag::Anthropic,
            model: "claude-haiku-5-5".to_string(),
            call: CallSettings::default(),
            variant: PromptVariant::WithRemoval,
            meter: Arc::new(move |usage: AgentUsage| {
                metered.fetch_add(1, Ordering::SeqCst);
                crate::agent::pricing::price_call(ProviderTag::Anthropic, "claude-haiku-5-5", usage)
            }),
        }
    }

    /// The real `inspect_file` core, counted.
    fn reader(&self) -> ReadFn {
        let reads = Arc::clone(&self.reads);
        Arc::new(
            move |path: &str, ask: &TextAsk, cancel: &std::sync::atomic::AtomicBool| -> FileRow {
                reads.fetch_add(1, Ordering::SeqCst);
                crate::tools::read::inspect::inspect_path(path, ask, cancel)
            },
        )
    }
}

fn says(chunks: &[&str]) -> FakeAgentLlm {
    let usage = AgentUsage {
        prompt_tokens: 900,
        completion_tokens: 40,
        ..AgentUsage::default()
    };
    FakeAgentLlm::script_with_usage(vec![(
        ScriptedTurn::Say(chunks.iter().map(|chunk| (*chunk).to_string()).collect()),
        usage,
    )])
}

/// A local file's card, planned as the route plans it.
struct Card {
    _dir: TestDir,
    path: String,
    gathered: Gathered,
}

fn text_file(name: &str, text: &str) -> Card {
    let dir = TestDir::new("about-verdict");
    let file = dir.as_ref().join(name);
    std::fs::write(&file, text).expect("write the fixture");
    let path = file.to_string_lossy().into_owned();
    let probed = probe(&path).expect("the fixture is there");
    let gathered = local_gathered(
        &path,
        probed,
        ItemText::new("Macintosh HD"),
        crate::about::platform::os_caps(),
        Sources::none(),
    );
    Card {
        _dir: dir,
        path,
        gathered,
    }
}

fn derived(card: &AboutCard) -> Derived {
    Derived {
        headline: cmdr_about::headline::headline(card),
        removal: cmdr_about::removal::consequences(card),
        actions: cmdr_about::actions::actions(card, &ActionCaps::none()),
    }
}

fn verdict_for<'a>(card: &Card, token: &CancellationToken, limits: &'a Limits, world: &World) -> VerdictRun<'a> {
    VerdictRun {
        request_id: "card-1".to_string(),
        volume_id: "root".to_string(),
        path: card.path.clone(),
        gate_reach: GateReach::FollowLocalLinks,
        item: card.gathered.item,
        card: token.clone(),
        locale: "en".to_string(),
        limits,
        read: world.reader(),
        facts_cap: LONG,
    }
}

/// Plays the card's half by hand: `planned`, then (when `settle`) every row and
/// `factsSettled`. Answers the plan's card as it stands.
fn play_facts(watched: &Watched, card: &Card, settle: bool) {
    let plan = card.gathered.plan.clone();
    let mut shown = AboutCard::new(&plan);
    AboutSink::send(
        watched,
        AboutUpdate::Planned {
            plan,
            derived: derived(&shown),
        },
    );
    if !settle {
        return;
    }
    for fact in card.gathered.ready.clone() {
        let _ = shown.apply(fact.clone());
        AboutSink::send(
            watched,
            AboutUpdate::Fact {
                fact,
                derived: derived(&shown),
            },
        );
    }
    for fact in shown.settle_pending(UnknownWhy::NoRecord) {
        AboutSink::send(
            watched,
            AboutUpdate::Fact {
                fact,
                derived: derived(&shown),
            },
        );
    }
    AboutSink::send(watched, AboutUpdate::FactsSettled);
}

fn recorded() -> (Arc<Recorder>, Watched) {
    let recorder = Arc::new(Recorder::default());
    let sink: Arc<dyn AboutSink> = Arc::clone(&recorder) as Arc<dyn AboutSink>;
    (recorder, Watched::new(sink))
}

fn position(updates: &[AboutUpdate], wanted: impl Fn(&AboutUpdate) -> bool) -> usize {
    updates
        .iter()
        .position(wanted)
        .unwrap_or_else(|| panic!("not sent: {updates:?}"))
}

/// An update as the wire carries it, for comparing (the wire types aren't `PartialEq`).
fn wire(update: &AboutUpdate) -> serde_json::Value {
    serde_json::to_value(update).expect("an update serializes")
}

fn sent(updates: &[AboutUpdate], wanted: &AboutUpdate) -> bool {
    let wanted = wire(wanted);
    updates.iter().any(|update| wire(update) == wanted)
}

fn verdict_text(updates: &[AboutUpdate]) -> String {
    updates
        .iter()
        .filter_map(|update| match update {
            AboutUpdate::VerdictDelta { text } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

/// The whole path: the verdict plans right after the card, says what it was given
/// before the first word, streams, ends with its cost, and the prompt carries the
/// projection and the first lines inside ONE fence.
#[tokio::test]
async fn a_verdict_streams_after_the_plan_with_its_given_line_first() {
    let card = text_file("notes.txt", "Shopping list\nmilk\neggs\n");
    let (recorder, watched) = recorded();
    let world = World::new();
    let limits = Limits::default();
    let token = CancellationToken::new();
    let verdict = verdict_for(&card, &token, &limits, &world);

    let facts = async { play_facts(&watched, &card, true) };
    tokio::join!(facts, run(verdict, Ok(world.ready(says(&VERDICT))), &watched));

    let updates = recorder.updates();
    assert!(
        matches!(updates[0], AboutUpdate::Planned { .. }),
        "the plan is first: {updates:?}"
    );
    let planned = position(&updates, |u| matches!(u, AboutUpdate::VerdictPlanned));
    let given = position(&updates, |u| matches!(u, AboutUpdate::VerdictGiven { .. }));
    let first_delta = position(&updates, |u| matches!(u, AboutUpdate::VerdictDelta { .. }));
    let settled = position(&updates, |u| matches!(u, AboutUpdate::FactsSettled));
    assert!(planned < given && given < first_delta, "{updates:?}");
    assert!(settled < given, "the call waits for the facts: {updates:?}");
    assert!(matches!(
        updates[given],
        AboutUpdate::VerdictGiven {
            contents: VerdictContents::FirstLines
        }
    ));
    assert_eq!(verdict_text(&updates), VERDICT.concat());
    let Some(AboutUpdate::VerdictEnd {
        ending: VerdictEnding::Done,
        cost: Some(cost),
    }) = updates.last()
    else {
        panic!("a finished verdict ends with its cost: {updates:?}");
    };
    assert_eq!((cost.prompt_tokens, cost.completion_tokens), (900, 40));
    assert!(cost.priced && cost.cost_micros > 0);
    assert_eq!(world.metered.load(Ordering::SeqCst), 1);
    assert_eq!(world.reads.load(Ordering::SeqCst), 1, "one look inside");

    let prompts = world.prompts.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let (system, messages) = &prompts[0];
    assert_eq!(*system, PromptVariant::WithRemoval.system());
    let AgentPart::Text(text) = &messages[0].parts[0] else {
        panic!("one text part");
    };
    assert_eq!(text.matches(FACTS_BEGIN).count(), 1);
    assert_eq!(text.matches(FACTS_END).count(), 1);
    let inside = &text[text.find(FACTS_BEGIN).expect("fence")..];
    assert!(
        inside.contains("notes.txt") && inside.contains("Shopping list"),
        "{text}"
    );
}

/// ❗ Every refusal of the gate reads no file for a model, calls no model, and meters
/// nothing: the card says why, right after its plan, and nothing else about a verdict.
#[tokio::test]
async fn each_refusal_reads_nothing_and_calls_nothing() {
    use crate::managed_policy::ManagedAiRefusal;
    let refusals = [
        VerdictUnavailable::AiOff,
        VerdictUnavailable::TurnedOff,
        VerdictUnavailable::LocalOff,
        VerdictUnavailable::NoCloudConsent,
        VerdictUnavailable::NotConfigured,
        VerdictUnavailable::NotReady,
        VerdictUnavailable::Managed {
            refusal: ManagedAiRefusal::AiOff,
        },
    ];
    for why in refusals {
        let card = text_file("notes.txt", "secret plans\n");
        let (recorder, watched) = recorded();
        let world = World::new();
        let limits = Limits::default();
        let token = CancellationToken::new();
        let verdict = verdict_for(&card, &token, &limits, &world);
        let facts = async { play_facts(&watched, &card, true) };
        tokio::join!(facts, run(verdict, Err(why), &watched));

        let updates = recorder.updates();
        let verdicts: Vec<serde_json::Value> = updates
            .iter()
            .filter(|u| {
                !matches!(
                    u,
                    AboutUpdate::Planned { .. } | AboutUpdate::Fact { .. } | AboutUpdate::FactsSettled
                )
            })
            .map(wire)
            .collect();
        assert_eq!(
            verdicts,
            vec![wire(&AboutUpdate::VerdictUnavailable { why })],
            "{why:?}: {updates:?}"
        );
        assert_eq!(world.reads.load(Ordering::SeqCst), 0, "{why:?} read the file");
        assert_eq!(world.calls.load(Ordering::SeqCst), 0, "{why:?} called a model");
        assert_eq!(world.metered.load(Ordering::SeqCst), 0);
    }
}

/// The slot's answer is on the channel before the facts settle: the card knows from its
/// first frame whether to reserve the block.
#[tokio::test]
async fn the_slots_answer_arrives_before_the_facts_settle() {
    let card = text_file("notes.txt", "x\n");
    let (recorder, watched) = recorded();
    let world = World::new();
    let limits = Limits::default();
    let token = CancellationToken::new();
    let verdict = verdict_for(&card, &token, &limits, &world);
    let check = async {
        play_facts(&watched, &card, false);
        wait_until_async(LONG, "the verdict planned", || {
            recorder
                .updates()
                .iter()
                .any(|u| matches!(u, AboutUpdate::VerdictPlanned))
        })
        .await;
        assert!(
            !recorder
                .updates()
                .iter()
                .any(|u| matches!(u, AboutUpdate::FactsSettled)),
            "planned while the facts were still out"
        );
        assert_eq!(world.calls.load(Ordering::SeqCst), 0, "no call before the facts");
        token.cancel();
    };
    tokio::join!(check, run(verdict, Ok(world.ready(says(&VERDICT))), &watched));
}

/// A slow row can't hold the verdict past its cap: it goes with what's in and names
/// the rows still out.
#[tokio::test]
async fn the_verdict_goes_at_its_cap_with_the_rows_still_out_named() {
    let card = text_file("notes.txt", "x\n");
    let (recorder, watched) = recorded();
    let world = World::new();
    let limits = Limits::default();
    let token = CancellationToken::new();
    let verdict = VerdictRun {
        facts_cap: Duration::from_millis(30),
        ..verdict_for(&card, &token, &limits, &world)
    };
    let facts = async { play_facts(&watched, &card, false) };
    tokio::join!(facts, run(verdict, Ok(world.ready(says(&VERDICT))), &watched));

    assert!(matches!(
        recorder.updates().last(),
        Some(AboutUpdate::VerdictEnd {
            ending: VerdictEnding::Done,
            ..
        })
    ));
    let prompts = world.prompts.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let AgentPart::Text(text) = &prompts[0].1[0].parts[0] else {
        panic!("one text part");
    };
    assert!(text.contains("still being gathered"), "{text}");
    assert!(text.contains("\"stillGathering\""), "{text}");
}

/// Stop ends the verdict with the text so far and a `stopped` ending; the card stays
/// open. A consent revoke stops it the same way (`stop_all`, what
/// `stop_in_flight_calls` calls).
#[tokio::test]
async fn stop_and_a_revoke_end_the_verdict_and_keep_the_card() {
    for by_revoke in [false, true] {
        let card = text_file("notes.txt", "x\n");
        let (recorder, watched) = recorded();
        let world = World::new();
        let limits = Limits::default();
        let token = CancellationToken::new();
        let verdict = verdict_for(&card, &token, &limits, &world);
        let held = says(&VERDICT).paced(Pace {
            per_delta: Duration::from_millis(1),
            hold_before_last: Duration::from_secs(60),
        });
        let stop = async {
            play_facts(&watched, &card, true);
            // Every delta but the held last one.
            wait_until_async(LONG, "the verdict holds before its last words", || {
                verdict_text(&recorder.updates()) == format!("{}{}", VERDICT[0], VERDICT[1])
            })
            .await;
            if by_revoke {
                assert_eq!(limits.stop_all(), 1);
            } else {
                assert!(limits.stop("card-1"));
            }
        };
        tokio::join!(stop, run(verdict, Ok(world.ready(held)), &watched));

        let updates = recorder.updates();
        assert_eq!(
            updates.last().map(wire),
            Some(wire(&AboutUpdate::VerdictEnd {
                ending: VerdictEnding::Stopped,
                cost: None
            })),
            "{updates:?}"
        );
        assert!(!token.is_cancelled(), "the card stays open");
        assert_eq!(verdict_text(&updates), format!("{}{}", VERDICT[0], VERDICT[1]));
        assert_eq!(
            world.metered.load(Ordering::SeqCst),
            0,
            "a stopped call reports no usage"
        );
    }
}

/// Closing the card stops the verdict and nothing more goes out, not even its end.
#[tokio::test]
async fn closing_the_card_stops_the_verdict_silently() {
    let card = text_file("notes.txt", "x\n");
    let (recorder, watched) = recorded();
    let world = World::new();
    let limits = Limits::default();
    let token = CancellationToken::new();
    let verdict = verdict_for(&card, &token, &limits, &world);
    let held = says(&VERDICT).paced(Pace {
        per_delta: Duration::from_millis(1),
        hold_before_last: Duration::from_secs(60),
    });
    let close = async {
        play_facts(&watched, &card, true);
        wait_until_async(LONG, "the first words are out", || {
            recorder
                .updates()
                .iter()
                .any(|u| matches!(u, AboutUpdate::VerdictDelta { .. }))
        })
        .await;
        token.cancel();
    };
    tokio::join!(close, run(verdict, Ok(world.ready(held)), &watched));
    assert!(
        !recorder
            .updates()
            .iter()
            .any(|u| matches!(u, AboutUpdate::VerdictEnd { .. })),
        "a closed card hears nothing more"
    );
}

/// A reopen on the same node reuses the finished verdict from memory: one call, and
/// the second card gets the text whole, marked from earlier.
#[tokio::test]
async fn a_reopen_on_the_same_node_reuses_the_verdict() {
    let card = text_file("notes.txt", "x\n");
    let world = World::new();
    let limits = Limits::default();
    for _ in 0..2 {
        let (recorder, watched) = recorded();
        let token = CancellationToken::new();
        let verdict = verdict_for(&card, &token, &limits, &world);
        let facts = async { play_facts(&watched, &card, true) };
        tokio::join!(facts, run(verdict, Ok(world.ready(says(&VERDICT))), &watched));
        let _ = recorder;
    }
    assert_eq!(world.calls.load(Ordering::SeqCst), 1, "the reopen made no call");

    let (recorder, watched) = recorded();
    let token = CancellationToken::new();
    let verdict = verdict_for(&card, &token, &limits, &world);
    let facts = async { play_facts(&watched, &card, true) };
    tokio::join!(facts, run(verdict, Ok(world.ready(says(&VERDICT))), &watched));
    let updates = recorder.updates();
    assert!(sent(
        &updates,
        &AboutUpdate::VerdictFromEarlier {
            contents: VerdictContents::FirstLines,
            text: VERDICT.concat(),
        }
    ));
    assert_eq!(world.reads.load(Ordering::SeqCst), 1, "the reuse looked inside nothing");
}

/// The 13th start inside a minute is refused, typed, before anything is read.
#[tokio::test]
async fn past_the_start_ceiling_the_card_says_try_again() {
    let card = text_file("notes.txt", "x\n");
    let world = World::new();
    let limits = Limits::default();
    let now = std::time::Instant::now();
    for _ in 0..MAX_STARTS {
        limits.admit(now).expect("inside the ceiling");
    }
    let (recorder, watched) = recorded();
    let token = CancellationToken::new();
    let verdict = verdict_for(&card, &token, &limits, &world);
    let facts = async { play_facts(&watched, &card, true) };
    tokio::join!(facts, run(verdict, Ok(world.ready(says(&VERDICT))), &watched));
    assert!(sent(
        &recorder.updates(),
        &AboutUpdate::VerdictUnavailable {
            why: VerdictUnavailable::TooManyRequests
        }
    ));
    assert_eq!(world.reads.load(Ordering::SeqCst), 0);
    assert_eq!(world.calls.load(Ordering::SeqCst), 0);
}

/// A protected item is never read: the verdict is told it didn't look inside, and its
/// facts go out name-level only.
#[tokio::test]
async fn a_protected_item_goes_out_name_level_with_nothing_read() {
    let dir = TestDir::new("about-verdict-ssh");
    let ssh = dir.as_ref().join(".ssh");
    std::fs::create_dir(&ssh).expect("make the folder");
    let key = ssh.join("id_ed25519");
    std::fs::write(&key, "-----BEGIN OPENSSH PRIVATE KEY-----\nAAAA\n").expect("write the key");
    let path = key.to_string_lossy().into_owned();
    let probed = probe(&path).expect("there");
    let card = Card {
        gathered: local_gathered(
            &path,
            probed,
            ItemText::new("Macintosh HD"),
            crate::about::platform::os_caps(),
            Sources::none(),
        ),
        path,
        _dir: dir,
    };
    let (recorder, watched) = recorded();
    let world = World::new();
    let limits = Limits::default();
    let token = CancellationToken::new();
    let verdict = verdict_for(&card, &token, &limits, &world);
    let facts = async { play_facts(&watched, &card, true) };
    tokio::join!(facts, run(verdict, Ok(world.ready(says(&VERDICT))), &watched));

    assert!(sent(
        &recorder.updates(),
        &AboutUpdate::VerdictGiven {
            contents: VerdictContents::Protected
        }
    ));
    assert_eq!(world.reads.load(Ordering::SeqCst), 0);
    let prompts = world.prompts.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let AgentPart::Text(text) = &prompts[0].1[0].parts[0] else {
        panic!("one text part");
    };
    assert!(text.contains("\"contentsStayHome\": \"credentials\""), "{text}");
    assert!(!text.contains("OPENSSH"), "{text}");
}

/// A folder's verdict looks inside nothing: its digest is the look.
#[tokio::test]
async fn a_folder_is_never_read() {
    let dir = TestDir::new("about-verdict-folder");
    let path = dir.as_ref().to_string_lossy().into_owned();
    let probed = probe(&path).expect("there");
    let card = Card {
        gathered: local_gathered(
            &path,
            probed,
            ItemText::new("Macintosh HD"),
            crate::about::platform::os_caps(),
            Sources::none(),
        ),
        path,
        _dir: dir,
    };
    assert!(!card.gathered.item.bytes_readable);
    let (recorder, watched) = recorded();
    let world = World::new();
    let limits = Limits::default();
    let token = CancellationToken::new();
    let verdict = verdict_for(&card, &token, &limits, &world);
    let facts = async { play_facts(&watched, &card, true) };
    tokio::join!(facts, run(verdict, Ok(world.ready(says(&VERDICT))), &watched));
    assert!(sent(
        &recorder.updates(),
        &AboutUpdate::VerdictGiven {
            contents: VerdictContents::None
        }
    ));
    assert_eq!(world.reads.load(Ordering::SeqCst), 0);
}

/// A provider failure ends the verdict typed, and nothing is metered.
#[tokio::test]
async fn a_provider_failure_ends_typed() {
    let card = text_file("notes.txt", "x\n");
    let (recorder, watched) = recorded();
    let world = World::new();
    let limits = Limits::default();
    let token = CancellationToken::new();
    let verdict = verdict_for(&card, &token, &limits, &world);
    let failing = FakeAgentLlm::script(vec![ScriptedTurn::Fail(AgentLlmError::RateLimited(
        "slow down".to_string(),
    ))]);
    let facts = async { play_facts(&watched, &card, true) };
    tokio::join!(facts, run(verdict, Ok(world.ready(failing)), &watched));
    assert_eq!(
        recorder.updates().last().map(wire),
        Some(wire(&AboutUpdate::VerdictEnd {
            ending: VerdictEnding::Failed {
                problem: VerdictProblem::RateLimited
            },
            cost: None
        }))
    );
    assert_eq!(world.metered.load(Ordering::SeqCst), 0);
}

/// A call that ends with no text is no verdict: it ends `noAnswer` with its cost, and
/// nothing is cached for a reopen.
#[tokio::test]
async fn a_call_that_says_nothing_ends_with_no_answer() {
    let card = text_file("notes.txt", "x\n");
    let world = World::new();
    let limits = Limits::default();
    for _ in 0..2 {
        let (recorder, watched) = recorded();
        let token = CancellationToken::new();
        let verdict = verdict_for(&card, &token, &limits, &world);
        let facts = async { play_facts(&watched, &card, true) };
        tokio::join!(facts, run(verdict, Ok(world.ready(says(&[]))), &watched));
        let Some(AboutUpdate::VerdictEnd {
            ending: VerdictEnding::Failed {
                problem: VerdictProblem::NoAnswer,
            },
            cost: Some(_),
        }) = recorder.updates().last()
        else {
            panic!("an empty answer is no verdict: {:?}", recorder.updates());
        };
    }
    assert_eq!(world.calls.load(Ordering::SeqCst), 2, "nothing empty was reused");
}

/// The Stop command stops the card's own verdict and no other.
#[test]
fn the_stop_command_stops_only_that_cards_verdict() {
    let mine = CancellationToken::new();
    let _claim = limits::LIMITS.claim("about-stop-test", mine.clone());
    crate::about::commands::about_stop_verdict("another-card".to_string());
    assert!(!mine.is_cancelled());
    crate::about::commands::about_stop_verdict("about-stop-test".to_string());
    assert!(mine.is_cancelled());
}

/// An unpriced model's cost reads unknown, never $0.
#[test]
fn an_unpriced_model_reads_unknown() {
    let priced: PricedCost = crate::agent::pricing::price_call(
        ProviderTag::Anthropic,
        "claude-from-the-future",
        AgentUsage {
            prompt_tokens: 900,
            completion_tokens: 40,
            ..AgentUsage::default()
        },
    );
    assert!(!priced.priced);
}
