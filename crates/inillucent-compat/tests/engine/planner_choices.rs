//! Six query shapes where the planner picked a plan far slower than SQLite's.
//!
//! Invariant: **each plan below is the one that stops early or reads the fewest rows, and returns
//! the rows the plan it replaced returned.**
//!
//! ## Why this exists
//!
//! Bug hunt 9 measured these on a 200,000 row `posts` table against SQLite 3.53.4:
//!
//! - Keyset pagination `WHERE (created_at, id) < (?, ?) ORDER BY created_at DESC, id DESC LIMIT 20`
//!   scanned the index from its end, because the row value comparison was an `OR` and no range was
//!   taken from it.
//! - `posts JOIN users ... ORDER BY p.created_at DESC LIMIT 20` started from `users`, joined every
//!   post and sorted: 244 ms against 9 ms.
//! - `SELECT p.id, (correlated subquery) FROM posts p ORDER BY p.id LIMIT 10` read every row
//!   because the executor replaced the table walk with a narrower index, whose order is not the
//!   `ORDER BY`'s, and sorted: 506 ms against 19 ms.
//! - A window subquery filtered on its `PARTITION BY` column from outside numbered every row of the
//!   table first: 152 ms against 5.4 ms.
//! - A six table join with `WHERE c.region = 'R3'` started from the 100,000 row `users` index
//!   instead of the filtered `customers`: 680 ms against 159 ms.
//!
//! And sqllogictest's `select5.test`: a join of 19 tables of 10 rows, written in no useful order,
//! kept its written order because it had more than eight terms, and passed 3 GB in a second where
//! SQLite answers in 0.2 s.
//!
//! The plan is asserted where the plan text shows the difference, and the rows are compared with a
//! form of the same query that cannot take the new path, or with the answer worked out from the
//! data.

use inillucent_compat::newengine::connect::Database;
use inillucent_compat::newengine::OwnedDatum;
use inillucent_compat::workspace_root;

/// Opens a fresh database holding every table the cases below read.
///
/// `posts.user_id` runs past the last `users.id`, so the inner join drops rows and a `LIMIT` has to
/// keep walking to find enough that match.
///
/// @param name - the case's name, which is also its directory's
fn shop(name: &str) -> Database {
    let directory = workspace_root()
        .join("_agent_output/planner-choices")
        .join(name);
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("the case directory can be made");
    let database = Database::open(directory.join("case.rdb")).expect("the database opens");
    database
        .session()
        .execute_batch(
            "CREATE TABLE users(id INTEGER PRIMARY KEY, name TEXT, country TEXT); \
             CREATE INDEX users_country ON users(country); \
             CREATE TABLE posts(id INTEGER PRIMARY KEY, user_id INT, title TEXT, created_at INT); \
             CREATE INDEX posts_user ON posts(user_id); \
             CREATE INDEX posts_created ON posts(created_at); \
             CREATE TABLE comments(id INTEGER PRIMARY KEY, post_id INT, body TEXT, created_at INT); \
             CREATE INDEX comments_post ON comments(post_id, created_at); \
             CREATE TABLE customers(id INTEGER PRIMARY KEY, region TEXT); \
             CREATE TABLE orders(id INTEGER PRIMARY KEY, customer_id INT); \
             CREATE INDEX orders_customer ON orders(customer_id); \
             CREATE TABLE items(id INTEGER PRIMARY KEY, order_id INT, qty INT); \
             CREATE INDEX items_order ON items(order_id); \
             INSERT INTO users SELECT value, 'name ' || value, 'C' || (value % 50) \
                 FROM generate_series(1, 2000); \
             INSERT INTO posts SELECT value, 1 + (value * 7919) % 2100, 'title ' || value, \
                 (value * 7919) % 5000 FROM generate_series(1, 20000); \
             INSERT INTO comments SELECT value, 1 + (value * 104729) % 20000, 'c' || value, \
                 (value * 11) % 6000 FROM generate_series(1, 40000); \
             INSERT INTO customers SELECT value, 'R' || (value % 10) FROM generate_series(1, 5000); \
             INSERT INTO orders SELECT value, 1 + (value * 7) % 5000 FROM generate_series(1, 15000); \
             INSERT INTO items SELECT value, 1 + value % 15000, value % 5 \
                 FROM generate_series(1, 40000); \
             ANALYZE;",
        )
        .expect("the tables are built");
    database
}

/// Returns the plan's lines, joined.
///
/// @param database - the database
/// @param sql - the query
fn plan(database: &Database, sql: &str) -> String {
    let rows = database
        .session()
        .query(&format!("EXPLAIN QUERY PLAN {sql}"))
        .expect("the plan is described");
    rows.iter()
        .filter_map(|row| match row.last() {
            Some(OwnedDatum::Text(text)) => Some(String::from_utf8_lossy(text).into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" / ")
}

/// Returns every row a query answers, as text.
///
/// @param database - the database
/// @param sql - the query
fn rows(database: &Database, sql: &str) -> Vec<String> {
    database
        .session()
        .query(sql)
        .expect("the query runs")
        .iter()
        .map(|row| format!("{row:?}"))
        .collect()
}

/// A row value comparison seeks the leading column's index, in both directions.
#[test]
fn a_keyset_comparison_seeks_the_index_on_its_leading_column() {
    let database = shop("keyset");
    for sql in [
        "SELECT id, title FROM posts WHERE (created_at, id) < (2500, 5000) \
         ORDER BY created_at DESC, id DESC LIMIT 20",
        "SELECT id, title FROM posts WHERE (created_at, id) > (2500, 5000) \
         ORDER BY created_at, id LIMIT 20",
    ] {
        let described = plan(&database, sql);
        assert!(
            (described.contains("created_at<?") || described.contains("created_at>?"))
                && !described.contains("TEMP B-TREE"),
            "{sql} was planned as {described}"
        );
    }
}

/// The seek and the scan agree for every comparison operator, including ties on `created_at`.
#[test]
fn a_keyset_comparison_returns_what_the_scan_returns() {
    let database = shop("keyset-answers");
    for op in ["<", "<=", ">", ">="] {
        let order = if op.starts_with('<') {
            "created_at DESC, id DESC"
        } else {
            "created_at, id"
        };
        let seeked = format!(
            "SELECT id, title FROM posts WHERE (created_at, id) {op} (2500, 5000) \
             ORDER BY {order} LIMIT 40"
        );
        let scanned = format!(
            "SELECT id, title FROM posts NOT INDEXED WHERE (created_at, id) {op} (2500, 5000) \
             ORDER BY {order} LIMIT 40"
        );
        assert_eq!(rows(&database, &seeked), rows(&database, &scanned), "{op}");
    }
    let seeked = "SELECT id FROM posts WHERE (created_at, id) < (NULL, 5) ORDER BY id LIMIT 5";
    assert!(
        rows(&database, seeked).is_empty(),
        "a NULL bound keeps no row"
    );
}

/// A join ordered by a column of its first table walks that table's index and probes the rest.
#[test]
fn a_limited_join_walks_the_ordered_table_and_probes_the_others() {
    let database = shop("join-order");
    for sql in [
        "SELECT p.id, u.name FROM posts p JOIN users u ON u.id = p.user_id \
         ORDER BY p.created_at DESC LIMIT 20",
        "SELECT p.id, u.name FROM posts p JOIN users u ON u.id = p.user_id \
         ORDER BY p.created_at LIMIT 20",
    ] {
        let described = plan(&database, sql);
        assert!(
            described.starts_with("SCAN p USING INDEX posts_created")
                && !described.contains("TEMP B-TREE"),
            "{sql} was planned as {described}"
        );
    }
}

/// The walk returns what the sort returns when the join drops posts, and for a `LEFT JOIN`.
///
/// `posts.user_id` reaches 2,100 and `users.id` stops at 2,000, so about one post in twenty has
/// no user and an inner join has to keep walking past them.
#[test]
fn a_limited_join_returns_what_the_sort_returns() {
    let database = shop("join-answers");
    for (walked, sorted) in [
        (
            "SELECT p.id, u.name FROM posts p JOIN users u ON u.id = p.user_id \
             ORDER BY p.created_at DESC, p.id DESC LIMIT 60",
            "SELECT p.id, u.name FROM posts p NOT INDEXED JOIN users u ON u.id = p.user_id \
             ORDER BY p.created_at DESC, p.id DESC LIMIT 60",
        ),
        (
            "SELECT p.id, u.name FROM posts p LEFT JOIN users u ON u.id = p.user_id \
             ORDER BY p.created_at, p.id LIMIT 60 OFFSET 11",
            "SELECT p.id, u.name FROM posts p NOT INDEXED LEFT JOIN users u ON u.id = p.user_id \
             ORDER BY p.created_at, p.id LIMIT 60 OFFSET 11",
        ),
        (
            "SELECT p.id, u.name FROM posts p JOIN users u ON u.id = p.user_id \
             WHERE u.country = 'C7' ORDER BY p.created_at DESC, p.id DESC LIMIT 25",
            "SELECT p.id, u.name FROM posts p NOT INDEXED JOIN users u ON u.id = p.user_id \
             WHERE u.country = 'C7' ORDER BY p.created_at DESC, p.id DESC LIMIT 25",
        ),
    ] {
        assert_eq!(rows(&database, walked), rows(&database, sorted), "{walked}");
    }
}

/// A forward walk with a `LIMIT` stops early even when a narrower index could supply the columns.
#[test]
fn an_ordered_table_walk_is_not_replaced_by_a_narrow_index() {
    let database = shop("narrow");
    let sql = "SELECT p.id, (SELECT c.body FROM comments c WHERE c.post_id = p.id \
               ORDER BY c.created_at DESC LIMIT 1) FROM posts p ORDER BY p.id LIMIT 10";
    let without = "SELECT p.id, (SELECT c.body FROM comments c WHERE c.post_id = p.id \
                   ORDER BY c.created_at DESC LIMIT 1) FROM posts p NOT INDEXED \
                   ORDER BY p.id + 0 LIMIT 10";
    assert_eq!(rows(&database, sql), rows(&database, without));
    assert_eq!(rows(&database, sql).len(), 10);
    // The plan text is the same with or without the sorter, so the walk is checked by cost: ten
    // rows must take a small share of the time the same statement takes for every row.
    let timed = |sql: &str| {
        let mut best = std::time::Duration::MAX;
        for _ in 0..3 {
            let started = std::time::Instant::now();
            let _ = rows(&database, sql);
            best = best.min(started.elapsed());
        }
        best
    };
    let all = sql.replace("LIMIT 10", "LIMIT 20000");
    let (few, every) = (timed(sql), timed(&all));
    assert!(
        few * 4 < every,
        "ten rows took {few:?} and all of them took {every:?}, so the walk did not stop early"
    );
}

/// A term on the `PARTITION BY` column is applied before the window numbers the rows.
///
/// The rows are compared with the same query written with the filter inside, and the cases where
/// pushing is wrong are checked too: a term on the window's value, and a partition under a
/// collation the term does not share.
#[test]
fn a_partition_filter_is_pushed_into_the_window_subquery() {
    let database = shop("window");
    for (outer, inner) in [
        (
            "SELECT user_id, id FROM (SELECT user_id, id, row_number() OVER \
             (PARTITION BY user_id ORDER BY created_at DESC, id) rn FROM posts) \
             WHERE rn <= 3 AND user_id < 50 ORDER BY user_id, id",
            "SELECT user_id, id FROM (SELECT user_id, id, row_number() OVER \
             (PARTITION BY user_id ORDER BY created_at DESC, id) rn FROM posts \
             WHERE user_id < 50) WHERE rn <= 3 ORDER BY user_id, id",
        ),
        (
            "SELECT user_id, id, rn FROM (SELECT user_id, id, rank() OVER \
             (PARTITION BY user_id ORDER BY created_at) rn FROM posts) \
             WHERE user_id = 9 AND rn <= 2 ORDER BY id",
            "SELECT user_id, id, rn FROM (SELECT user_id, id, rank() OVER \
             (PARTITION BY user_id ORDER BY created_at) rn FROM posts WHERE user_id = 9) \
             WHERE rn <= 2 ORDER BY id",
        ),
    ] {
        assert_eq!(rows(&database, outer), rows(&database, inner), "{outer}");
    }
    // A term on a column the window does not partition by keeps its place after the window.
    let by_other = "SELECT id, rn FROM (SELECT id, created_at, row_number() OVER \
                    (PARTITION BY user_id ORDER BY created_at, id) rn FROM posts) \
                    WHERE created_at > 4000 ORDER BY id LIMIT 30";
    let inside = "SELECT id, rn FROM (SELECT id, created_at, row_number() OVER \
                  (PARTITION BY user_id ORDER BY created_at, id) rn FROM posts \
                  WHERE created_at > 4000) ORDER BY id LIMIT 30";
    assert_ne!(
        rows(&database, by_other),
        rows(&database, inside),
        "numbering after the filter is a different answer, so the term must not move"
    );
    // Two windows with different partitions: only a column both partition by may move.
    let two = "SELECT user_id, id, a, b FROM (SELECT user_id, id, \
               row_number() OVER (PARTITION BY user_id ORDER BY id) a, \
               row_number() OVER (PARTITION BY created_at ORDER BY id) b FROM posts) \
               WHERE user_id = 3 ORDER BY id LIMIT 20";
    let two_inside = "SELECT user_id, id, a, b FROM (SELECT user_id, id, \
                      row_number() OVER (PARTITION BY user_id ORDER BY id) a, \
                      row_number() OVER (PARTITION BY created_at ORDER BY id) b FROM posts \
                      WHERE user_id = 3) ORDER BY id LIMIT 20";
    assert_ne!(rows(&database, two), rows(&database, two_inside));
}

/// A join counted under an equality on an unindexed column starts from the filtered table.
#[test]
fn a_filter_on_an_unindexed_column_chooses_the_outer_table() {
    let database = shop("filtered-outer");
    let sql = "SELECT count(*) FROM items i JOIN orders o ON o.id = i.order_id \
               JOIN customers c ON c.id = o.customer_id JOIN users u ON u.id = c.id \
               WHERE c.region = 'R3'";
    let described = plan(&database, sql);
    assert!(
        described.starts_with("SCAN c"),
        "the filtered customers should drive the join: {described}"
    );
    let forced = "SELECT count(*) FROM items i CROSS JOIN orders o CROSS JOIN customers c \
                  CROSS JOIN users u WHERE o.id = i.order_id AND c.id = o.customer_id \
                  AND u.id = c.id AND c.region = 'R3'";
    assert_eq!(rows(&database, sql), rows(&database, forced));
}

/// Returns where the chain's row `a` of table `n` points: its `b`, the `a` of table `n + 1` it
/// joins.
///
/// A permutation of 1 to 10 for every table, so each row of one table joins exactly one row of the
/// next and the chain answers exactly one row for each value it is pinned to.
///
/// @param n - the table, 1 to 20
/// @param a - the row, 1 to 10
fn chain_next(n: i64, a: i64) -> i64 {
    let step = [1, 3, 7, 9][(n % 4) as usize];
    (a * step + n) % 10 + 1
}

/// A join of 20 tables written out of order is planned along its equalities, and answers.
///
/// sqllogictest's `select5.test` joins up to 64 tables of 10 rows by chains of equalities, each
/// table's `b` naming the next one's `INTEGER PRIMARY KEY` `a`, with the tables and the terms in
/// no useful order. Above eight terms the planner kept the written order, so every table the
/// written order reached before the one its key comes from was scanned inside the tables before
/// it: a product of 10 rows a table. Its query of 19 tables passed 3 GB in a second; SQLite
/// answers it in 0.2 s with one scan and 18 key lookups. This is the same shape with 20 tables,
/// pinned on the chain's last table as that query is, and written so that the old order scans
/// nine tables one inside another.
///
/// The plan is asserted before the query runs, so the engine this test was written against
/// fails here instead of running the product. The row is the one the chain reaches, worked out
/// here from the data.
#[test]
fn a_twenty_table_chain_written_out_of_order_follows_its_equalities() {
    let directory = workspace_root().join("_agent_output/planner-choices/chain");
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("the case directory can be made");
    let database = Database::open(directory.join("case.rdb")).expect("the database opens");
    let mut schema = String::from("BEGIN;");
    for n in 1..=20i64 {
        schema.push_str(&format!(
            "CREATE TABLE t{n}(a{n} INTEGER PRIMARY KEY, b{n} INTEGER, x{n} VARCHAR(40));"
        ));
        for a in 1..=10i64 {
            schema.push_str(&format!(
                "INSERT INTO t{n} VALUES({a}, {}, 'table t{n} row {a}');",
                chain_next(n, a)
            ));
        }
    }
    schema.push_str("COMMIT;");
    database
        .session()
        .execute_batch(&schema)
        .expect("the tables are built");
    let pinned = 4i64;
    let columns = (1..=20).map(|n| format!("x{n}")).collect::<Vec<_>>();
    let from = (0..20)
        .map(|at| format!("t{}", (at * 9) % 20 + 1))
        .collect::<Vec<_>>();
    let mut terms = (0..19)
        .map(|at| {
            let n = (at * 11) % 19 + 1;
            format!("b{n}=a{}", n + 1)
        })
        .collect::<Vec<_>>();
    terms.insert(7, format!("a20={pinned}"));
    let sql = format!(
        "SELECT {} FROM {} WHERE {}",
        columns.join(","),
        from.join(","),
        terms.join(" AND ")
    );

    let described = plan(&database, &sql);
    let scans = described
        .split(" / ")
        .filter(|line| line.starts_with("SCAN "))
        .count();
    assert!(
        scans <= 1,
        "the plan scans {scans} tables one inside another, where one entry into the chain \
         is enough: {described}"
    );

    let start = (1..=10i64)
        .find(|first| (1..20).fold(*first, |a, n| chain_next(n, a)) == pinned)
        .expect("one row of t1 reaches the pinned row of t20");
    let mut expected = Vec::new();
    let mut a = start;
    for n in 1..=20i64 {
        expected.push(OwnedDatum::Text(format!("table t{n} row {a}").into_bytes()));
        a = chain_next(n, a);
    }
    let answered = database.session().query(&sql).expect("the query runs");
    assert_eq!(answered, vec![expected]);
}
