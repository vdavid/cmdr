//! The `cmdr-cli://open?…` URL the `cmdr` script sends, and the one parser that turns it into a
//! pane move.
//!
//! The scheme is ❌ NOT registered with LaunchServices (no `CFBundleURLTypes`), on purpose: the
//! script names our bundle with `open -a`, which delivers an unclaimed scheme just fine, while a
//! web page or another app firing a bare `cmdr-cli://` link gets "no application knows how to
//! open URL" from the OS. `DETAILS.md` § "The transport".
//!
//! Whatever arrives here is still untrusted input: it can only ever become a navigation, and it is
//! validated hard before it gets that far.

use std::path::{Component, PathBuf};

use tauri::Url;

use crate::tools::{GoToTarget, PaneChoice};

/// The URL scheme the `cmdr` script speaks. ❗ Must match `SCHEME` in `resources/bin/cmdr`.
pub(crate) const SCHEME: &str = "cmdr-cli";

/// The only action the scheme has. A URL naming anything else is refused, so the scheme can't
/// quietly grow a second power.
const OPEN_ACTION: &str = "open";

/// The most paths one request may carry. `cmdr *.jpg` in a busy folder is a real use, and every
/// one of them gets selected; past this the request is refused whole rather than truncated, since
/// a partial selection would look like a complete one.
const MAX_PATHS: usize = 10_000;

/// The longest path accepted, in bytes. Generous (macOS's own `PATH_MAX` is 1,024); it only bounds
/// input nobody legitimate sends.
const MAX_PATH_BYTES: usize = 4_096;

/// One `cmdr <path>…` invocation, parsed and validated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CliOpen {
    /// Absolute paths, in the order the user typed them. Never empty.
    pub paths: Vec<PathBuf>,
    /// Which pane, and whether in a new tab.
    pub target: GoToTarget,
}

/// Why a `cmdr-cli://` URL was refused. Logged, never shown: the script validated the same things
/// before sending, so a refusal here means something other than the script built the URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CliUrlRefusal {
    /// Not `cmdr-cli://open`, or carrying parts the script never sends (a path, a port, a user).
    NotAnOpenUrl,
    /// No `path` parameter at all.
    NoPaths,
    /// More than [`MAX_PATHS`].
    TooManyPaths(usize),
    /// A path that's relative, holds a `..`, a NUL, or is absurdly long.
    BadPath(String),
    /// `pane` given twice, or set to something other than `left` / `right`.
    BadPane(String),
    /// `tab` given twice, or set to something other than `new`.
    BadTab(String),
}

/// Parse a `cmdr-cli://open?path=…&pane=left&tab=new` URL.
///
/// `path` repeats, once per path; `pane` and `tab` are optional and may appear once. Unknown
/// parameters are ignored, so an app a version behind the script it's sent from still opens the
/// paths. ❗ Values are percent-decoded by `query_pairs`, which also turns a literal `+` into a
/// space: the script percent-encodes every byte, `+` included, so that can't bite.
pub(crate) fn parse_cli_url(url: &Url) -> Result<CliOpen, CliUrlRefusal> {
    let is_open = url.scheme() == SCHEME
        && url.host_str() == Some(OPEN_ACTION)
        && matches!(url.path(), "" | "/")
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
        && url.fragment().is_none();
    if !is_open {
        return Err(CliUrlRefusal::NotAnOpenUrl);
    }

    let mut paths = Vec::new();
    let mut pane: Option<PaneChoice> = None;
    let mut new_tab: Option<bool> = None;
    for (key, value) in url.query_pairs() {
        match key.as_ref() {
            "path" => {
                if paths.len() == MAX_PATHS {
                    let total = url.query_pairs().filter(|(key, _)| key == "path").count();
                    return Err(CliUrlRefusal::TooManyPaths(total));
                }
                paths.push(validate_path(&value)?);
            }
            "pane" => {
                let choice = match value.as_ref() {
                    "left" => PaneChoice::Left,
                    "right" => PaneChoice::Right,
                    _ => return Err(CliUrlRefusal::BadPane(value.into_owned())),
                };
                if pane.replace(choice).is_some() {
                    return Err(CliUrlRefusal::BadPane(value.into_owned()));
                }
            }
            "tab" => {
                if value != "new" || new_tab.replace(true).is_some() {
                    return Err(CliUrlRefusal::BadTab(value.into_owned()));
                }
            }
            other => log::debug!(target: "cli", "Ignoring unknown `{other}` parameter in a cmdr-cli URL"),
        }
    }
    if paths.is_empty() {
        return Err(CliUrlRefusal::NoPaths);
    }
    Ok(CliOpen {
        paths,
        target: GoToTarget {
            pane: pane.unwrap_or(PaneChoice::Focused),
            new_tab: new_tab.unwrap_or(false),
        },
    })
}

/// An absolute path with no `..`, no NUL, and a sane length. The script resolves every path
/// before sending it, so anything else didn't come from the script.
fn validate_path(raw: &str) -> Result<PathBuf, CliUrlRefusal> {
    let path = PathBuf::from(raw);
    let ok = raw.len() <= MAX_PATH_BYTES
        && !raw.contains('\0')
        && path.is_absolute()
        && !path.components().any(|part| part == Component::ParentDir);
    if ok {
        Ok(path)
    } else {
        Err(CliUrlRefusal::BadPath(raw.chars().take(200).collect()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(url: &str) -> Result<CliOpen, CliUrlRefusal> {
        parse_cli_url(&Url::parse(url).expect("a URL"))
    }

    fn focused() -> GoToTarget {
        GoToTarget {
            pane: PaneChoice::Focused,
            new_tab: false,
        }
    }

    #[test]
    fn a_single_path_opens_in_the_focused_pane() {
        let open = parse("cmdr-cli://open?path=%2FUsers%2Fjane%2Fnotes").expect("an open");
        assert_eq!(open.paths, vec![PathBuf::from("/Users/jane/notes")]);
        assert_eq!(open.target, focused());
    }

    #[test]
    fn every_byte_the_script_encodes_comes_back() {
        // The script percent-encodes every byte, so spaces, `+`, `&`, `=`, `#`, `%`, and UTF-8
        // all arrive as `%XX` and must decode to exactly what the user typed.
        let open = parse("cmdr-cli://open?path=%2F%74mp%2Fa%20b%2Bc%26d%3De%23f%25g%C3%A9").expect("an open");
        assert_eq!(open.paths, vec![PathBuf::from("/tmp/a b+c&d=e#f%gé")]);
    }

    #[test]
    fn paths_keep_their_order() {
        let open = parse("cmdr-cli://open?path=%2Ftmp%2Fb&path=%2Ftmp%2Fa").expect("an open");
        assert_eq!(open.paths, vec![PathBuf::from("/tmp/b"), PathBuf::from("/tmp/a")]);
    }

    #[test]
    fn pane_and_tab_are_honoured() {
        let open = parse("cmdr-cli://open?path=%2Ftmp&pane=right&tab=new").expect("an open");
        assert_eq!(
            open.target,
            GoToTarget {
                pane: PaneChoice::Right,
                new_tab: true
            }
        );
        let open = parse("cmdr-cli://open?pane=left&path=%2Ftmp").expect("an open");
        assert_eq!(open.target.pane, PaneChoice::Left);
    }

    #[test]
    fn unknown_parameters_are_ignored() {
        // A newer script talking to an app a version behind must still open the path.
        let open = parse("cmdr-cli://open?path=%2Ftmp&someday=1").expect("an open");
        assert_eq!(open.paths, vec![PathBuf::from("/tmp")]);
    }

    #[test]
    fn anything_but_the_open_action_is_refused() {
        for url in [
            "cmdr-cli://delete?path=%2Ftmp",
            "cmdr-cli://open/extra?path=%2Ftmp",
            "cmdr-cli://user@open?path=%2Ftmp",
            "cmdr-cli://open:80?path=%2Ftmp",
            "cmdr-cli://open?path=%2Ftmp#frag",
            "https://open?path=%2Ftmp",
            "file:///tmp",
        ] {
            assert_eq!(parse(url), Err(CliUrlRefusal::NotAnOpenUrl), "{url}");
        }
    }

    #[test]
    fn a_url_with_no_path_is_refused() {
        assert_eq!(parse("cmdr-cli://open"), Err(CliUrlRefusal::NoPaths));
        assert_eq!(parse("cmdr-cli://open?pane=left"), Err(CliUrlRefusal::NoPaths));
    }

    #[test]
    fn relative_parent_and_nul_paths_are_refused() {
        for raw in ["tmp%2Fa", "%2Ftmp%2F..%2Fetc", "%2Ftmp%00x", ""] {
            let url = format!("cmdr-cli://open?path={raw}");
            assert!(matches!(parse(&url), Err(CliUrlRefusal::BadPath(_))), "{url}");
        }
    }

    #[test]
    fn an_absurdly_long_path_is_refused() {
        let url = format!("cmdr-cli://open?path=%2F{}", "a".repeat(MAX_PATH_BYTES));
        assert!(matches!(parse(&url), Err(CliUrlRefusal::BadPath(_))));
    }

    #[test]
    fn too_many_paths_are_refused_whole() {
        let query = vec!["path=%2Ftmp%2Fx"; MAX_PATHS + 1].join("&");
        let url = format!("cmdr-cli://open?{query}");
        assert_eq!(parse(&url), Err(CliUrlRefusal::TooManyPaths(MAX_PATHS + 1)));
    }

    #[test]
    fn a_bad_or_repeated_pane_or_tab_is_refused() {
        assert!(matches!(
            parse("cmdr-cli://open?path=%2Ftmp&pane=middle"),
            Err(CliUrlRefusal::BadPane(_))
        ));
        assert!(matches!(
            parse("cmdr-cli://open?path=%2Ftmp&pane=left&pane=right"),
            Err(CliUrlRefusal::BadPane(_))
        ));
        assert!(matches!(
            parse("cmdr-cli://open?path=%2Ftmp&tab=old"),
            Err(CliUrlRefusal::BadTab(_))
        ));
        assert!(matches!(
            parse("cmdr-cli://open?path=%2Ftmp&tab=new&tab=new"),
            Err(CliUrlRefusal::BadTab(_))
        ));
    }
}
