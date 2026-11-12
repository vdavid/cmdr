use gtk::gdk::ModifierType;
use gtk::prelude::GtkWindowExt;

use super::MnemonicModifier;

/// Sets the window's `GtkWindow` mnemonic modifier on the GTK main thread. GTK objects never
/// leave it, so the GTK window is fetched inside the closure.
pub(crate) fn apply(window: &tauri::Window, modifier: MnemonicModifier) -> tauri::Result<()> {
    let mask = match modifier {
        MnemonicModifier::Alt => ModifierType::MOD1_MASK,
        MnemonicModifier::Unreachable => {
            ModifierType::CONTROL_MASK
                | ModifierType::SHIFT_MASK
                | ModifierType::MOD1_MASK
                | ModifierType::SUPER_MASK
                | ModifierType::HYPER_MASK
                | ModifierType::META_MASK
        }
    };
    let target = window.clone();
    window.run_on_main_thread(move || match target.gtk_window() {
        Ok(gtk_window) => gtk_window.set_mnemonic_modifier(mask),
        Err(error) => log::warn!(
            target: "menu",
            "Couldn't reach window {}'s GTK window to set its menu mnemonics: {error}",
            // allowed-log-private: a window label is Cmdr's own id (`main`, `settings`), never the user's.
            target.label()
        ),
    })
}
