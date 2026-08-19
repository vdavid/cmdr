//! GTK mnemonic assignment: which letter of a menu label gets underlined.
//!
//! Linux menus underline one letter per item so it can be reached from the
//! keyboard, and that letter has to be unique within its submenu. Which letters
//! are free depends on the words in the menu, so it depends on the LANGUAGE: a
//! hand-picked English set can't survive translation into nine more, and a
//! translator can't be asked to solve a per-submenu uniqueness puzzle on top of
//! translating. So the letters are allocated here, at build time, from the
//! translated labels.
//!
//! macOS has no mnemonics, so [`Mnemonics::assign`] is a no-op there. The call
//! sites stay identical on both platforms, which is what stops a new menu item
//! from being given one and not the other.

/// The letters already handed out in one submenu.
///
/// Word-initial letters are offered first (that's what people scan for), then
/// every other letter or digit in the label. A label that finds nothing free
/// keeps no mnemonic, which costs one keystroke and nothing else.
#[derive(Default)]
pub(crate) struct Mnemonics {
    /// Lowercased letters already handed out in this submenu.
    taken: Vec<char>,
}

impl Mnemonics {
    /// A fresh allocator, for one submenu.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// `label` with a mnemonic marker if this platform uses them, unchanged
    /// otherwise.
    pub(crate) fn assign(&mut self, label: &str) -> String {
        if cfg!(target_os = "macos") {
            return label.to_string();
        }
        self.mark(label)
    }

    /// `label` with `&` inserted before the first letter still free in this
    /// submenu, or unchanged when every letter it offers is taken.
    ///
    /// Compiled on every platform even though only Linux calls it, so its tests
    /// run in the same suite everyone runs. Splitting the decision
    /// ([`Self::assign`]) from the mechanism is what makes that possible.
    fn mark(&mut self, label: &str) -> String {
        let chars: Vec<char> = label.chars().collect();
        for index in mnemonic_candidates(&chars) {
            let letter = chars[index].to_lowercase().next().unwrap_or(chars[index]);
            if self.taken.contains(&letter) {
                continue;
            }
            self.taken.push(letter);
            let mut marked: String = chars[..index].iter().collect();
            marked.push('&');
            marked.extend(chars[index..].iter());
            return marked;
        }
        label.to_string()
    }
}

/// Character indices in `label` that could carry the mnemonic, best first:
/// every word's first letter in order, then every remaining letter or digit.
/// Non-alphanumerics are never offered, because GTK underlines a character and
/// an underlined comma helps nobody.
fn mnemonic_candidates(label: &[char]) -> Vec<usize> {
    let usable: Vec<usize> = label
        .iter()
        .enumerate()
        .filter(|(_, character)| character.is_alphanumeric())
        .map(|(index, _)| index)
        .collect();
    let word_initial: Vec<usize> = usable
        .iter()
        .copied()
        .filter(|&index| index == 0 || !label[index - 1].is_alphanumeric())
        .collect();
    let rest: Vec<usize> = usable
        .into_iter()
        .filter(|index| !word_initial.contains(index))
        .collect();
    word_initial.into_iter().chain(rest).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mnemonic_lands_on_the_first_free_word_initial() {
        let mut mnemonics = Mnemonics::new();
        assert_eq!(mnemonics.mark("New folder…"), "&New folder…");
        // `N` is taken, so the next label's own initial wins.
        assert_eq!(mnemonics.mark("Delete"), "&Delete");
    }

    #[test]
    fn a_clash_moves_to_the_next_word_before_falling_back_to_a_letter() {
        let mut mnemonics = Mnemonics::new();
        assert_eq!(mnemonics.mark("Copy path"), "&Copy path");
        // `C` is gone, so "Copy filename" takes its SECOND word's initial.
        assert_eq!(mnemonics.mark("Copy filename"), "Copy &filename");
        // Every word initial gone: fall back to a later letter inside the label.
        assert_eq!(mnemonics.mark("Copy"), "C&opy");
    }

    #[test]
    fn the_case_of_the_letter_does_not_decide_uniqueness() {
        // GTK matches the mnemonic case-insensitively, so "Open" and "open" would
        // be the same keystroke.
        let mut mnemonics = Mnemonics::new();
        assert_eq!(mnemonics.mark("Open"), "&Open");
        assert_eq!(mnemonics.mark("orange"), "o&range");
    }

    #[test]
    fn a_label_with_nothing_free_keeps_no_mnemonic() {
        // Costs one keystroke and nothing else; a duplicate marker would cost
        // correctness, since two items would answer the same key.
        let mut mnemonics = Mnemonics::new();
        assert_eq!(mnemonics.mark("ab"), "&ab");
        assert_eq!(mnemonics.mark("ba"), "&ba");
        assert_eq!(mnemonics.mark("aabb"), "aabb");
    }

    #[test]
    fn punctuation_never_carries_the_marker() {
        // GTK underlines the marked character, and an underlined ellipsis or
        // percent sign is unreachable and meaningless.
        let mut mnemonics = Mnemonics::new();
        assert_eq!(mnemonics.mark("75%"), "&75%");
        assert_eq!(mnemonics.mark("…"), "…");
    }

    #[test]
    fn a_non_latin_label_still_gets_a_marker() {
        // The allocator runs on TRANSLATED labels, so it has to work in every
        // script we ship, not just the one it was written in.
        let mut mnemonics = Mnemonics::new();
        assert_eq!(mnemonics.mark("Új mappa…"), "&Új mappa…");
        assert_eq!(mnemonics.mark("打开"), "&打开");
    }
}
