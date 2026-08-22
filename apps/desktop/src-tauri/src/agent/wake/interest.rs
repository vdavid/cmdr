//! The interest scorer: how much one bundle is worth waking the agent for, and how soon.
//!
//! A pure function of two values — what changed, and what the importance scorer says about the
//! folder it changed in. No clock, no store, no I/O, so the agent's proactive behaviour is
//! reproducible and testable rather than a thing that happens to emerge at runtime.

use std::time::Duration;

use super::EventBundle;

/// Wake within seconds: something arrived somewhere that matters.
///
/// The three tier values are coarse on purpose and want tuning against real use (agent-spec
/// §18); what has to hold is the ORDER, which `wake_delay` is tested for.
pub const DEFAULT_HOT_DELAY: Duration = Duration::from_secs(5);

/// How much more patience a warm bundle gets than a hot one: a minute for every second.
///
/// One number the user moves, both tiers following it, so "calmer, please" means calmer
/// everywhere rather than calmer in the one place they happened to see.
const WARM_MULTIPLE: u32 = 60;

/// However patient the user asks the agent to be, a warm folder is looked at within the working
/// day rather than eventually. At the slider's quiet end the multiple alone would say five days.
pub const MAX_WARM_DELAY: Duration = Duration::from_secs(6 * 60 * 60);

/// At or above this, a bundle is worth waking for within seconds.
pub const HOT_THRESHOLD: f64 = 0.7;
/// At or above this, a bundle is worth waking for within minutes.
pub const WARM_THRESHOLD: f64 = 0.3;

/// What an UNKNOWN folder is worth: enough to be noticed, below any folder actually scored as
/// mattering.
///
/// This constant is the whole reason [`FolderImportance`] has three variants instead of a
/// float. It must stay above zero — see [`FolderImportance::Unknown`].
const UNKNOWN_IMPORTANCE_WEIGHT: f64 = 0.35;

/// The change count at which volume ALONE maxes out the signal. Logarithmic below it, clamped
/// above: the step from 5 changes to 50 is worth a lot, 50 to 500 less, and 500,000 to
/// 5,000,000 nothing at all. One pathological folder must not be able to out-shout every other
/// bundle in the inbox.
const VOLUME_REFERENCE: f64 = 1_000.0;

/// What the importance scorer says about a folder, in this module's own vocabulary.
///
/// Mirrors `cmdr_index::importance::WeightLookup`'s three-way answer deliberately, and does
/// NOT mirror its `score()`, which collapses `Floored` and `Unscored` into the same `0.0`.
/// That collapse is right for ranking folders by stored weight and wrong here.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FolderImportance {
    /// The importance scorer has a weight for this folder, in `0.0..=1.0`.
    Scored(f64),
    /// The folder is deliberately-junk ground: `node_modules`, `.git`, a cache dir. Change
    /// here is real but never worth acting on.
    Floored,
    /// The importance scorer hasn't reached this folder yet — a project cloned five minutes
    /// ago, a volume still scanning.
    ///
    /// ❌ Never treat this as zero. An unknown folder collapsed into zero scores exactly like
    /// `node_modules`, so the agent would silently ignore every new project folder on the
    /// disk, and the symptom ("it just isn't very good at noticing things") points nowhere
    /// near the cause.
    Unknown,
}

impl FolderImportance {
    /// The weight this answer contributes. `Unknown` is deliberately not zero.
    fn weight(self) -> f64 {
        match self {
            FolderImportance::Scored(score) => score.clamp(0.0, 1.0),
            FolderImportance::Floored => 0.0,
            FolderImportance::Unknown => UNKNOWN_IMPORTANCE_WEIGHT,
        }
    }
}

/// How much a bundle is worth waking for, in `0.0..=1.0`.
///
/// A newtype rather than a bare `f64` so it can't be confused with an importance weight, which
/// is the other 0-to-1 number in this module and means something entirely different.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Interest(f64);

impl Interest {
    /// An interest of exactly `value`, clamped into range. For a caller that already has a
    /// score (a stored inbox row) rather than a bundle to weigh.
    pub fn of(value: f64) -> Self {
        Interest(value.clamp(0.0, 1.0))
    }

    pub fn value(self) -> f64 {
        self.0
    }
}

/// Score one bundle against what's known about its folder.
///
/// Two signals, combined by taking the STRONGER rather than by averaging: a folder is
/// interesting because things APPEARED in it (intent) or because a great deal happened in it
/// (volume). Averaging would let a single high-intent arrival — the flagship "a file just
/// landed in Downloads" — be diluted to lukewarm by its own low volume, which is the one case
/// the feature exists for.
pub fn interest(bundle: &EventBundle, importance: FolderImportance) -> Interest {
    let total = bundle.counters.total();
    if total == 0 {
        return Interest(0.0);
    }
    let weight = importance.weight();
    let signal = bundle.counters.intent_share().max(volume_signal(total));
    Interest((weight * signal).clamp(0.0, 1.0))
}

/// How much the sheer amount of change is worth, saturating at [`VOLUME_REFERENCE`].
fn volume_signal(total: u64) -> f64 {
    let scaled = ((total as f64) + 1.0).ln() / (VOLUME_REFERENCE + 1.0).ln();
    scaled.clamp(0.0, 1.0)
}

/// How long a bundle of this interest may wait before the agent is woken for it, or `None` when
/// it is not worth waking for at all.
///
/// Coarse tiers, not a continuous curve: the exact seconds are untuned (§18) but the ordering
/// is a contract, and three named tiers are something a person can reason about when deciding
/// whether the agent feels attentive or twitchy.
///
/// **A cold bundle gets no deadline**, which is what makes it ride along rather than cause a
/// wake. Given one, a trickle in a barely-scored folder comes due on its own and spends a whole
/// model turn reporting that a cache directory changed.
pub fn wake_delay(interest: Interest, hot_delay: Duration) -> Option<Duration> {
    if interest.value() >= HOT_THRESHOLD {
        Some(hot_delay)
    } else if interest.value() >= WARM_THRESHOLD {
        Some(warm_delay(hot_delay))
    } else {
        None
    }
}

/// The warm tier for a given hot setting: [`WARM_MULTIPLE`] times it, held to
/// [`MAX_WARM_DELAY`]. Saturating, so an absurd setting cannot overflow into a short wait.
fn warm_delay(hot_delay: Duration) -> Duration {
    hot_delay.saturating_mul(WARM_MULTIPLE).min(MAX_WARM_DELAY)
}
