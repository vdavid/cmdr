package com.getcmdr.idea.features.i18n

/**
 * What a key site reads as once folded: the catalog text in curly quotes, so it reads as a fold rather than as a string
 * literal someone typed.
 *
 * Deliberately close to the raw message:
 * - ICU's doubled apostrophes collapse (`Here''s` becomes `Here's`), because that doubling is escaping, not copy. See
 *   `apps/desktop/src/lib/intl/CLAUDE.md` for why the catalog doubles them.
 * - `{countText}` placeholders and `<tag>` markers stay verbatim. Seeing where the variables go is half the point.
 * - Newlines collapse to one space, since a fold is one line by definition.
 * - **Nothing is ever truncated.** The whole sentence is what makes the fold worth having; a long line is what
 *   horizontal scrolling is for.
 */
internal fun foldedMessage(message: String): String =
    OPEN_QUOTE + LINE_BREAK.replace(message.replace(DOUBLED_APOSTROPHE, "'"), " ").trim() + CLOSE_QUOTE

private const val OPEN_QUOTE = '“'
private const val CLOSE_QUOTE = '”'
private const val DOUBLED_APOSTROPHE = "''"

/** A line break plus whatever indentation wrapped with it, so a wrapped message reads as one sentence. */
private val LINE_BREAK = Regex("""\s*\R\s*""")
