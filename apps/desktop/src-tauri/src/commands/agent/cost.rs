//! Cost visibility: the per-thread footer total and the per-day rollup.

use tauri::AppHandle;

use super::with_read_connection;
use crate::agent::store;

/// One conversation's cumulative token + cost total (all days, all models), for the
/// per-thread footer. Zeroed for a thread with no metered turn yet. Empty store ⇒ zeroed.
#[tauri::command]
#[specta::specta]
pub async fn ask_cmdr_conversation_cost(app: AppHandle, id: i64) -> Result<store::ConversationCost, String> {
    let empty = store::ConversationCost {
        prompt_tokens: 0,
        completion_tokens: 0,
        cost_micros: 0,
        fully_priced: true,
        providers: Vec::new(),
    };
    with_read_connection(app, empty, move |conn| store::conversation_cost(conn, id)).await
}

/// The per-day cost rollup across every thread and model, newest day first (the settings
/// spend display). Empty when the store never opened.
#[tauri::command]
#[specta::specta]
pub async fn ask_cmdr_cost_summary(app: AppHandle) -> Result<store::CostSummary, String> {
    let empty = store::CostSummary { days: Vec::new() };
    with_read_connection(app, empty, store::cost_summary).await
}
