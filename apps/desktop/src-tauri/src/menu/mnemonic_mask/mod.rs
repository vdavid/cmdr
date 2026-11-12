//! Turning a window's GTK menu-bar mnemonics off while a modal surface owns the keyboard.
//!
//! On Linux every bar title carries a mnemonic (`mnemonics.rs`), and GTK takes a title's
//! `Alt+<letter>` before WebKit sees the keydown, so `preventDefault` can't help: the search
//! dialog's `Alt+F` would open the File menu instead of switching to Filename mode. A native modal
//! blocks its parent's bar; Cmdr's dialogs are web content, so the window does it by hand: while
//! one is up, its `GtkWindow` mnemonic modifier becomes a mask no keyboard produces, and it goes
//! back to Alt when the last one closes (real X key events prove it in the Linux E2E lane:
//! `apps/desktop/test/e2e-playwright/menu-mnemonic-mask.spec.ts`).
//!
//! Item mnemonics are inert while a menu is closed, so the titles are the whole problem. A click on
//! the bar still works: the mask only changes which modifier activates a mnemonic. The modifier is
//! a property of the window, so it survives a bar rebuild (`window.set_menu` on a language change).
//!
//! The decision ([`mnemonic_modifier`]) is pure and compiled everywhere, so its test runs on every
//! host; only the GTK call is Linux's (`platform/`). macOS has no mnemonics, and Windows has none
//! until its menu model is decided, so both arms answer without touching anything.

mod platform;

/// Which modifier activates a window's bar mnemonics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MnemonicModifier {
    /// GTK's default: `Alt+<letter>` opens the menu whose title underlines that letter.
    Alt,
    /// Every modifier GTK knows, held at once. Nobody presses Ctrl+Alt+Shift+Super+Hyper+Meta with a
    /// letter, and where Hyper or Meta has no key (the usual X keymap) GTK can't match it at all.
    Unreachable,
}

/// The modifier a window's mnemonics answer to, given whether a modal surface owns its keyboard.
pub(crate) fn mnemonic_modifier(suspended: bool) -> MnemonicModifier {
    if suspended {
        MnemonicModifier::Unreachable
    } else {
        MnemonicModifier::Alt
    }
}

/// Suspends or restores `window`'s bar mnemonics. Nothing to recover on failure (the bar keeps its
/// mnemonics, and the keypress opens a menu as it always did), so it's a log line.
pub(crate) fn set_window_mnemonics_suspended(window: &tauri::Window, suspended: bool) {
    if let Err(error) = platform::apply(window, mnemonic_modifier(suspended)) {
        // allowed-log-private: a window label is Cmdr's own id (`main`, `settings`), never the user's.
        log::warn!(target: "menu", "Couldn't set window {}'s menu mnemonics: {error}", window.label());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_modal_surface_takes_the_mnemonics_away_and_closing_it_gives_them_back() {
        assert_eq!(mnemonic_modifier(true), MnemonicModifier::Unreachable);
        assert_eq!(mnemonic_modifier(false), MnemonicModifier::Alt);
    }
}
