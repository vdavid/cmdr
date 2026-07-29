//! What a dropped tool result says about itself: a digest of the call, a digest of the
//! result, and the sentence that gets the model back to the real thing.
//!
//! **Structural, never a model call.** Every phrase here is array lengths, key names,
//! counts, and a common path prefix, derived from the two JSON values the assembly already
//! holds. So it costs nothing, it can't hallucinate, and it stays a pure function.
//!
//! **Shape-agnostic on purpose** (invariant 2): no branch names a tool. The rules below read
//! whatever JSON a tool happens to return, which is why the pure core can carry them at all.
//! The moment one tool needs wording these rules can't produce, the fix is a `digest()`
//! passed in as a value by the caller — NOT a match arm per tool in here.
//!
//! **A result's strings never survive.** A digest reports a string field's LENGTH, never its
//! text, at every depth. Two reasons, and the second is the load-bearing one:
//! 2,000 characters of OCR has no re-fetch value, and text lifted out of a result reads as
//! content the model was handed. A digest is a description of a delivery, never a delivery
//! (invariant 6), and it must not look like one either.
//!
//! The CALL's arguments are quoted, within a cap: the model wrote them itself, and they are
//! what makes the call reconstructable.

use serde_json::Value;

/// The most fields one array's digest names before it says "and more".
const MAX_FIELDS: usize = 6;

/// The most bytes one argument's value may contribute, so one long string can't crowd out
/// the keys after it.
const MAX_ARGUMENT_BYTES: usize = 48;

/// The most bytes the re-fetch sentence takes from an argument's key name.
const MAX_NOUN_BYTES: usize = 24;

const ELLIPSIS: &str = "…";

/// How the call read, within `budget` bytes: `12 paths under /Users/me/Downloads/shots,
/// volumeId: root`. The model's own arguments, so it can re-issue the call it lost.
pub(super) fn of_arguments(arguments: Option<&Value>, budget: usize) -> String {
    let described = match arguments {
        None => "unknown".to_string(),
        Some(Value::Object(map)) if map.is_empty() => "no arguments".to_string(),
        Some(Value::Object(map)) => map
            .iter()
            .map(|(key, value)| describe_argument(key, value))
            .collect::<Vec<_>>()
            .join(", "),
        Some(other) => other.to_string(),
    };
    truncate(&described, budget)
}

/// What the dropped result held, within `budget` bytes: `0 coverage, 12 facts (path, state,
/// tags in 9, text in 11), status (2 chars)`. Counts and key names only.
pub(super) fn of_result(content: &Value, budget: usize) -> String {
    let described = match content {
        Value::Object(map) if map.is_empty() => "an empty object".to_string(),
        Value::Object(map) => map
            .iter()
            .map(|(key, value)| describe_field(key, value))
            .collect::<Vec<_>>()
            .join(", "),
        Value::Array(items) => describe_array("items", items),
        Value::String(text) => format!("{} chars", text.chars().count()),
        Value::Null => "nothing".to_string(),
        other => other.to_string(),
    };
    truncate(&described, budget)
}

/// The way back to the real thing. Re-calling IS the rehydrate (every agent tool is an
/// idempotent local read), so the hint names the tool and, when the call took a collection,
/// the model's own word for what it asked about.
pub(super) fn refetch_hint(tool: Option<&str>, arguments: Option<&Value>) -> String {
    let tool = tool.unwrap_or("the tool");
    match arguments.and_then(collection_key) {
        Some(noun) => format!("call {tool} again for the {noun} you still need"),
        None => format!("call {tool} again if you still need what it returned"),
    }
}

/// One argument as the digest reads it. A collection becomes a count (plus the folder its
/// paths share, which is the one thing that makes 12 paths re-issuable in a phrase); a
/// scalar keeps its value, capped.
fn describe_argument(key: &str, value: &Value) -> String {
    match value {
        Value::Array(items) => match common_dir(items) {
            Some(dir) => format!("{} {key} under {dir}", items.len()),
            None => format!("{} {key}", items.len()),
        },
        Value::Object(map) => format!("{key} ({} fields)", map.len()),
        Value::String(text) => format!("{key}: {}", truncate(text, MAX_ARGUMENT_BYTES)),
        Value::Null => format!("{key}: null"),
        other => format!("{key}: {other}"),
    }
}

/// One result field as the digest reads it. A string reports its LENGTH — this is the branch
/// that keeps OCR text out of the prompt.
fn describe_field(key: &str, value: &Value) -> String {
    match value {
        Value::Array(items) => describe_array(key, items),
        Value::Object(map) => format!("{key} ({} fields)", map.len()),
        Value::String(text) => format!("{key} ({} chars)", text.chars().count()),
        Value::Null => format!("{key}: null"),
        other => format!("{key}: {other}"),
    }
}

/// `12 facts (path, state, tags in 9, text in 11)`: how many rows, which fields they carry,
/// and — where only some rows filled one in — how many did. The gaps are what tell a model
/// whether re-fetching is worth it.
fn describe_array(key: &str, items: &[Value]) -> String {
    let fields = row_fields(items);
    if fields.is_empty() {
        format!("{} {key}", items.len())
    } else {
        format!("{} {key} ({})", items.len(), fields.join(", "))
    }
}

/// The field names the objects in `items` carry, each with `in N` when only N of the rows
/// filled it in. Names and counts, never a value.
fn row_fields(items: &[Value]) -> Vec<String> {
    let mut order: Vec<&str> = Vec::new();
    let mut filled: Vec<usize> = Vec::new();
    for item in items {
        let Some(map) = item.as_object() else { continue };
        for (key, value) in map {
            let position = match order.iter().position(|seen| *seen == key.as_str()) {
                Some(position) => position,
                None => {
                    order.push(key);
                    filled.push(0);
                    order.len() - 1
                }
            };
            if is_filled(value) {
                filled[position] += 1;
            }
        }
    }
    let mut described: Vec<String> = order
        .iter()
        .zip(&filled)
        .take(MAX_FIELDS)
        .map(|(key, count)| {
            if *count == items.len() {
                (*key).to_string()
            } else {
                format!("{key} in {count}")
            }
        })
        .collect();
    if order.len() > MAX_FIELDS {
        described.push("and more".to_string());
    }
    described
}

/// Whether a field carries anything: an empty string, an empty list, and `null` all read as
/// "this row didn't have one", which is what `in N` counts.
fn is_filled(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::String(text) => !text.is_empty(),
        Value::Array(items) => !items.is_empty(),
        Value::Object(map) => !map.is_empty(),
        Value::Bool(_) | Value::Number(_) => true,
    }
}

/// The folder every path in `items` sits under, when they are all path-like strings: their
/// longest common prefix, cut back to a separator so it names a directory rather than half a
/// filename.
fn common_dir(items: &[Value]) -> Option<String> {
    let paths: Vec<&str> = items.iter().map(Value::as_str).collect::<Option<Vec<_>>>()?;
    let first = *paths.first()?;
    if !first.contains('/') {
        return None;
    }
    let shared = paths[1..]
        .iter()
        .fold(first.len(), |shared, path| shared.min(common_prefix_bytes(first, path)));
    let separator = first[..shared].rfind('/')?;
    let dir = if separator == 0 { "/" } else { &first[..separator] };
    Some(truncate(dir, MAX_ARGUMENT_BYTES))
}

/// How many bytes two strings share from the front, always landing on a character boundary.
fn common_prefix_bytes(a: &str, b: &str) -> usize {
    a.chars()
        .zip(b.chars())
        .take_while(|(x, y)| x == y)
        .map(|(x, _)| x.len_utf8())
        .sum()
}

/// The first argument key holding a non-empty collection: the noun the re-fetch sentence
/// uses. The model's own key name, so the sentence never invents vocabulary.
fn collection_key(arguments: &Value) -> Option<String> {
    arguments
        .as_object()?
        .iter()
        .find(|(_, value)| matches!(value, Value::Array(items) if !items.is_empty()))
        .map(|(key, _)| truncate(key, MAX_NOUN_BYTES))
}

/// Cut `text` to `budget` BYTES (the unit the shared token estimator counts), on a character
/// boundary, marking the cut.
fn truncate(text: &str, budget: usize) -> String {
    if text.len() <= budget {
        return text.to_string();
    }
    let mut end = budget.saturating_sub(ELLIPSIS.len()).min(text.len());
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}{ELLIPSIS}", &text[..end])
}
