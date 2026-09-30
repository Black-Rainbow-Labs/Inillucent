//! `chunk_text`, the table function that cuts a document into windows.
//!
//! Invariant: the windows cover the text in order with no gap, each window is at most `size`
//! characters, no window splits a character, the heading is on every chunk, and an argument that
//! cannot produce windows is refused with a message that names it. Every case here goes through
//! SQL, including the correlated form `FROM doc AS d, chunk_text(d.body, ...)`, because that is how
//! the function is used.

use inillucent_compat::facade::{Connection, Database};
use inillucent_value::Value;

/// Opens a database file of this test's own, under the gitignored root.
fn connect() -> Connection {
    let root = inillucent_compat::workspace_root().join("_agent_output/chunk_text");
    let _ = std::fs::create_dir_all(&root);
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let serial = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path = root.join(format!("{}-{serial}.rdb", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let database = Database::open(path).expect("opens");
    Box::leak(Box::new(database)).session().expect("connects")
}

/// One row of `chunk_text`: `seq`, `chunk`, `start`, `length`.
#[derive(Debug, PartialEq)]
struct Chunk {
    seq: i64,
    chunk: String,
    start: i64,
    length: i64,
}

/// Returns the text of a value, or an empty string for anything that is not text.
///
/// @param value - a result column
fn text(value: Option<&Value<'static>>) -> String {
    value
        .and_then(Value::as_text)
        .map(|held| String::from_utf8_lossy(held.raw()).into_owned())
        .unwrap_or_default()
}

/// Runs `chunk_text` over a text with the arguments given and returns every row.
///
/// @param connection - the connection to ask
/// @param arguments - the SQL after the function's opening parenthesis, such as `'abc', 10, 2`
fn chunks(connection: &Connection, arguments: &str) -> Vec<Chunk> {
    let sql = format!("SELECT seq, chunk, start, length FROM chunk_text({arguments}) ORDER BY seq");
    connection
        .query(&sql)
        .unwrap_or_else(|error| panic!("{sql} failed: {}", error.message()))
        .iter()
        .map(|row| Chunk {
            seq: row.first().and_then(Value::as_integer).unwrap_or(-1),
            chunk: text(row.get(1)),
            start: row.get(2).and_then(Value::as_integer).unwrap_or(-1),
            length: row.get(3).and_then(Value::as_integer).unwrap_or(-1),
        })
        .collect()
}

/// Returns the refusal a query gives, as one string, and checks its status is `invalid_state`.
///
/// @param connection - the connection to ask
/// @param arguments - the arguments after the opening parenthesis
fn refusal(connection: &Connection, arguments: &str) -> String {
    let error = connection
        .query(&format!("SELECT * FROM chunk_text({arguments})"))
        .expect_err("the call must be refused");
    assert!(
        error.requirement().is_some(),
        "the refusal must carry the requirement marker that the driver reports as invalid_state: {error:?}"
    );
    error.message().to_string()
}

/// A text shorter than `size` is one chunk, and `NULL` or empty text is none.
#[test]
fn a_short_text_is_one_chunk() {
    let connection = connect();
    let rows = chunks(&connection, "'hello world', 900");
    assert_eq!(
        rows,
        vec![Chunk {
            seq: 0,
            chunk: "hello world".to_string(),
            start: 0,
            length: 11
        }]
    );
    assert!(chunks(&connection, "NULL, 900").is_empty());
    assert!(chunks(&connection, "'', 900").is_empty());
}

/// The size defaults to 900 characters, and `text` alone is a valid call.
#[test]
fn the_size_defaults_to_nine_hundred() {
    let connection = connect();
    let rows = chunks(&connection, "replace(hex(zeroblob(1000)), '0', 'a')");
    let lengths: Vec<i64> = rows.iter().map(|row| row.length).collect();
    assert_eq!(lengths, vec![900, 900, 200]);
}

/// A paragraph break, a sentence end and a run of whitespace each end a window where the design says.
#[test]
fn boundaries_end_a_window_in_order_of_preference() {
    let connection = connect();
    let paragraph = chunks(
        &connection,
        "'aaaaaaaa' || char(10) || char(10) || 'bbbbbbbbbbbbbbbbbbbb', 10",
    );
    assert_eq!(paragraph[0].chunk, "aaaaaaaa\n\n");
    let sentence = chunks(&connection, "'aaaaaaa. bbbbbbbbbbbb', 10");
    assert_eq!(sentence[0].chunk, "aaaaaaa. ");
    let space = chunks(&connection, "'aa bb cc dd ee ff', 10");
    assert_eq!(space[0].chunk, "aa bb cc ");
    let hard = chunks(&connection, "'abcdefghijkl', 5");
    let cut: Vec<&str> = hard.iter().map(|row| row.chunk.as_str()).collect();
    assert_eq!(cut, vec!["abcde", "fghij", "kl"]);
}

/// The overlap repeats the end of one window at the start of the next, starting at a word.
#[test]
fn the_overlap_repeats_the_end_of_the_previous_window() {
    let connection = connect();
    let text = "one two three four five six seven eight nine ten";
    let rows = chunks(&connection, &format!("'{text}', 20, 8"));
    assert!(rows.len() > 2, "{rows:?}");
    for pair in rows.windows(2) {
        let (first, second) = (&pair[0], &pair[1]);
        assert!(
            second.start <= first.start + first.length,
            "a gap: {pair:?}"
        );
        assert!(second.start > first.start);
        let repeated = &text[second.start as usize..(first.start + first.length) as usize];
        assert!(
            first.chunk.ends_with(repeated) && second.chunk.starts_with(repeated),
            "the overlap {repeated:?} is not shared by {pair:?}"
        );
        assert!(!second.chunk.starts_with(' '));
    }
}

/// Windows of multibyte text are counted in characters and never split a character.
#[test]
fn multibyte_text_is_counted_in_characters() {
    let connection = connect();
    let text = "é😀é😀é😀é😀é😀é😀é😀é😀";
    let rows = chunks(&connection, &format!("'{text}', 5, 2"));
    assert!(rows.len() > 1);
    for row in &rows {
        assert!(row.chunk.chars().count() <= 5, "{row:?}");
        assert_eq!(row.length as usize, row.chunk.chars().count());
    }
    let whole = chunks(&connection, &format!("'{text}', 5, 0"));
    let joined: String = whole.iter().map(|row| row.chunk.as_str()).collect();
    assert_eq!(joined, text);
}

/// The heading and a blank line start every chunk, and `NULL` writes none.
#[test]
fn the_heading_is_on_every_chunk() {
    let connection = connect();
    let rows = chunks(&connection, "'aaaa bbbb cccc dddd', 9, 0, 'Subject: hi'");
    assert!(rows.len() > 1);
    for row in &rows {
        assert!(row.chunk.starts_with("Subject: hi\n\n"), "{row:?}");
        assert_eq!(
            row.chunk.chars().count(),
            "Subject: hi\n\n".chars().count() + row.length as usize,
            "the heading is not counted in length"
        );
    }
    let bare = chunks(&connection, "'aaaa bbbb', 9, 0");
    assert!(!bare[0].chunk.contains('\n'));
}

/// Every refusal has status `invalid_state` and a message naming the argument.
#[test]
fn a_bad_argument_is_refused_and_named() {
    let connection = connect();
    assert!(refusal(&connection, "'abc', 0").contains("size"));
    assert!(refusal(&connection, "'abc', -5").contains("size"));
    assert!(refusal(&connection, "'abc', 10, -1").contains("overlap"));
    let said = refusal(&connection, "'abc', 10, 10");
    assert!(said.contains("overlap") && said.contains("size"), "{said}");
    assert!(refusal(&connection, "'abc', 10, 11").contains("overlap"));
}

/// The correlated form reads a column of an earlier table in the `FROM` list, the way `json_each(d.x)` does.
///
/// Each document is chunked on its own, and a document with a `NULL` body produces no rows.
#[test]
fn a_table_function_can_read_the_earlier_table() {
    let connection = connect();
    connection
        .execute("CREATE TABLE doc (id INTEGER PRIMARY KEY, subject TEXT, body TEXT)")
        .expect("creates");
    connection
        .execute(
            "INSERT INTO doc VALUES (1, 'first', 'alpha beta gamma delta epsilon'), \
             (2, 'second', 'one two three'), (3, 'third', NULL)",
        )
        .expect("fills");
    let rows = connection
        .query(
            "SELECT d.id, c.seq, c.chunk FROM doc AS d, \
             chunk_text(d.body, 12, 0, 'Subject: ' || d.subject) AS c ORDER BY d.id, c.seq",
        )
        .expect("the correlated form works");
    let seen: Vec<(i64, i64, String)> = rows
        .iter()
        .map(|row| {
            (
                row.first().and_then(Value::as_integer).unwrap_or(-1),
                row.get(1).and_then(Value::as_integer).unwrap_or(-1),
                text(row.get(2)),
            )
        })
        .collect();
    assert_eq!(
        seen,
        vec![
            (1, 0, "Subject: first\n\nalpha beta ".to_string()),
            (1, 1, "Subject: first\n\ngamma delta ".to_string()),
            (1, 2, "Subject: first\n\nepsilon".to_string()),
            (2, 0, "Subject: second\n\none two ".to_string()),
            (2, 1, "Subject: second\n\nthree".to_string()),
        ]
    );
}

/// Joining the windows and dropping each overlap gives back the original text, for many sizes and overlaps.
#[test]
fn the_windows_tile_the_text() {
    let connection = connect();
    let text = "The quick brown fox.  Jumps over the lazy dog! A new paragraph starts here? Yes. It has \
                several sentences. Some are short. Others run on and on without stopping for a good \
                long while, so that a window has to end somewhere.";
    let characters: Vec<char> = text.chars().collect();
    for (size, overlap) in [
        (1, 0),
        (7, 0),
        (7, 3),
        (30, 10),
        (45, 0),
        (45, 44),
        (500, 100),
    ] {
        let rows = chunks(&connection, &format!("'{text}', {size}, {overlap}"));
        let mut rebuilt = String::new();
        let mut covered = 0usize;
        for row in &rows {
            let (start, length) = (row.start as usize, row.length as usize);
            assert!(start <= covered, "a gap at size {size} overlap {overlap}");
            assert!(length >= 1 && length <= size);
            rebuilt.extend(&characters[covered..start + length]);
            covered = start + length;
        }
        assert_eq!(rebuilt, text, "size {size} overlap {overlap}");
    }
}

/// A heading that is `NULL` because it was built from a `NULL` column writes no heading.
///
/// A literal `NULL` in the argument list is different: the planner folds `heading = NULL` to false
/// before the function is asked, so the call returns no rows. `generate_series(1, NULL)` does the
/// same. The documentation says to leave the argument out.
#[test]
fn a_heading_built_from_a_null_column_writes_none() {
    let connection = connect();
    connection
        .execute("CREATE TABLE doc (id INTEGER PRIMARY KEY, subject TEXT, body TEXT)")
        .expect("creates");
    connection
        .execute("INSERT INTO doc VALUES (1, NULL, 'alpha beta'), (2, 's', 'gamma')")
        .expect("fills");
    let rows = connection
        .query(
            "SELECT d.id, c.chunk FROM doc AS d, \
             chunk_text(d.body, 900, 0, 'Subject: ' || d.subject) AS c ORDER BY d.id",
        )
        .expect("runs");
    let seen: Vec<(i64, String)> = rows
        .iter()
        .map(|row| {
            (
                row.first().and_then(Value::as_integer).unwrap_or(-1),
                text(row.get(1)),
            )
        })
        .collect();
    assert_eq!(
        seen,
        vec![
            (1, "alpha beta".to_string()),
            (
                2,
                "Subject: s

gamma"
                    .to_string()
            )
        ]
    );
}
