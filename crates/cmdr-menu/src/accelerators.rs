//! Keyboard accelerators: the frontend's shortcut strings in Tauri's accelerator syntax, and that
//! syntax as the glyphs a macOS menu draws.
//!
//! The conversion is the seam between two vocabularies. The frontend speaks macOS glyphs and
//! canonical key words (`⌘⇧P`, `Backspace`), or off macOS the `+`-joined words it captures there
//! (`Ctrl+Shift+P`); Tauri wants `Cmd+Shift+P`, and on Linux `Ctrl+Shift+P` for a ⌘ (#284).
//! Anything unrecognized passes through as-is, so a glyph that leaks in here produces a garbage
//! accelerator rather than an error, which is what the canonical-key-names test guards.
//!
//! Two ways out, because a menu BAR accelerator is registered with the app and a POPUP's is only
//! drawn: [`to_menu_text`] converts, [`to_accelerator`] converts and then refuses anything the
//! bar can't safely own. The desktop's `menu/accelerators.rs` wraps both for the platform it runs
//! on, and parses what they emit with muda. [`glyphs`] goes back the other way, for a host that
//! draws the bar itself (the browser demo).

use crate::spec::Platform;

/// Convert frontend shortcut format (⌘2) to the text a menu item shows (Cmd+2, Ctrl+2 on Linux).
/// Returns None if the shortcut is empty.
///
/// No floor of any kind: a popup menu's key equivalents are never registered with the
/// app, so a bare `Space` or `F5` there is display text and can't swallow anything.
/// ❌ The menu bar must not use this — [`to_accelerator`] is its door.
pub fn to_menu_text(shortcut: &str, platform: Platform) -> Option<String> {
    convert(shortcut, platform).map(|(accelerator, _)| accelerator)
}

/// Convert frontend shortcut format (⌘2) to a Tauri accelerator the menu BAR may register (Cmd+2,
/// Ctrl+2 on Linux). Returns None if the shortcut is empty, carries no ⌘ / ⌃ / ⌥ / Super, or names
/// a key muda has no `Code` for.
///
/// ❗ The modifier floor is what keeps a bare key out of the menu bar. AppKit fires a
/// registered accelerator app-wide, ahead of the webview, so a menu item bound to `*`
/// or `+` would eat that character in every text field in the app. Shift alone doesn't
/// clear the floor either: `⇧8` IS `*`.
///
/// ❗ The second refusal is [`is_codeable_key`], and it exists because the failure is SILENT:
/// Tauri discards a parse error and builds the item with no accelerator at all, so a string muda
/// can't read is indistinguishable from no shortcut until someone opens the menu and looks.
///
/// A combo refused for either reason isn't lost — it becomes a display-only accelerator
/// (`spec::ItemSpec::display_accelerator`), drawn beside the item while the file pane's
/// keydown handler does the work.
pub fn to_accelerator(shortcut: &str, platform: Platform) -> Option<String> {
    match convert(shortcut, platform)? {
        (accelerator, true) => Some(accelerator),
        (_, false) => None,
    }
}

/// A Tauri accelerator (`Cmd+Shift+P`, `Alt+Cmd+O`, `Shift+F8`) as a macOS menu draws it: the
/// modifiers in AppKit's fixed order (⌃⌥⇧⌘), then the key, with the named keys as their glyphs
/// (`⇧⌘P`, `⌥⌘O`, `⇧F8`). Display text only, ❌ never parsed back.
pub fn glyphs(accelerator: &str) -> String {
    let mut held = [false; 4];
    let mut key = "";
    for token in accelerator.split('+') {
        match token.to_ascii_uppercase().as_str() {
            "CTRL" | "CONTROL" => held[0] = true,
            "ALT" | "OPTION" => held[1] = true,
            "SHIFT" => held[2] = true,
            "CMD" | "COMMAND" | "SUPER" | "META" => held[3] = true,
            _ => key = token,
        }
    }
    // `Cmd++`: the split's last two tokens are empty, and the key was a `+`.
    if key.is_empty() && accelerator.ends_with('+') {
        key = "+";
    }
    let mut out: String = ["⌃", "⌥", "⇧", "⌘"]
        .iter()
        .zip(held)
        .filter_map(|(glyph, on)| on.then_some(*glyph))
        .collect();
    let named = match key.to_ascii_uppercase().as_str() {
        "UP" => Some("↑"),
        "DOWN" => Some("↓"),
        "LEFT" => Some("←"),
        "RIGHT" => Some("→"),
        "ENTER" | "RETURN" => Some("↩"),
        "TAB" => Some("⇥"),
        "SPACE" => Some("Space"),
        "ESCAPE" | "ESC" => Some("⎋"),
        "BACKSPACE" => Some("⌫"),
        "DELETE" => Some("⌦"),
        "PAGEUP" => Some("⇞"),
        "PAGEDOWN" => Some("⇟"),
        "HOME" => Some("↖"),
        "END" => Some("↘"),
        "EQUAL" => Some("="),
        "MINUS" => Some("-"),
        "PLUS" => Some("+"),
        _ => None,
    };
    match named {
        Some(glyph) => out.push_str(glyph),
        None => out.push_str(&key.to_uppercase()),
    }
    out
}

/// The punctuation muda names a physical key for, verbatim from `parse_named_code`
/// (`muda-0.20.0/src/accelerator/mod.rs:278`). ASCII letters and digits are the rest of it.
const CODEABLE_PUNCTUATION: &str = "`\\[],=-./';";

/// Whether muda can turn this key into a `Code`, which decides whether an accelerator naming
/// it can be REGISTERED at all.
///
/// ❗ The SHIFTED characters are the gap: `+`, `*`, `(`, `_` and friends have no `Code` of their
/// own, only the unshifted key they sit on. So `⌘+` can't be a real accelerator however it's
/// spelled — Tauri would discard the parse error and build the item with no key, silently. It
/// routes to the display path instead, exactly like a bare key, which is honest: the glyph shows
/// and the frontend's keydown dispatch runs the command.
///
/// ❌ Don't "fix" this by emitting `Cmd+Shift+Equal` for `⌘+`: that's a guess about the user's
/// layout, and the physical key that types `+` moves between them.
fn is_codeable_key(key: &str) -> bool {
    let mut chars = key.chars();
    let (Some(only), None) = (chars.next(), chars.next()) else {
        // A word form that reached here fell through every named-key arm above, so muda has no
        // name for it either.
        return false;
    };
    only.is_ascii_alphanumeric() || CODEABLE_PUNCTUATION.contains(only)
}

/// A modifier, whichever way the frontend spelled it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Modifier {
    Command,
    Control,
    Alt,
    Shift,
    Super,
}

/// Every modifier prefix the frontend writes, in both of its spellings: macOS glyphs (`⌘⇧P`, which
/// is how the registry ships its defaults and how a Mac captures a rebind), and the `+`-joined words
/// `formatKeyCombo` captures off macOS (`Ctrl+Alt+Shift+Super+P`).
const MODIFIER_PREFIXES: [(&str, Modifier); 8] = [
    ("⌘", Modifier::Command),
    ("⌃", Modifier::Control),
    ("⌥", Modifier::Alt),
    ("⇧", Modifier::Shift),
    ("Ctrl+", Modifier::Control),
    ("Alt+", Modifier::Alt),
    ("Shift+", Modifier::Shift),
    ("Super+", Modifier::Super),
];

/// The modifiers `shortcut` opens with, in its own order, and the key after them.
fn split_modifiers(shortcut: &str) -> (Vec<Modifier>, &str) {
    let mut modifiers = Vec::new();
    let mut rest = shortcut;
    'strip: loop {
        for (prefix, modifier) in MODIFIER_PREFIXES {
            if let Some(after) = rest
                .get(..prefix.len())
                .filter(|head| head.eq_ignore_ascii_case(prefix))
                .map(|_| &rest[prefix.len()..])
            {
                modifiers.push(modifier);
                rest = after;
                continue 'strip;
            }
        }
        return (modifiers, rest);
    }
}

/// The accelerator tokens for `modifiers` on `platform`.
///
/// ❗ On Linux ⌘ is Ctrl, ❌ never `Cmd`: muda maps `Cmd` to `Modifiers::META`, which GTK binds and
/// prints as the Super key (#284). It's the chord the frontend dispatches ⌘ as there too
/// (`toPlatformShortcut`), and like it, a combo holding ⌃ as well spends that one on Shift, so
/// the two don't collapse into one Ctrl.
fn modifier_tokens(modifiers: &[Modifier], platform: Platform) -> Vec<&'static str> {
    let holds_command = modifiers.contains(&Modifier::Command);
    let mut tokens: Vec<&'static str> = Vec::new();
    for modifier in modifiers {
        let token = match (modifier, platform) {
            (Modifier::Command, Platform::MacOs) => "Cmd",
            (Modifier::Command, Platform::Linux) => "Ctrl",
            (Modifier::Control, Platform::Linux) if holds_command => "Shift",
            (Modifier::Control, _) => "Ctrl",
            // ❗ `Alt`, ❌ never `Opt`: muda accepts `OPTION` and `ALT` and nothing else
            // (`muda-0.20.0/src/accelerator/mod.rs:249`), and Tauri throws the parse error away
            // (`normal.rs:65` is `.parse().ok()`), so `Opt` built items with NO accelerator at
            // all, silently. `every_accelerator_this_converter_emits_is_one_muda_can_parse` guards it.
            (Modifier::Alt, _) => "Alt",
            (Modifier::Shift, _) => "Shift",
            (Modifier::Super, _) => "Super",
        };
        if !tokens.contains(&token) {
            tokens.push(token);
        }
    }
    tokens
}

/// The accelerator's name for `key`, plus whether muda can turn it into a `Code`.
fn key_token(key: &str) -> (String, bool) {
    const NAMED: [&str; 11] = [
        "Enter",
        "Space",
        "Tab",
        "Escape",
        "Backspace",
        "Delete",
        "Insert",
        "PageUp",
        "PageDown",
        "Home",
        "End",
    ];
    let arrow = match key {
        "↑" => Some("Up"),
        "↓" => Some("Down"),
        "←" => Some("Left"),
        "→" => Some("Right"),
        _ => None,
    };
    if let Some(arrow) = arrow {
        return (arrow.to_string(), true);
    }
    if let Some(named) = NAMED.iter().find(|named| named.eq_ignore_ascii_case(key)) {
        return ((*named).to_string(), true);
    }
    let is_function_key =
        key.len() > 1 && key.starts_with(['F', 'f']) && key[1..].bytes().all(|byte| byte.is_ascii_digit());
    if is_function_key {
        return (key.to_uppercase(), true);
    }
    (key.to_uppercase(), is_codeable_key(key))
}

/// The converted accelerator, plus whether the menu bar may register it: it needs a command
/// modifier (⌘, ⌃, ⌥, or Super) AND a key muda can name.
fn convert(shortcut: &str, platform: Platform) -> Option<(String, bool)> {
    let (modifiers, key) = split_modifiers(shortcut);
    let mut tokens: Vec<String> = modifier_tokens(&modifiers, platform)
        .into_iter()
        .map(str::to_string)
        .collect();
    let mut registerable = modifiers.iter().any(|modifier| *modifier != Modifier::Shift);
    if key.is_empty() {
        registerable = false;
    } else {
        let (key, codeable) = key_token(key);
        registerable &= codeable;
        tokens.push(key);
    }
    if tokens.is_empty() {
        None
    } else {
        Some((tokens.join("+"), registerable))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What a macOS menu draws for every accelerator `MENU_BAR` registers there.
    #[test]
    fn the_glyphs_a_mac_menu_draws() {
        for (accelerator, drawn) in [
            ("Cmd+Shift+P", "⇧⌘P"),
            ("Alt+Cmd+O", "⌥⌘O"),
            ("Cmd+Alt+C", "⌥⌘C"),
            ("Shift+F8", "⇧F8"),
            ("F3", "F3"),
            ("Ctrl+Shift+Tab", "⌃⇧⇥"),
            ("Shift+Space", "⇧Space"),
            ("Cmd+Up", "⌘↑"),
            ("Cmd+Equal", "⌘="),
            ("Cmd+Minus", "⌘-"),
            ("Cmd+,", "⌘,"),
            ("Cmd+Shift+.", "⇧⌘."),
            ("Alt+Shift+Enter", "⌥⇧↩"),
            ("Cmd+[", "⌘["),
            ("Ctrl+t", "⌃T"),
            ("Cmd++", "⌘+"),
        ] {
            assert_eq!(glyphs(accelerator), drawn, "`{accelerator}`");
        }
    }

    /// A rebind goes frontend glyphs → accelerator → glyphs, and comes back in AppKit's order.
    #[test]
    fn a_rebind_draws_in_appkits_modifier_order() {
        let drawn = to_accelerator("⌘⇧T", Platform::MacOs).map(|accelerator| glyphs(&accelerator));
        assert_eq!(drawn.as_deref(), Some("⇧⌘T"));
    }

    #[test]
    fn test_frontend_shortcut_to_accelerator_simple() {
        // Basic modifier + key combinations
        assert_eq!(to_accelerator("⌘1", Platform::MacOs), Some("Cmd+1".to_string()));
        assert_eq!(to_accelerator("⌘2", Platform::MacOs), Some("Cmd+2".to_string()));
        assert_eq!(to_accelerator("⌘⇧P", Platform::MacOs), Some("Cmd+Shift+P".to_string()));
        assert_eq!(to_accelerator("⌥⌘O", Platform::MacOs), Some("Alt+Cmd+O".to_string()));
        assert_eq!(to_accelerator("⌃⌘C", Platform::MacOs), Some("Ctrl+Cmd+C".to_string()));
    }

    #[test]
    fn test_frontend_shortcut_to_accelerator_arrows() {
        assert_eq!(to_accelerator("⌘↑", Platform::MacOs), Some("Cmd+Up".to_string()));
        assert_eq!(to_accelerator("⌘↓", Platform::MacOs), Some("Cmd+Down".to_string()));
        assert_eq!(to_accelerator("⌘[", Platform::MacOs), Some("Cmd+[".to_string()));
        assert_eq!(to_accelerator("⌘]", Platform::MacOs), Some("Cmd+]".to_string()));
    }

    /// The key words a popup menu spells out. Every one of them is bare, so the menu
    /// bar refuses them and only the popup path sees them.
    #[test]
    fn test_frontend_shortcut_to_menu_text_special_keys() {
        assert_eq!(to_menu_text("Tab", Platform::MacOs), Some("Tab".to_string()));
        assert_eq!(to_menu_text("Enter", Platform::MacOs), Some("Enter".to_string()));
        assert_eq!(to_menu_text("Space", Platform::MacOs), Some("Space".to_string()));
        assert_eq!(to_menu_text("F4", Platform::MacOs), Some("F4".to_string()));
        assert_eq!(
            to_menu_text("Backspace", Platform::MacOs),
            Some("Backspace".to_string())
        );
        assert_eq!(to_menu_text("Delete", Platform::MacOs), Some("Delete".to_string()));
        assert_eq!(to_menu_text("Insert", Platform::MacOs), Some("Insert".to_string()));
        assert_eq!(to_menu_text("PageUp", Platform::MacOs), Some("PageUp".to_string()));
        assert_eq!(to_menu_text("Escape", Platform::MacOs), Some("Escape".to_string()));
    }

    /// ❗ The modifier floor. AppKit fires a menu-bar accelerator app-wide, ahead of the
    /// webview, so a bare `*` / `+` / `-` registered here would be swallowed in every
    /// text field in the app. Shift doesn't clear the floor: `⇧8` IS `*` on a US layout.
    #[test]
    fn a_combo_with_no_command_modifier_is_no_menu_bar_accelerator() {
        for bare in ["*", "+", "-", "⇧8", "⇧-", "Space", "Tab", "F4", "Backspace", "↑"] {
            assert_eq!(
                to_accelerator(bare, Platform::MacOs),
                None,
                "`{bare}` carries no ⌘ / ⌃ / ⌥, so the menu bar must not register it"
            );
            assert!(
                to_menu_text(bare, Platform::MacOs).is_some(),
                "`{bare}` is still fine as menu TEXT, which is what a popup shows"
            );
        }
    }

    /// Any one of the three clears it, and the two paths then agree.
    #[test]
    fn one_command_modifier_is_enough_to_register() {
        for combo in ["⌘K", "⌃Enter", "⌥⇧=", "⇧⌘A"] {
            assert_eq!(
                to_accelerator(combo, Platform::MacOs),
                to_menu_text(combo, Platform::MacOs),
                "`{combo}` carries a command modifier, so both paths spell it the same way"
            );
            assert!(to_accelerator(combo, Platform::MacOs).is_some(), "`{combo}`");
        }
    }

    /// The SECOND refusal: a command modifier isn't enough if the key itself has no muda `Code`.
    ///
    /// Every shifted character is in this hole — muda names the physical key, and `+` doesn't have
    /// one. `view.zoom.in` ships `⌘+`, so this is a combo the app really produces. Registering it
    /// is impossible however it's spelled, and the silent failure (Tauri discards the parse error)
    /// is why refusing beats emitting something that looks fine and does nothing.
    #[test]
    fn a_key_muda_cannot_name_is_no_menu_bar_accelerator_either() {
        for uncodeable in ["⌘+", "⌘*", "⌥(", "⌃_"] {
            assert_eq!(
                to_accelerator(uncodeable, Platform::MacOs),
                None,
                "`{uncodeable}` names a key muda has no `Code` for, so registering it would \
                 silently produce a menu item with no accelerator"
            );
        }
        // The unshifted keys those characters sit on are fine, which is what makes the hole
        // specific rather than a blanket refusal of punctuation.
        for codeable in ["⌘=", "⌘-", "⌘[", "⌘,", "⌘.", "⌘/", "⌘;", "⌘'", "⌘`"] {
            assert!(
                to_accelerator(codeable, Platform::MacOs).is_some(),
                "`{codeable}` sits on a key muda names, so it registers"
            );
        }
    }

    /// The frontend only ever hands us its canonical word forms; the macOS glyphs
    /// (⌫ ⎋ ↩) are a display concern that must never reach an accelerator, since
    /// they'd fall through to the "unknown, use as-is" branch and produce garbage
    /// like `Cmd+⌫`.
    #[test]
    fn test_frontend_shortcut_to_accelerator_canonical_key_names() {
        assert_eq!(
            to_accelerator("⌘Backspace", Platform::MacOs),
            Some("Cmd+Backspace".to_string())
        );
        assert_eq!(
            to_accelerator("⌘⌥Escape", Platform::MacOs),
            Some("Cmd+Alt+Escape".to_string())
        );
    }

    /// #284: muda reads `Cmd` as Super on GTK, so a Linux menu must never say it. ⌘ there is the
    /// Ctrl chord the frontend dispatches it as (`toPlatformShortcut`), on the bar and in popups.
    #[test]
    fn on_linux_a_command_glyph_becomes_ctrl() {
        for (combo, expected) in [
            ("⌘F", "Ctrl+F"),
            ("⌘⇧P", "Ctrl+Shift+P"),
            ("⌥⌘O", "Alt+Ctrl+O"),
            ("⌘↑", "Ctrl+Up"),
            ("⌘Backspace", "Ctrl+Backspace"),
            ("⌃D", "Ctrl+D"),
        ] {
            assert_eq!(
                to_accelerator(combo, Platform::Linux).as_deref(),
                Some(expected),
                "`{combo}`"
            );
            assert_eq!(
                to_menu_text(combo, Platform::Linux).as_deref(),
                Some(expected),
                "`{combo}`"
            );
        }
    }

    /// ⌃ and ⌘ both land on Ctrl off macOS, so a combo holding both spends ⌃ on Shift, exactly
    /// as `toPlatformShortcut` does: the menu has to say the chord the frontend really binds.
    #[test]
    fn on_linux_a_combo_holding_both_command_glyphs_mirrors_the_frontend() {
        assert_eq!(to_accelerator("⌃⌘C", Platform::Linux).as_deref(), Some("Shift+Ctrl+C"));
    }

    /// What a rebind captured on Linux stores (`formatKeyCombo` off macOS: `+`-joined words,
    /// Ctrl+Alt+Shift+Super order). It has to register as the same chord, ❌ not fall through to
    /// the "unknown key" branch, which once made every Linux rebind display-only.
    #[test]
    fn a_combo_captured_on_linux_registers_as_typed() {
        for (combo, expected) in [
            ("Ctrl+F", "Ctrl+F"),
            ("Ctrl+Shift+P", "Ctrl+Shift+P"),
            ("Ctrl+Alt+Shift+Super+A", "Ctrl+Alt+Shift+Super+A"),
            ("Alt+F5", "Alt+F5"),
            ("Ctrl+↑", "Ctrl+Up"),
            ("Ctrl+Backspace", "Ctrl+Backspace"),
            ("Ctrl+=", "Ctrl+="),
            // A Super chord the user really pressed stays one.
            ("Super+E", "Super+E"),
        ] {
            assert_eq!(
                to_accelerator(combo, Platform::Linux).as_deref(),
                Some(expected),
                "`{combo}`"
            );
        }
    }

    /// The two refusals hold for the Linux form too: Shift alone isn't a command modifier, and `+`
    /// (the key, after the `Ctrl+` separator) has no muda `Code`. Both stay labels.
    #[test]
    fn a_combo_captured_on_linux_keeps_both_refusals() {
        for (combo, text) in [("Shift+F4", "Shift+F4"), ("Ctrl++", "Ctrl++"), ("F2", "F2")] {
            assert_eq!(to_accelerator(combo, Platform::Linux), None, "`{combo}`");
            assert_eq!(to_menu_text(combo, Platform::Linux).as_deref(), Some(text), "`{combo}`");
        }
    }

    #[test]
    fn test_frontend_shortcut_to_accelerator_empty() {
        assert_eq!(to_accelerator("", Platform::MacOs), None);
        assert_eq!(to_menu_text("", Platform::MacOs), None);
    }
}
