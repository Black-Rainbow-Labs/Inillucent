//! Narrowing a `LIKE` or `GLOB` by the trigrams of its pattern.
//!
//! Invariant: `narrowing_query` returns the query SQLite's `sqlite3Fts5ExprPattern` builds for a
//! pattern, so the rows it finds are a superset of the rows the pattern matches. The pattern is
//! cut at its wildcards, every piece of three or more characters becomes a quoted phrase, and the
//! phrases are all required. A pattern with no such piece narrows nothing, and every row is a
//! candidate. The engine tests the pattern itself against each candidate, so this only has to
//! leave out rows that cannot match.

/// Returns the wildcard characters that cut a pattern into pieces.
///
/// @param glob - whether the pattern is a `GLOB` pattern rather than a `LIKE` pattern
fn wildcards(glob: bool) -> [char; 3] {
    match glob {
        true => ['*', '?', '['],
        false => ['_', '%', '\0'],
    }
}

/// Appends one piece of the pattern to the query when it has three characters or more.
///
/// @param query - the query being built
/// @param piece - the characters between two wildcards
fn push_piece(query: &mut String, piece: &str) {
    if piece.chars().count() < 3 {
        return;
    }
    query.push('"');
    for character in piece.chars() {
        query.push(character);
        if character == '"' {
            query.push('"');
        }
    }
    query.push_str("\" ");
}

/// Returns the query that finds every row a pattern can match, or nothing when the pattern has no
/// piece long enough to narrow by.
///
/// A `GLOB` character class (`[...]`) is skipped as one wildcard: its characters are not a piece.
///
/// @param pattern - the text of the `LIKE` or `GLOB` pattern
/// @param glob - whether the pattern is a `GLOB` pattern
pub(super) fn narrowing_query(pattern: &str, glob: bool) -> Option<String> {
    let specials = wildcards(glob);
    let characters: Vec<char> = pattern.chars().collect();
    let mut query = String::new();
    let mut piece_start = 0usize;
    let mut at = 0usize;
    while at <= characters.len() {
        let wildcard = characters
            .get(at)
            .is_some_and(|character| specials.contains(character));
        if at == characters.len() || wildcard {
            let piece: String = characters
                .get(piece_start..at)
                .unwrap_or_default()
                .iter()
                .collect();
            push_piece(&mut query, &piece);
            if glob && characters.get(at) == Some(&'[') {
                // Skip the class: past its first character, past a `^`, and up to the `]`.
                at = at.saturating_add(2);
                if characters.get(at.saturating_sub(1)) == Some(&'^') {
                    at = at.saturating_add(1);
                }
                while at < characters.len() && characters.get(at) != Some(&']') {
                    at = at.saturating_add(1);
                }
            }
            piece_start = at.saturating_add(1);
        }
        at = at.saturating_add(1);
    }
    (!query.is_empty()).then_some(query)
}

/// Turns the argument of a LIKE or GLOB constraint into the part of the query it adds.
///
/// Answers nothing for a role that is not a pattern. A pattern that has no piece to narrow by, and
/// a NULL pattern, add no part and leave every row a candidate.
///
/// @param role - the role the plan gave the argument, L followed by the column number or G followed by it
/// @param argument - the pattern the statement compared the column with
pub(super) fn narrowed(
    role: &str,
    argument: &inillucent_value::Value<'static>,
) -> Option<Option<(String, Vec<u8>)>> {
    let glob = match role.chars().next() {
        Some('G') => true,
        Some('L') => false,
        _ => return None,
    };
    let column = role.get(1..)?;
    let narrowing = super::text_of(argument)
        .map(|text| String::from_utf8_lossy(&text).into_owned())
        .and_then(|text| narrowing_query(&text, glob));
    Some(narrowing.map(|query| (format!("c{column}"), query.into_bytes())))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `LIKE` pattern is cut at `%` and `_`, and short pieces are dropped.
    #[test]
    fn a_like_pattern_keeps_the_pieces_of_three_characters() {
        assert_eq!(
            narrowing_query("%ello%", false).as_deref(),
            Some("\"ello\" ")
        );
        assert_eq!(
            narrowing_query("abc_defg%hi", false).as_deref(),
            Some("\"abc\" \"defg\" ")
        );
        assert_eq!(narrowing_query("%ab%", false), None);
    }

    /// A `GLOB` pattern is cut at its wildcards and a class is skipped whole.
    #[test]
    fn a_glob_pattern_skips_its_character_classes() {
        assert_eq!(
            narrowing_query("*[hH]ello?wor*", true).as_deref(),
            Some("\"ello\" \"wor\" ")
        );
        assert_eq!(
            narrowing_query("a[^]x]bcd", true).as_deref(),
            Some("\"bcd\" ")
        );
    }

    /// A quote inside a piece is doubled so it stays one phrase.
    #[test]
    fn a_quote_in_a_piece_is_doubled() {
        assert_eq!(
            narrowing_query("a\"bc", false).as_deref(),
            Some("\"a\"\"bc\" ")
        );
    }
}
