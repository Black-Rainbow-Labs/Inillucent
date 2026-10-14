//! `.mode`: choosing how rows are laid out, and the options that adjust it.
//!
//! Invariant: **a mode is a set of defaults for the same settings the options
//! change, applied in order from left to right.** `.mode csv` sets the column
//! separator, the row separator, the way a value is quoted and whether titles are
//! shown, and leaves every other setting as it was. `.mode box --wrap 10` is
//! `.mode box` and then `--wrap 10`. Printing the mode with no arguments writes
//! the command that would put the settings back, naming only the options that
//! differ from the mode's own.

use crate::dot::word_error;
use crate::render::{BlobQuote, Escape, Layout, Mode, Quote, WordWrap};
use crate::settings::boolean;
use crate::shell::Shell;

/// The names `.mode --list` prints.
const MODE_LIST: &str = "available modes: ascii box c column count csv html insert jatom jobject json line list markdown off psql qbox quote split table tabs tcl batch tty";

/// What choosing a mode sets.
struct Defaults {
    /// The column separator, when the mode sets one.
    column: Option<&'static str>,
    /// The row separator, when the mode sets one.
    row: Option<&'static str>,
    /// What a NULL is written as, when the mode sets it.
    null: Option<&'static str>,
    /// How text is written.
    quote: Quote,
    /// How a blob is written.
    blob: BlobQuote,
    /// Whether titles are shown, when the mode has titles at all.
    titles: Option<bool>,
    /// Whether the outer border is drawn.
    border: bool,
    /// Whether the mode sets the screen width to `auto`.
    screen_auto: bool,
}

/// Returns what a mode sets.
///
/// Measured against the pinned reference: each mode was chosen after every setting
/// had been moved off its default, and what came back changed is what the mode
/// sets. `box`, `column`, `table` and `markdown` leave the separators alone, so
/// the separator in force after `.mode csv` and `.mode box` is the comma.
///
/// @param mode - the mode
fn defaults(mode: Mode) -> Defaults {
    let plain = Defaults {
        column: None,
        row: None,
        null: Some(""),
        quote: Quote::Off,
        blob: BlobQuote::Auto,
        titles: Some(false),
        border: true,
        screen_auto: false,
    };
    match mode {
        Mode::List => Defaults {
            column: Some("|"),
            row: Some("\n"),
            ..plain
        },
        Mode::Ascii => Defaults {
            column: Some("\u{1f}"),
            row: Some("\u{1e}"),
            ..plain
        },
        Mode::Csv => Defaults {
            column: Some(","),
            row: Some("\r\n"),
            quote: Quote::Csv,
            ..plain
        },
        Mode::Tabs => Defaults {
            column: Some("\t"),
            row: Some("\n"),
            quote: Quote::Csv,
            ..plain
        },
        Mode::Quote => Defaults {
            column: Some(","),
            row: Some("\n"),
            null: Some("NULL"),
            quote: Quote::Sql,
            ..plain
        },
        Mode::Tcl => Defaults {
            column: Some(" "),
            row: Some("\n"),
            null: Some("\"\""),
            quote: Quote::Tcl,
            blob: BlobQuote::Tcl,
            ..plain
        },
        Mode::C => Defaults {
            column: Some(","),
            row: Some("\n"),
            null: Some("NULL"),
            quote: Quote::Tcl,
            blob: BlobQuote::Tcl,
            ..plain
        },
        Mode::Jatom => Defaults {
            column: Some(","),
            row: Some("\n"),
            null: Some("null"),
            quote: Quote::Json,
            ..plain
        },
        Mode::Jobject => Defaults {
            row: Some("\n"),
            null: Some("null"),
            quote: Quote::Json,
            titles: None,
            ..plain
        },
        Mode::Json => Defaults {
            null: Some("null"),
            quote: Quote::Json,
            titles: None,
            ..plain
        },
        Mode::Line => Defaults {
            column: Some(": "),
            row: Some("\n"),
            titles: None,
            ..plain
        },
        Mode::Insert => Defaults {
            null: Some("NULL"),
            quote: Quote::Sql,
            ..plain
        },
        Mode::Qbox => Defaults {
            null: Some("NULL"),
            quote: Quote::Sql,
            titles: Some(true),
            ..plain
        },
        Mode::Box | Mode::Column | Mode::Markdown | Mode::Table | Mode::Html => Defaults {
            titles: Some(true),
            ..plain
        },
        Mode::Psql => Defaults {
            titles: Some(true),
            border: false,
            ..plain
        },
        Mode::Split => Defaults {
            screen_auto: true,
            ..plain
        },
        Mode::Count | Mode::Off => Defaults {
            null: None,
            titles: None,
            ..plain
        },
    }
}

/// Reports whether a mode has titles to show or hide.
///
/// @param mode - the mode
fn has_titles(mode: Mode) -> bool {
    defaults(mode).titles.is_some()
}

/// Puts a mode's defaults into a layout.
///
/// @param layout - the settings
/// @param mode - the mode chosen
/// @param headers_chosen - what `.headers` last set, which wins over the mode
pub fn apply_mode(layout: &mut Layout, mode: Mode, headers_chosen: Option<bool>) {
    let set = defaults(mode);
    layout.mode = mode;
    if let Some(column) = set.column {
        layout.separator = column.to_string();
    }
    if let Some(row) = set.row {
        layout.row_separator = row.to_string();
    }
    if let Some(null) = set.null {
        layout.null = null.to_string();
    }
    layout.quote = set.quote;
    layout.title_quote = match mode {
        Mode::Quote => Some(Quote::Sql),
        Mode::Csv | Mode::Tabs => Some(Quote::Csv),
        Mode::Tcl | Mode::C => Some(Quote::Tcl),
        Mode::Jatom => Some(Quote::Json),
        _ => None,
    };
    layout.blob_quote = set.blob;
    layout.border = set.border;
    if set.screen_auto {
        layout.screen_width = None;
    }
    if let Some(shown) = set.titles {
        layout.headers = headers_chosen.unwrap_or(shown);
    } else if let Some(chosen) = headers_chosen {
        layout.headers = chosen;
    }
}

/// Returns the settings a mode has before anything is changed.
///
/// @param mode - the mode
fn pristine(mode: Mode) -> Layout {
    let mut layout = Layout::default();
    apply_mode(&mut layout, mode, None);
    layout
}

/// Writes a string the way the reference prints one in a command: between double
/// quotes, a backslash before a quote or a backslash, `\n`, `\r` and `\t` for
/// those, and three octal digits for any other control character.
///
/// @param text - the text
pub fn c_quoted(text: &str) -> String {
    let mut out = String::from("\"");
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            control if u32::from(control) < 0x20 => {
                out.push_str(&format!("\\{:03o}", u32::from(control)));
            }
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

/// Returns the name a tag or a mode goes by when the settings are described.
///
/// @param layout - the settings
fn described_name(layout: &Layout) -> &'static str {
    layout.mode.name()
}

/// Writes `--limits` the way the reference does.
///
/// @param layout - the settings
fn limits_text(layout: &Layout) -> String {
    match (layout.line_limit, layout.char_limit, layout.title_limit) {
        (0, 0, 0) => "off".to_string(),
        (5, 300, 20) => "on".to_string(),
        (lines, characters, titles) => format!("{lines},{characters},{titles}"),
    }
}

/// Writes the screen width setting.
///
/// @param layout - the settings
fn screen_text(layout: &Layout) -> String {
    layout
        .screen_width
        .map_or_else(|| "auto".to_string(), |width| width.to_string())
}

/// Writes the widths setting.
///
/// @param layout - the settings
fn widths_text(layout: &Layout) -> String {
    layout
        .widths
        .iter()
        .map(i32::to_string)
        .collect::<Vec<String>>()
        .join(",")
}

/// Writes a boolean setting.
///
/// @param on - the value
fn on_off(on: bool) -> &'static str {
    if on {
        "on"
    } else {
        "off"
    }
}

/// Returns the command that describes the settings.
///
/// With `verbose` every option is named. Otherwise only an option that differs from
/// what the mode sets is, and the options that only a mode that draws columns
/// reads (`--wrap`, `--ww`) only for such a mode.
///
/// @param layout - the settings
/// @param verbose - whether to name every option
/// @param once - whether the settings revert after the next statement
pub fn describe(layout: &Layout, verbose: bool, once: bool) -> String {
    let mut base = pristine(layout.mode);
    // The reference names the blob quoting of `tcl` and `c`, and the table name of
    // `insert`, whatever they are, because its own comparison starts from the
    // plain settings for those two.
    base.blob_quote = BlobQuote::Auto;
    if layout.mode == Mode::Insert {
        base.table = "tab".to_string();
    }
    let mut out = String::from(".mode");
    if once {
        out.push_str(" --once");
    }
    out.push(' ');
    out.push_str(described_name(layout));
    let mut add = |differs: bool, text: String| {
        if verbose || differs {
            out.push(' ');
            out.push_str(&text);
        }
    };
    add(
        layout.align != base.align,
        format!("--align {}", c_quoted(&layout.align)),
    );
    add(
        layout.border != base.border,
        format!("--border {}", on_off(layout.border)),
    );
    add(
        layout.blob_quote != base.blob_quote,
        format!("--blob-quote {}", layout.blob_quote.name()),
    );
    add(
        defaults(layout.mode).column.is_some() && layout.separator != base.separator,
        format!("--colsep {}", c_quoted(&layout.separator)),
    );
    add(
        layout.escape != base.escape,
        format!("--escape {}", escape_name(layout.escape)),
    );
    add(
        limits_text(layout) != limits_text(&base),
        format!("--limits {}", limits_text(layout)),
    );
    add(
        layout.multi_insert != base.multi_insert,
        format!("--multiinsert {}", layout.multi_insert),
    );
    add(
        has_null(layout.mode) && layout.null != base.null,
        format!("--null {}", c_quoted(&layout.null)),
    );
    add(
        layout.quote != base.quote,
        format!("--quote {}", layout.quote.printed()),
    );
    add(
        defaults(layout.mode).row.is_some() && layout.row_separator != base.row_separator,
        format!("--rowsep {}", c_quoted(&layout.row_separator)),
    );
    add(
        layout.screen_width != base.screen_width,
        format!("--sw {}", screen_text(layout)),
    );
    add(
        layout.table != base.table,
        format!("--tablename {}", c_quoted(&layout.table)),
    );
    add(
        layout.text_jsonb != base.text_jsonb,
        format!("--textjsonb {}", on_off(layout.text_jsonb)),
    );
    if has_titles(layout.mode) {
        add(
            layout.headers != base.headers,
            format!("--titles {}", on_off(layout.headers)),
        );
    }
    add(
        layout.widths != base.widths,
        format!("--widths {}", c_quoted(&widths_text(layout))),
    );
    if verbose {
        out.push_str(&format!(
            " --wordwrap {}",
            on_off(layout.word_wrap == WordWrap::On)
        ));
        if layout.wrap != 0 {
            out.push_str(&format!(" --wrap {}", layout.wrap));
        }
    } else if layout.mode.is_columnar() {
        if layout.wrap != 0 {
            out.push_str(&format!(" --wrap {}", layout.wrap));
        }
        if layout.word_wrap == WordWrap::On {
            out.push_str(" --ww");
        }
    }
    out
}

/// Reports whether a mode writes NULL as text it can be asked to change.
///
/// @param mode - the mode
fn has_null(mode: Mode) -> bool {
    !matches!(mode, Mode::Count | Mode::Off)
}

/// Returns the name an escape setting prints as.
///
/// @param escape - the setting
fn escape_name(escape: Escape) -> &'static str {
    match escape {
        Escape::Auto => "auto",
        Escape::Off => "off",
        Escape::Ascii => "ascii",
        Escape::Symbol => "symbol",
    }
}

/// The mode settings a `--once` puts back.
pub struct Snapshot {
    /// The settings as they were.
    pub layout: Layout,
    /// What `.headers` had chosen.
    pub headers_chosen: Option<bool>,
}

/// `.mode ?MODE? ?OPTIONS?`.
///
/// @param shell - the shell
/// @param arguments - the words after the command
pub fn mode(shell: &mut Shell, arguments: &[&str]) {
    if arguments.is_empty() {
        let text = describe(&shell.layout, false, shell.mode_once.is_some());
        shell.say(&text);
        return;
    }
    let mut verbose = false;
    let mut index = 0usize;
    let mut chose_a_mode = false;
    let mut named_a_table = false;
    while let Some(word) = arguments.get(index).copied() {
        index += 1;
        let step = if word.starts_with('-') {
            option(
                shell,
                arguments,
                &mut index,
                word,
                &mut verbose,
                chose_a_mode,
            )
        } else if !chose_a_mode {
            chose_a_mode = true;
            choose(shell, word, index)
        } else if shell.layout.mode == Mode::Insert {
            // The table name of the older `.mode insert TABLE`; a second word is
            // not read.
            if named_a_table {
                word_error(shell, index, "bad argument");
                Err(Stop::Help)
            } else {
                named_a_table = true;
                shell.layout.table = word.to_string();
                Ok(())
            }
        } else {
            word_error(shell, index, "bad option");
            Err(Stop::Help)
        };
        if let Err(stop) = step {
            if stop == Stop::Help {
                let place = shell.error_location();
                shell.complain(&format!("{place} Use \".help .mode\" for more info"));
            }
            return;
        }
    }
    if verbose {
        let text = describe(&shell.layout, true, shell.mode_once.is_some());
        shell.say(&text);
    }
}

/// How a failed option ended the command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stop {
    /// The command stops and points at the help.
    Help,
    /// The command stops; the message said enough.
    Done,
}

/// Chooses the mode a word names.
///
/// @param shell - the shell
/// @param word - the name
/// @param index - which word of the command it is
fn choose(shell: &mut Shell, word: &str, index: usize) -> Result<(), Stop> {
    let lowered = word.to_ascii_lowercase();
    if let Some(tagged) = shell.mode_tags.get(&lowered).cloned() {
        shell.layout = tagged;
        return Ok(());
    }
    let resolved = match lowered.as_str() {
        "batch" => Some(Mode::List),
        "tty" => None,
        other => Mode::from_name(other),
    };
    if lowered == "tty" {
        let headers = shell.headers_chosen;
        apply_mode(&mut shell.layout, Mode::Qbox, headers);
        shell.layout.escape = Escape::Auto;
        shell.layout.quote = Quote::Relaxed;
        shell.layout.line_limit = 5;
        shell.layout.char_limit = 300;
        shell.layout.title_limit = 20;
        shell.layout.multi_insert = 3000;
        shell.layout.screen_width = None;
        shell.layout.text_jsonb = true;
        return Ok(());
    }
    match resolved {
        Some(mode) => {
            let headers = shell.headers_chosen;
            apply_mode(&mut shell.layout, mode, headers);
            Ok(())
        }
        None => {
            word_error(shell, index, "unknown mode");
            Err(Stop::Help)
        }
    }
}

/// Reads one option and its value.
///
/// @param shell - the shell
/// @param arguments - every word after `.mode`
/// @param index - how many words have been read, advanced past the value
/// @param word - the option
/// @param verbose - set when `-v` is given
/// @param chose_a_mode - whether a mode name came before this option
fn option(
    shell: &mut Shell,
    arguments: &[&str],
    index: &mut usize,
    word: &str,
    verbose: &mut bool,
    chose_a_mode: bool,
) -> Result<(), Stop> {
    let name = match word {
        "-v" | "--verbose" => "--verbose".to_string(),
        other if other.starts_with("--") => other.to_string(),
        other => format!("-{other}"),
    };
    let at = *index;
    match name.as_str() {
        "--ww" => shell.layout.word_wrap = WordWrap::On,
        "--once" => {
            if shell.mode_once.is_none() {
                shell.mode_once = Some(Snapshot {
                    layout: shell.layout.clone(),
                    headers_chosen: shell.headers_chosen,
                });
            }
        }
        "--reset" => {
            let mode = shell.layout.mode;
            shell.layout = Layout::default();
            apply_mode(&mut shell.layout, mode, shell.headers_chosen);
        }
        "--verbose" => *verbose = true,
        "--list" => shell.say(MODE_LIST),
        "--noquote" => shell.layout.quote = Quote::Off,
        "--quote" => return quote_option(shell, arguments, index, chose_a_mode),
        _ => {
            let Some(value) = arguments.get(*index).copied() else {
                return if is_known_with_value(&name) {
                    word_error(shell, at, "missing argument");
                    Err(Stop::Done)
                } else {
                    word_error(shell, at, "bad option");
                    Err(Stop::Help)
                };
            };
            if !is_known_with_value(&name) {
                word_error(shell, at, "bad option");
                return Err(Stop::Help);
            }
            *index += 1;
            return valued_option(shell, &name, value, *index);
        }
    }
    Ok(())
}

/// Reports whether an option takes a value.
///
/// @param name - the option, with two dashes
fn is_known_with_value(name: &str) -> bool {
    matches!(
        name,
        "--align"
            | "--blob-quote"
            | "--border"
            | "--charlimit"
            | "--colsep"
            | "--escape"
            | "--linelimit"
            | "--limits"
            | "--multiinsert"
            | "--null"
            | "--rowsep"
            | "--sw"
            | "--screenwidth"
            | "--tablename"
            | "--tag"
            | "--textjsonb"
            | "--title"
            | "--titles"
            | "--titlelimit"
            | "--widths"
            | "--wordwrap"
            | "--wrap"
    )
}

/// `--quote ?ARG?`: with no argument, SQL quoting.
///
/// **The next word is a mode name, and not the quoting, when no mode has been
/// chosen yet** (bug hunt 10). `.mode --quote csv` turns quoting on and then
/// selects `csv`, and `.mode --quote list` selects `list`; both used to read the
/// mode name as the quoting and fail or do nothing. `off` is always the quoting,
/// and once a mode is chosen any word is. This is `dotCmdMode`'s rule in sqlite3.
///
/// @param shell - the shell
/// @param arguments - every word after `.mode`
/// @param index - how many words have been read, advanced past the value
/// @param chose_a_mode - whether a mode name came before `--quote`
fn quote_option(
    shell: &mut Shell,
    arguments: &[&str],
    index: &mut usize,
    chose_a_mode: bool,
) -> Result<(), Stop> {
    match arguments.get(*index).copied() {
        Some(value)
            if !value.starts_with('-')
                && (chose_a_mode || value == "off" || !names_a_mode(shell, value)) =>
        {
            *index += 1;
            match Quote::from_name(value) {
                Some(style) => {
                    set_quote(shell, style);
                    Ok(())
                }
                None => {
                    left_error(shell, *index, "unknown");
                    let place = shell.error_location();
                    shell.complain(&format!(
                        "{place} should be off, on, sql, csv, html, tcl, json, or relaxed"
                    ));
                    Err(Stop::Done)
                }
            }
        }
        _ => {
            set_quote(shell, Quote::Sql);
            Ok(())
        }
    }
}

/// Reports whether a word names an output mode, a saved mode tag, `batch` or `tty`.
///
/// @param shell - the shell, which holds the saved tags
/// @param word - the word
fn names_a_mode(shell: &Shell, word: &str) -> bool {
    let lowered = word.to_ascii_lowercase();
    shell.mode_tags.contains_key(&lowered)
        || matches!(lowered.as_str(), "batch" | "tty")
        || Mode::from_name(&lowered).is_some()
}

/// Sets the quoting, and the text for NULL that goes with it when none was chosen.
///
/// @param shell - the shell
/// @param style - the quoting
fn set_quote(shell: &mut Shell, style: Quote) {
    shell.layout.quote = style;
    if matches!(style, Quote::Sql | Quote::Relaxed)
        && shell.layout.null == pristine(shell.layout.mode).null
    {
        shell.layout.null = "NULL".to_string();
    }
}

/// Reports a mistake in a value with the reason written to the left of the caret.
///
/// @param shell - the shell
/// @param word - which word
/// @param reason - what is wrong
fn left_error(shell: &mut Shell, word: usize, reason: &str) {
    let place = shell.error_location();
    let column = shell.dot_offsets.get(word).copied().unwrap_or(0);
    let typed = shell.dot_line.clone();
    shell.complain(&format!("{place} {typed}"));
    let tag = format!("{reason} ---");
    let spaces = column.saturating_sub(tag.chars().count());
    shell.complain(&format!("{place} {}{tag}^", " ".repeat(spaces)));
}

/// Applies an option that has a value.
///
/// @param shell - the shell
/// @param name - the option
/// @param value - its value
/// @param at - which word the value is
fn valued_option(shell: &mut Shell, name: &str, value: &str, at: usize) -> Result<(), Stop> {
    match name {
        "--align" => {
            let valid: String = value
                .chars()
                .take_while(|c| matches!(c, 'L' | 'C' | 'R' | 'l' | 'c' | 'r'))
                .collect::<String>()
                .to_ascii_uppercase();
            let all_valid = valid.chars().count() == value.chars().count();
            // The letters before the first bad one are kept, as the reference keeps them.
            shell.layout.align = valid;
            if !all_valid {
                word_error(shell, at, "bad alignment string");
                let place = shell.error_location();
                shell.complain(&format!(
                    "{place} Should contain only characters L, C, and R."
                ));
                return Err(Stop::Done);
            }
        }
        "--blob-quote" => {
            if let Some(style) = BlobQuote::from_name(value) {
                shell.layout.blob_quote = style;
            }
        }
        "--border" => shell.layout.border = boolean(shell, value),
        "--charlimit" => shell.layout.char_limit = unsigned(value),
        "--linelimit" => shell.layout.line_limit = unsigned(value),
        "--titlelimit" => shell.layout.title_limit = unsigned(value),
        "--colsep" => shell.layout.separator = value.to_string(),
        "--rowsep" => shell.layout.row_separator = value.to_string(),
        "--null" => shell.layout.null = value.to_string(),
        "--tablename" => shell.layout.table = value.to_string(),
        "--multiinsert" => shell.layout.multi_insert = unsigned(value),
        "--textjsonb" => shell.layout.text_jsonb = boolean(shell, value),
        "--wordwrap" => {
            shell.layout.word_wrap = if boolean(shell, value) {
                WordWrap::On
            } else {
                WordWrap::Off
            }
        }
        "--wrap" => shell.layout.wrap = unsigned(value),
        "--sw" | "--screenwidth" => {
            shell.layout.screen_width = if value.eq_ignore_ascii_case("auto") {
                None
            } else {
                Some(unsigned(value))
            }
        }
        "--escape" => match value.to_ascii_lowercase().as_str() {
            "off" => shell.layout.escape = Escape::Off,
            "ascii" => shell.layout.escape = Escape::Ascii,
            "symbol" => shell.layout.escape = Escape::Symbol,
            _ => {
                word_error(shell, at, "unknown escape type");
                let place = shell.error_location();
                shell.complain(&format!("{place} should be off, ascii, or symbol"));
                return Err(Stop::Done);
            }
        },
        "--title" | "--titles" => return titles_option(shell, value, at),
        "--limits" => return limits_option(shell, value, at),
        "--widths" => return widths_option(shell, value),
        "--tag" => {
            shell
                .mode_tags
                .insert(value.to_ascii_lowercase(), shell.layout.clone());
        }
        _ => {}
    }
    Ok(())
}

/// Reads a number the way `atoi` does, with a negative one as zero.
///
/// @param value - the argument
fn unsigned(value: &str) -> usize {
    usize::try_from(crate::dot::c_atoi(value)).unwrap_or(0)
}

/// `--title ARG`: whether titles are shown and how they are written.
///
/// @param shell - the shell
/// @param value - the argument
/// @param at - which word it is
fn titles_option(shell: &mut Shell, value: &str, at: usize) -> Result<(), Stop> {
    match value.to_ascii_lowercase().as_str() {
        "off" => shell.layout.headers = false,
        "on" | "plain" => {
            shell.layout.headers = true;
            if value.eq_ignore_ascii_case("plain") {
                shell.layout.title_quote = Some(Quote::Off);
            }
        }
        other => match Quote::from_name(other) {
            Some(style) if other != "relaxed" => {
                shell.layout.headers = true;
                shell.layout.title_quote = Some(style);
            }
            _ => {
                word_error(shell, at, "bad --titles value");
                let place = shell.error_location();
                shell.complain(&format!(
                    "{place} should be off, on, plain, sql, csv, html, tcl, or json"
                ));
                return Err(Stop::Done);
            }
        },
    }
    Ok(())
}

/// `--limits L,C,T`, `on` or `off`.
///
/// @param shell - the shell
/// @param value - the argument
/// @param at - which word it is
fn limits_option(shell: &mut Shell, value: &str, at: usize) -> Result<(), Stop> {
    let parts: Vec<&str> = value.split(',').collect();
    let numbers: Option<Vec<usize>> = parts
        .iter()
        .map(|part| part.trim().parse::<usize>().ok())
        .collect();
    match (value, numbers.as_deref()) {
        ("off", _) => {
            shell.layout.line_limit = 0;
            shell.layout.char_limit = 0;
            shell.layout.title_limit = 0;
        }
        ("on", _) => {
            shell.layout.line_limit = 5;
            shell.layout.char_limit = 300;
            shell.layout.title_limit = 20;
        }
        (_, Some([lines, characters])) => {
            shell.layout.line_limit = *lines;
            shell.layout.char_limit = *characters;
        }
        (_, Some([lines, characters, titles])) => {
            shell.layout.line_limit = *lines;
            shell.layout.char_limit = *characters;
            shell.layout.title_limit = *titles;
        }
        _ => {
            left_error(shell, at, "bad argument");
            let place = shell.error_location();
            shell.complain(&format!(
                "{place} Should be \"L,C,T\" where L, C and T are unsigned integers"
            ));
            return Err(Stop::Done);
        }
    }
    Ok(())
}

/// `--widths LIST`: a comma separated list of integers.
///
/// @param shell - the shell
/// @param value - the argument
fn widths_option(shell: &mut Shell, value: &str) -> Result<(), Stop> {
    if value.is_empty() {
        shell.layout.widths.clear();
        return Ok(());
    }
    let parsed: Option<Vec<i32>> = value
        .split(',')
        .map(|part| part.trim().parse::<i32>().ok())
        .collect();
    match parsed {
        Some(widths) => {
            shell.layout.widths = widths;
            Ok(())
        }
        None => {
            let place = shell.error_location();
            shell.complain(&format!(
                "{place} should be a comma-separated list if integers"
            ));
            Err(Stop::Done)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A mode's own settings are not named when the mode is described.
    #[test]
    fn a_plain_mode_describes_as_its_name() {
        let mut layout = Layout::default();
        apply_mode(&mut layout, Mode::Box, None);
        assert_eq!(describe(&layout, false, false), ".mode box");
        layout.null = "X".to_string();
        assert_eq!(describe(&layout, false, false), ".mode box --null \"X\"");
        layout.wrap = 10;
        layout.word_wrap = WordWrap::On;
        assert_eq!(
            describe(&layout, false, false),
            ".mode box --null \"X\" --wrap 10 --ww"
        );
    }

    /// The defaults of each mode match what the reference reports for it.
    #[test]
    fn csv_sets_its_separators_and_quoting() {
        let mut layout = Layout::default();
        apply_mode(&mut layout, Mode::Csv, None);
        assert_eq!(layout.separator, ",");
        assert_eq!(layout.row_separator, "\r\n");
        assert_eq!(layout.quote, Quote::Csv);
        assert!(!layout.headers);
        assert_eq!(c_quoted("\u{1f}"), "\"\\037\"");
    }
}
