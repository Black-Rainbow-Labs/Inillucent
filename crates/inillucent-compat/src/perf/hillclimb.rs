//! The hillclimb plan: the workloads the performance hill climb is graded on,
//! beyond the contract's own plan.
//!
//! Invariant: **every workload here belongs to exactly one of two fixed sets,
//! train or test, and the set is a function of the workload's name alone.**
//! The hill climb keeps a change only when the train score and the test score
//! both improve, so a change tuned to the workloads it was profiled on has to
//! help workloads nobody looked at while making it. The split is a hash of the
//! name, so it cannot be moved after a result is known without renaming a
//! workload, which a reviewer sees.
//!
//! The contract's plan (`perf::plan_for`) measures a fixture that was bulk
//! imported, so every table in it is packed and every leaf is sorted. An
//! application builds its tables with `INSERT`, one statement or one batch at
//! a time, and reads them straight after. That is the gap this plan covers:
//! tables built by the SQL an application runs, reads on those tables while
//! they are fresh, the same tables after updates and deletes, the queries an
//! application writes most, and edge cases where one engine's strategy can be
//! far from the other's.
//!
//! The plan runs through the same gate and the same `sqlite_bench.c` as the
//! contract's plan. `sqlite_bench.c` holds at most 64 workloads and 8,192
//! bytes of SQL a statement, and a test below checks both.

use super::{Bind, Grouping, Plan, Workload};

/// How many rows the `fresh` table is built with by `ai.build`.
pub const FRESH_ROWS: u32 = 20_000;

/// Returns the hillclimb plan for one scale.
///
/// The plan reads the same fixture as the contract's plan and builds every
/// table of its own in a `pre`, so a round starts from the same file on both
/// engines and the workloads that follow see the same rows.
///
/// @param scale - `small`, `medium` or `large`
pub fn plan_for(scale: &str) -> Plan {
    let base = super::plan_for(scale);
    let (point, scan, write) = super::repeats_for(scale);
    let mut workloads: Vec<Workload> = Vec::new();
    workloads.extend(after_insert_workloads(point, scan, write));
    workloads.extend(churn_workloads(point, scan, write));
    workloads.extend(app_workloads(point, scan, write));
    workloads.extend(edge_workloads(point, scan));
    workloads.extend(check_workloads());
    Plan { workloads, ..base }
}

/// Returns whether a workload is in the held out test set.
///
/// FNV-1a over the name, one in three. The contract's workloads are split by
/// the same rule, so the score tool can grade both plans with one function.
///
/// @param name - the workload's name
pub fn is_test(name: &str) -> bool {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in name.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash.is_multiple_of(3)
}

/// Returns a read workload with the common defaults.
///
/// @param name - the workload's name
/// @param family - the family it is reported under
/// @param sql - the measured statement
/// @param repeat - how many times it runs
/// @param binds - its parameters
fn read(name: &str, family: &str, sql: &str, repeat: u32, binds: Vec<Bind>) -> Workload {
    Workload {
        name: name.to_string(),
        family: family.to_string(),
        sql: sql.to_string(),
        pre: None,
        post: None,
        repeat: repeat.max(1),
        grouping: Grouping::Autocommit,
        prepare_each: false,
        binds,
        mutates: false,
    }
}

/// Returns a write workload with the common defaults.
///
/// @param name - the workload's name
/// @param family - the family it is reported under
/// @param sql - the measured statement
/// @param repeat - how many times it runs
/// @param grouping - how the statements are grouped into transactions
/// @param binds - its parameters
fn write(
    name: &str,
    family: &str,
    sql: &str,
    repeat: u32,
    grouping: Grouping,
    binds: Vec<Bind>,
) -> Workload {
    Workload {
        name: name.to_string(),
        family: family.to_string(),
        sql: sql.to_string(),
        pre: None,
        post: None,
        repeat: repeat.max(1),
        grouping,
        prepare_each: false,
        binds,
        mutates: true,
    }
}

/// Returns the workload with a `pre` statement set.
///
/// @param workload - the workload
/// @param pre - the untimed statements run before it
fn with_pre(mut workload: Workload, pre: &str) -> Workload {
    workload.pre = Some(pre.to_string());
    workload
}

/// Returns the workloads that build a table with `INSERT` and read it while it is fresh.
///
/// This is the case the contract's plan does not have. `fresh` gets 20,000
/// rows in batches of 500, with two secondary indexes whose keys arrive in no
/// order, which is how an application's table grows. The reads that follow
/// are the ones an application runs against it: by rowid, by a unique name,
/// the newest rows of a group, a grouped total and a count. Then a second
/// batch of rows arrives and the reads run again over the rows that are
/// newest. `copied` is the other way a table is built, one `INSERT ... SELECT`.
///
/// @param point - how many times a point workload repeats
/// @param scan - how many times a scan workload repeats
/// @param repeat_write - how many times a write workload repeats
fn after_insert_workloads(point: u32, scan: u32, repeat_write: u32) -> Vec<Workload> {
    let family = "hc.after_insert";
    let fresh = format!("(?1 % {FRESH_ROWS}) + 1");
    vec![
        with_pre(
            write(
                "ai.build",
                family,
                "INSERT INTO fresh(grp, name, amount, created) \
                 VALUES (?1 % 97, 'user ' || ?2, (?1 % 10000) * 0.25, ?2)",
                FRESH_ROWS,
                Grouping::Every(500),
                vec![Bind::Int, Bind::Counter],
            ),
            "DROP TABLE IF EXISTS fresh; CREATE TABLE fresh(id INTEGER PRIMARY KEY, \
             grp INTEGER NOT NULL, name TEXT NOT NULL, amount REAL, created INTEGER NOT NULL); \
             CREATE INDEX fresh_grp ON fresh(grp, created); \
             CREATE UNIQUE INDEX fresh_name ON fresh(name)",
        ),
        read(
            "ai.point.rowid",
            family,
            &format!("SELECT name, amount FROM fresh WHERE id = {fresh}"),
            point,
            vec![Bind::Scatter],
        ),
        read(
            "ai.point.name",
            family,
            "SELECT id, amount FROM fresh WHERE name = 'user ' || ((?1 % 20000) + 100001)",
            point,
            vec![Bind::Scatter],
        ),
        read(
            "ai.group.newest",
            family,
            "SELECT id, amount FROM fresh WHERE grp = ?1 % 97 ORDER BY created DESC LIMIT 20",
            point / 4,
            vec![Bind::Scatter],
        ),
        read(
            "ai.group.total",
            family,
            "SELECT grp, count(*), sum(amount) FROM fresh GROUP BY grp",
            scan,
            Vec::new(),
        ),
        read(
            "ai.count",
            family,
            "SELECT count(*) FROM fresh",
            scan,
            Vec::new(),
        ),
        write(
            "ai.append",
            family,
            "INSERT INTO fresh(grp, name, amount, created) \
             VALUES (?1 % 97, 'user ' || (?2 + 50000), (?1 % 10000) * 0.25, ?2 + 50000)",
            repeat_write,
            Grouping::Every(20),
            vec![Bind::Int, Bind::Counter],
        ),
        read(
            "ai.point.newest",
            family,
            &format!(
                "SELECT name FROM fresh WHERE id = {} + (?1 % {repeat_write})",
                FRESH_ROWS + 1
            ),
            point,
            vec![Bind::Scatter],
        ),
        read(
            "ai.range.created",
            family,
            "SELECT count(*), sum(amount) FROM fresh WHERE grp = ?1 % 97 AND created > 150000",
            point / 4,
            vec![Bind::Scatter],
        ),
        with_pre(
            write(
                "ai.copy",
                family,
                "INSERT INTO copied(id, key, label) SELECT id, key, label FROM main_table",
                1,
                Grouping::Autocommit,
                Vec::new(),
            ),
            "DROP TABLE IF EXISTS copied; \
             CREATE TABLE copied(id INTEGER PRIMARY KEY, key INTEGER, label TEXT)",
        ),
        read(
            "ai.copy.scan",
            family,
            "SELECT sum(length(label)), sum(key) FROM copied",
            scan,
            Vec::new(),
        ),
        read(
            "ai.copy.point",
            family,
            "SELECT label FROM copied WHERE id = ?1",
            point,
            vec![Bind::Scatter],
        ),
    ]
}

/// Returns the workloads that change `fresh` and read it again.
///
/// Updates that grow a value, a delete of every other row, and the reads
/// afterwards: what a table looks like after a few weeks of an application
/// using it.
///
/// @param point - how many times a point workload repeats
/// @param scan - how many times a scan workload repeats
/// @param repeat_write - how many times a write workload repeats
fn churn_workloads(point: u32, scan: u32, repeat_write: u32) -> Vec<Workload> {
    let family = "hc.churn";
    vec![
        write(
            "churn.update.grow",
            family,
            &format!(
                "UPDATE fresh SET amount = amount + 1, name = name || '.x' \
                 WHERE id = (?1 % {FRESH_ROWS}) + 1"
            ),
            repeat_write,
            Grouping::Every(100),
            vec![Bind::Scatter],
        ),
        write(
            "churn.delete.half",
            family,
            "DELETE FROM fresh WHERE id % 2 = 0",
            1,
            Grouping::Autocommit,
            Vec::new(),
        ),
        read(
            "churn.scan",
            family,
            "SELECT count(*), sum(amount), sum(length(name)) FROM fresh",
            scan,
            Vec::new(),
        ),
        read(
            "churn.point.name",
            family,
            "SELECT id FROM fresh WHERE name = 'user ' || ((?1 % 20000) + 100001)",
            point,
            vec![Bind::Scatter],
        ),
        read(
            "churn.group.newest",
            family,
            "SELECT id, amount FROM fresh WHERE grp = ?1 % 97 ORDER BY created DESC LIMIT 20",
            point / 4,
            vec![Bind::Scatter],
        ),
        write(
            "churn.refill",
            family,
            "INSERT INTO fresh(grp, name, amount, created) \
             VALUES (?1 % 97, 'refill ' || ?2, 1.5, ?2 + 90000)",
            repeat_write,
            Grouping::Single,
            vec![Bind::Int, Bind::Counter],
        ),
    ]
}

/// Returns the queries an application writes most, over the contract's fixture.
///
/// Pagination, a count with a filter, a short `IN` list, a view with a `WHERE`,
/// a dashboard join, a window, a recursive CTE, a JSON filter, statements an
/// ORM prepares on every call, an upsert counter, `RETURNING` and `EXISTS`.
///
/// @param point - how many times a point workload repeats
/// @param scan - how many times a scan workload repeats
/// @param repeat_write - how many times a write workload repeats
fn app_workloads(point: u32, scan: u32, repeat_write: u32) -> Vec<Workload> {
    let family = "hc.app";
    let mut prepared_insert = write(
        "app.insert.prepare_each",
        family,
        "INSERT INTO side_table(owner, note) VALUES (?1, ?2)",
        repeat_write,
        Grouping::Single,
        vec![Bind::Int, Bind::Text],
    );
    prepared_insert.prepare_each = true;
    let mut prepared_join = read(
        "app.join.prepare_each",
        family,
        "SELECT s.note, m.label FROM side_table s JOIN main_table m ON m.id = s.owner \
         WHERE s.id = (?1 % 25000) + 1",
        point / 4,
        vec![Bind::Scatter],
    );
    prepared_join.prepare_each = true;
    vec![
        read(
            "app.page.offset",
            family,
            "SELECT id, label FROM main_table ORDER BY id LIMIT 20 OFFSET (?1 % 250) * 20",
            point / 4,
            vec![Bind::Scatter],
        ),
        read(
            "app.count.where",
            family,
            "SELECT count(*) FROM main_table WHERE category = ?1 % 64",
            point / 8,
            vec![Bind::Scatter],
        ),
        read(
            "app.in.list",
            family,
            "SELECT label FROM main_table WHERE id IN (?1, ?1 + 7, ?1 + 13, ?1 + 101, ?1 + 977)",
            point,
            vec![Bind::Scatter],
        ),
        with_pre(
            read(
                "app.view.where",
                family,
                "SELECT note, label FROM owner_view WHERE id = (?1 % 25000) + 1",
                point / 4,
                vec![Bind::Scatter],
            ),
            "CREATE VIEW IF NOT EXISTS owner_view AS SELECT s.id, s.note, m.label \
             FROM side_table s JOIN main_table m ON m.id = s.owner",
        ),
        read(
            "app.dashboard",
            family,
            "SELECT m.category, count(*), max(s.id) FROM side_table s \
             JOIN main_table m ON m.id = s.owner WHERE m.category < 8 \
             GROUP BY m.category ORDER BY 2 DESC, 1 LIMIT 5",
            scan,
            Vec::new(),
        ),
        read(
            "app.window.rank",
            family,
            "SELECT id, row_number() OVER (ORDER BY key, id) FROM main_table \
             WHERE category = ?1 % 64",
            scan,
            vec![Bind::Scatter],
        ),
        read(
            "app.cte.recursive",
            family,
            "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < 10000) \
             SELECT sum(x) FROM c",
            scan,
            Vec::new(),
        ),
        with_pre(
            read(
                "app.json.where",
                family,
                "SELECT count(*) FROM docs WHERE json_extract(body, '$.k') = ?1 % 100",
                scan,
                vec![Bind::Scatter],
            ),
            "CREATE TABLE IF NOT EXISTS docs(id INTEGER PRIMARY KEY, body TEXT); \
             DELETE FROM docs; \
             INSERT INTO docs(id, body) SELECT id, json_object('k', id % 100, 'name', label, \
             'tags', json_array(category, key % 7)) FROM main_table WHERE id <= 5000",
        ),
        prepared_insert,
        prepared_join,
        with_pre(
            write(
                "app.upsert.counter",
                family,
                "INSERT INTO counters(name, n) VALUES ('c' || (?1 % 100), 1) \
                 ON CONFLICT(name) DO UPDATE SET n = n + 1",
                repeat_write,
                Grouping::Every(100),
                vec![Bind::Int],
            ),
            "CREATE TABLE IF NOT EXISTS counters(name TEXT PRIMARY KEY, n INTEGER NOT NULL); \
             DELETE FROM counters",
        ),
        write(
            "app.insert.returning",
            family,
            "INSERT INTO side_table(owner, note) VALUES (?1, ?2) RETURNING id",
            repeat_write,
            Grouping::Single,
            vec![Bind::Scatter, Bind::Text],
        ),
        read(
            "app.exists",
            family,
            "SELECT EXISTS (SELECT 1 FROM side_table WHERE owner = ?1)",
            point,
            vec![Bind::Scatter],
        ),
        read(
            "app.minmax",
            family,
            "SELECT min(key), max(key) FROM main_table",
            scan,
            Vec::new(),
        ),
        read(
            "app.order.text.limit",
            family,
            "SELECT id FROM main_table ORDER BY label, id LIMIT 10",
            scan,
            Vec::new(),
        ),
    ]
}

/// Returns the edge cases: shapes where the two engines' strategies can be far apart.
///
/// A long `IN` list, a `LIKE` that has to read every row, a high cardinality
/// `GROUP BY`, a self join, an `OR` over two indexes, `NOT IN` a subquery, a
/// compound, a wide row, an empty table, a whole table `UPDATE` that grows
/// every value, and a delete of a key range.
///
/// @param point - how many times a point workload repeats
/// @param scan - how many times a scan workload repeats
fn edge_workloads(point: u32, scan: u32) -> Vec<Workload> {
    let family = "hc.edge";
    let wide_columns: Vec<String> = (0..40).map(|column| format!("c{column}")).collect();
    let wide_values: Vec<String> = (0..40)
        .map(|column| format!("id * {} % 1000", column + 1))
        .collect();
    vec![
        read(
            "edge.in.long",
            family,
            &format!(
                "SELECT count(*), sum(key) FROM main_table WHERE id IN ({})",
                long_in_list(600)
            ),
            scan,
            Vec::new(),
        ),
        read(
            "edge.like.contains",
            family,
            "SELECT count(*) FROM main_table WHERE label LIKE '%9 lorem%'",
            (scan / 4).max(1),
            Vec::new(),
        ),
        read(
            "edge.group.high",
            family,
            "SELECT key % 50000, count(*) FROM main_table GROUP BY 1 ORDER BY 2 DESC, 1 LIMIT 3",
            (scan / 8).max(1),
            Vec::new(),
        ),
        read(
            "edge.self.join",
            family,
            "SELECT count(*) FROM main_table a JOIN main_table b ON b.id = a.key + 1 \
             WHERE a.category = 3",
            (scan / 4).max(1),
            Vec::new(),
        ),
        read(
            "edge.or.two.indexes",
            family,
            "SELECT count(*) FROM main_table WHERE key = ?1 OR id = ?1",
            point / 40,
            vec![Bind::Scatter],
        ),
        read(
            "edge.not.in",
            family,
            "SELECT count(*) FROM side_table WHERE owner NOT IN \
             (SELECT id FROM main_table WHERE category = 1)",
            (scan / 20).max(1),
            Vec::new(),
        ),
        read(
            "edge.union",
            family,
            "SELECT id FROM main_table WHERE key < 500 UNION SELECT owner FROM side_table \
             WHERE id < 500 ORDER BY 1",
            scan,
            Vec::new(),
        ),
        with_pre(
            read(
                "edge.wide.row",
                family,
                "SELECT c39, c0, c20 FROM wide40 WHERE id = (?1 % 2000) + 1",
                point,
                vec![Bind::Scatter],
            ),
            &format!(
                "DROP TABLE IF EXISTS wide40; CREATE TABLE wide40(id INTEGER PRIMARY KEY, {}); \
                 INSERT INTO wide40(id, {}) SELECT id, {} FROM main_table WHERE id <= 2000",
                wide_columns.join(", "),
                wide_columns.join(", "),
                wide_values.join(", ")
            ),
        ),
        with_pre(
            read(
                "edge.empty",
                family,
                "SELECT a, b FROM empty_t WHERE id = ?1 OR a = ?1",
                point,
                vec![Bind::Scatter],
            ),
            "CREATE TABLE IF NOT EXISTS empty_t(id INTEGER PRIMARY KEY, a INTEGER, b TEXT); \
             CREATE INDEX IF NOT EXISTS empty_a ON empty_t(a)",
        ),
        write(
            "edge.update.all",
            family,
            "UPDATE side_table SET note = note || '!'",
            1,
            Grouping::Autocommit,
            Vec::new(),
        ),
        write(
            "edge.delete.range",
            family,
            "DELETE FROM copied WHERE id BETWEEN 20000 AND 59999",
            1,
            Grouping::Autocommit,
            Vec::new(),
        ),
        read(
            "edge.scan.after.delete",
            family,
            "SELECT count(*), sum(key) FROM copied",
            scan,
            Vec::new(),
        ),
    ]
}

/// Returns the reads that check every table the plan built agrees between the two engines.
///
/// The gate compares a digest of what each workload returned, and a write
/// returns nothing, so without these a write that lost a row would still be
/// timed.
fn check_workloads() -> Vec<Workload> {
    let family = "hc.check";
    vec![
        read(
            "check.fresh",
            family,
            "SELECT count(*), sum(id), sum(grp), sum(created), sum(length(name)), \
             total(amount) FROM fresh",
            1,
            Vec::new(),
        ),
        read(
            "check.copied",
            family,
            "SELECT count(*), sum(id), sum(key), sum(length(label)) FROM copied",
            1,
            Vec::new(),
        ),
        read(
            "check.counters",
            family,
            "SELECT count(*), sum(n), sum(length(name)) FROM counters",
            1,
            Vec::new(),
        ),
    ]
}

/// Returns a comma separated list of scattered rowids for a long `IN` list.
///
/// @param count - how many values the list holds
fn long_in_list(count: u32) -> String {
    let values: Vec<String> = (0..count)
        .map(|index| (1 + (u64::from(index) * 2_654_435_761) % 100_000).to_string())
        .collect();
    values.join(",")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The reference driver holds 64 workloads and 8,192 bytes a statement, so a plan over either fails there and not here.
    #[test]
    fn the_plan_fits_the_reference_driver() {
        let plan = plan_for("medium");
        assert!(
            plan.workloads.len() <= 64,
            "{} workloads",
            plan.workloads.len()
        );
        for workload in &plan.workloads {
            assert!(workload.sql.len() < 8_000, "{} is too long", workload.name);
            if let Some(pre) = &workload.pre {
                assert!(pre.len() < 8_000, "{}'s pre is too long", workload.name);
            }
        }
    }

    /// Names are unique, and both sets hold a real share of the plan.
    #[test]
    fn the_split_is_fixed_and_both_sets_are_real() {
        let plan = plan_for("medium");
        let mut names: Vec<&str> = plan.workloads.iter().map(|w| w.name.as_str()).collect();
        names.sort_unstable();
        let total = names.len();
        names.dedup();
        assert_eq!(names.len(), total, "a workload name is used twice");
        let test = plan.workloads.iter().filter(|w| is_test(&w.name)).count();
        assert!(test * 5 >= total, "only {test} of {total} are held out");
        assert!(test * 2 <= total, "{test} of {total} are held out");
    }
}
