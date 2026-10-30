//! Cost guards for the JSON functions and the table valued `json_each` and `json_tree`.
//!
//! Invariant: **a guard here counts the bytes the calling thread allocated, because the defects it
//! catches copy or rebuild a large document and do no extra I/O.** A clock cannot hold them on a
//! shared machine (see the header of `budget.rs`), and an allocation count cannot see them either,
//! because a copy of a 2 MB document is one allocation. The bytes allocated are the same number on
//! an idle machine and a loaded one, and they grow with the quadratic the guard is there to refuse.
//!
//! Each guard states what it measured before the fix and after it, so a threshold is a fraction of
//! the measured gap and not a number somebody tuned until the test stopped failing.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use inillucent_base::testing::ScratchDir;
use inillucent_engine::connect::{Connection, Database};
use inillucent_tree::datum::OwnedDatum;

thread_local! {
    /// How many bytes the calling thread has asked the allocator for.
    ///
    /// Per thread, so another test in the same binary cannot move it. `const` initialised and
    /// holding a type with no destructor, so reading it from inside the allocator cannot itself
    /// allocate.
    static BYTES: Cell<u64> = const { Cell::new(0) };
}

/// An allocator that adds up the bytes each thread requests, and otherwise delegates.
struct ByteCountingAllocator;

// SAFETY: every method forwards to the system allocator with the same arguments. The counter is
// the only addition and it touches no memory the allocator owns.
unsafe impl GlobalAlloc for ByteCountingAllocator {
    // SAFETY: the layout is the caller's, forwarded unchanged.
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count_bytes(layout.size());
        // SAFETY: forwarded unchanged to the system allocator.
        unsafe { System.alloc(layout) }
    }

    // SAFETY: the pointer and layout are the ones this allocator handed out.
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: forwarded unchanged to the allocator that made the pointer.
        unsafe { System.dealloc(pointer, layout) }
    }

    // SAFETY: the pointer and layout are the ones this allocator handed out.
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        count_bytes(size);
        // SAFETY: forwarded unchanged to the allocator that made the pointer.
        unsafe { System.realloc(pointer, layout, size) }
    }
}

/// Adds a request to the calling thread's total, if the thread still has one.
///
/// `try_with` because a thread being torn down has dropped its locals, and an allocation made
/// after that must not panic inside the allocator.
///
/// @param size - the bytes requested
fn count_bytes(size: usize) {
    let _ = BYTES.try_with(|total| total.set(total.get().saturating_add(size as u64)));
}

#[global_allocator]
static ALLOCATOR: ByteCountingAllocator = ByteCountingAllocator;

/// Returns how many bytes the calling thread allocated while running a closure.
///
/// @param body - the work to count
fn bytes_allocated(body: impl FnOnce()) -> u64 {
    let before = BYTES.with(Cell::get);
    body();
    BYTES.with(Cell::get).saturating_sub(before)
}

/// Returns a fresh directory for one test's database, removed when the guard drops.
///
/// @param tag - what to name the directory after
fn scratch(tag: &str) -> ScratchDir {
    ScratchDir::new(&format!(
        "inillucent-json-budget-{}-{tag}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ))
}

/// Returns the one integer a single row, single column answer holds.
///
/// @param rows - what the query returned
fn one(rows: &[Vec<OwnedDatum>]) -> i64 {
    match rows {
        [row] => match row.as_slice() {
            [OwnedDatum::Int(value)] => *value,
            other => panic!("expected one integer, got {other:?}"),
        },
        other => panic!("expected one row, got {}", other.len()),
    }
}

/// Runs a query that returns one integer and reports it with the bytes it allocated.
///
/// @param connection - the connection to run it on
/// @param sql - a query with one integer answer
fn counted(connection: &Connection<'_>, sql: &str) -> (i64, u64) {
    let mut answer = 0;
    let bytes = bytes_allocated(|| {
        answer = one(&connection.query(sql).expect("the query runs"));
    });
    (answer, bytes)
}

/// Bytes one table valued function call may allocate for itself, whatever it is given.
///
/// Measured at about 11.8 KB for a call over a 26 byte document: the module is connected, its
/// columns are declared and its rows are collected for every outer row of a lateral join.
const BYTES_PER_CALL: u64 = 48 * 1024;

/// A second `json_each` over the elements of a large document does not copy the document per row.
///
/// **What this is a regression test for.** `FROM s, json_each(s.j) e, json_each(e.value) t` copied
/// every column of its outer row for each outer row, and the outer row carried `s.j`, the whole
/// document, so the join allocated the document's size once per element: 40,000 elements took
/// 14.5 seconds where SQLite takes 20 milliseconds. The lateral join now borrows the outer row
/// from the batch it was given.
///
/// The allowance is eight times the document plus [`BYTES_PER_CALL`] for each element. Measured
/// with 2,000 elements and a document of 470,894 bytes: 983,135,685 bytes allocated
/// before the fix and 40,215,882 after it.
#[test]
fn a_nested_json_each_does_not_copy_the_document_for_every_row() {
    const ELEMENTS: u64 = 2000;
    let directory = scratch("nested");
    let database = Database::open(directory.join("j.rdb")).expect("the database opens");
    let connection = database.session();
    connection
        .execute_batch(&format!(
            "CREATE TABLE s(j);\
             INSERT INTO s SELECT json_group_array(json_object('id', value, 'tags', json_array(1,2,3), \
             'pad', printf('%.200c', 'x'))) FROM generate_series(1, {ELEMENTS});"
        ))
        .expect("the document is built");
    let size = one(&connection
        .query("SELECT length(j) FROM s")
        .expect("the length reads")) as u64;
    let (rows, bytes) = counted(
        &connection,
        "SELECT count(*) FROM s, json_each(s.j) e, json_each(e.value) t",
    );
    assert_eq!(
        rows,
        3 * ELEMENTS as i64,
        "each element is an object of three members"
    );
    let allowed = size * 8 + ELEMENTS * BYTES_PER_CALL;
    assert!(
        bytes <= allowed,
        "the nested json_each over a {size} byte document allocated {bytes} bytes; the guard \
         asks for at most {allowed}, which is eight copies of the document and {BYTES_PER_CALL} \
         bytes for each of its {ELEMENTS} elements"
    );
}

/// Builds a table of `rows` documents, each about 18 KB: an array of 300 small objects and a
/// small object of four members.
///
/// @param connection - the connection to build the table on
/// @param rows - how many documents to make
fn build_documents(connection: &Connection<'_>, rows: u64) {
    connection
        .execute_batch(&format!(
            "CREATE TABLE d(id INTEGER PRIMARY KEY, j);\
             INSERT INTO d SELECT value, json_object('id', value, 'items', \
             (SELECT json_group_array(json_object('sku', 'S' || x.value, 'qty', x.value)) \
             FROM generate_series(1, 300) x), \
             'meta', json_object('a', 1, 'b', 2, 'c', 3, 'd', 4)) FROM generate_series(1, {rows});"
        ))
        .expect("the documents are built");
}

/// Five `->>` calls over one document cost about what one costs, and one costs about what
/// `json_extract` costs.
///
/// **What this is a regression test for.** Every JSON call kept a cache of its own, and `->` and
/// `->>` kept none, so five calls on the same text parsed it five times and an arrow parsed the
/// whole document into a tree where `json_extract` built only the part its path reads. 2,000
/// documents of 18 KB took 861 ms for five `->>` calls, 237 ms for five `json_extract` calls and
/// 172 ms for one `->>`, against 27 ms for the five in SQLite. The calls of a statement now share
/// one parse of a document, and the arrows read through the same cache as `json_extract`.
///
/// Both guards are ratios of bytes allocated in the same run. Measured with 200 documents, one
/// `->>` allocated 64,077,168 bytes before the fix, five `->>` 314,300,512 and five `json_extract`
/// 4,329,172. After it all three allocate 3,292,716, which is the cost of reading the 200 rows.
#[test]
fn several_json_calls_over_one_document_parse_it_once() {
    let directory = scratch("calls");
    let database = Database::open(directory.join("j.rdb")).expect("the database opens");
    let connection = database.session();
    build_documents(&connection, 200);
    let one = "SELECT sum(j->>'$.meta.a') FROM d";
    let five = "SELECT sum(j->>'$.meta.a' + j->>'$.meta.b' + j->>'$.meta.c' + j->>'$.meta.d' + j->>'$.id') FROM d";
    let extract_five = "SELECT sum(json_extract(j, '$.meta.a') + json_extract(j, '$.meta.b') + \
                        json_extract(j, '$.meta.c') + json_extract(j, '$.meta.d') + json_extract(j, '$.id')) FROM d";
    // Each statement runs once first, so none pays for a plan the others already have.
    for sql in [one, five, extract_five] {
        counted(&connection, sql);
    }
    let (one_sum, one_bytes) = counted(&connection, one);
    let (five_sum, five_bytes) = counted(&connection, five);
    let (extract_sum, extract_bytes) = counted(&connection, extract_five);
    assert_eq!(
        one_sum, 200,
        "the one member read is 1 in each of 200 documents"
    );
    assert_eq!(
        five_sum, extract_sum,
        "the arrows and json_extract read the same members"
    );
    assert!(
        five_bytes <= one_bytes * 2,
        "five ->> calls allocated {five_bytes} bytes and one allocated {one_bytes}; the guard \
         asks for at most twice, because the five share one parse of each document"
    );
    assert!(
        five_bytes <= extract_bytes * 2,
        "five ->> calls allocated {five_bytes} bytes and five json_extract calls allocated \
         {extract_bytes}; the guard asks for at most twice"
    );
}
