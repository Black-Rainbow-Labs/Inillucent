"""Run drivers/conformance/suite.json against the Python binding.

The claim this file exists to test is not "Python works". It is that
``drivers/README.md`` and ``include/inillucent_driver.h`` are enough for
somebody who has not read the Rust to write a correct binding - and that claim
is worth nothing until the same suite that grades the Rust driver grades a
second implementation and agrees.

So this runner asserts the same things ``drivers/inillucent-driver/tests/
conformance.rs`` asserts, from the same file. When the two disagree, one of the
bindings is wrong; when they agree, the specification is followable.

Run it with:

    cargo build --manifest-path <repo>/Cargo.toml -p inillucent-driver-capi
    python drivers/bindings/python/run_conformance.py

It exits 0 when every case passed and 1 otherwise, so it can be a step in
something larger.
"""

from __future__ import annotations

import glob
import json
import os
import sys
import tempfile
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import inillucent  # noqa: E402  (the path has to be set first)


def value_of(described):
    """Read a value out of the suite's one-key object form.

    One key rather than a bare literal, so that NULL and the empty string can
    never be confused by the file itself.
    """
    if "null" in described:
        return None
    if "int" in described:
        return int(described["int"])
    if "real" in described:
        return float(described["real"])
    if "text" in described:
        return described["text"]
    if "blob" in described:
        return bytes(described["blob"])
    raise ValueError(f"{described!r} names no value kind")


def same(want, got):
    """Compare an expected value to what came back.

    A float is compared exactly rather than approximately: the suite's floats
    are values that round-trip exactly through IEEE-754, and an approximate
    comparison here would hide a driver that lost precision.
    """
    if want is None or got is None:
        return want is None and got is None
    if isinstance(want, bytes) or isinstance(got, bytes):
        return want == got
    return want == got and type(want) is type(got)


def shown(value):
    """Render a value for a failure message."""
    if value is None:
        return "NULL"
    if isinstance(value, bytes):
        return f"{len(value)} bytes {list(value)}"
    return repr(value)


def check_success(step, rows, wrong):
    """Check a step that was expected to succeed."""
    if "status" in step:
        wrong.append(f"expected it to fail with `{step['status']}` and it succeeded")
        return
    if "columns" in step and step["columns"] != rows.columns:
        wrong.append(f"columns are {rows.columns} and should be {step['columns']}")
    if "rows" in step:
        want = step["rows"]
        if len(want) != len(rows.rows):
            wrong.append(f"there are {len(rows.rows)} rows and there should be {len(want)}")
        else:
            for nth, row in enumerate(want):
                got = rows.rows[nth]
                if len(row) != len(got):
                    wrong.append(f"row {nth} has {len(got)} cells and should have {len(row)}")
                    continue
                for column, cell in enumerate(row):
                    expected = value_of(cell)
                    if not same(expected, got[column]):
                        wrong.append(
                            f"row {nth} column {column} is {shown(got[column])} "
                            f"and should be {shown(expected)}"
                        )
    if "affected" in step:
        want = step["affected"]
        if rows.affected != want:
            wrong.append(f"affected is {rows.affected} and should be {want}")
    if "total" in step and rows.total != step["total"]:
        wrong.append(
            f"total is {rows.total} and should be {step['total']} - and total is exact, "
            "so this is a real disagreement rather than an estimate being off"
        )
    if "more" in step and rows.more != step["more"]:
        wrong.append(f"more is {rows.more} and should be {step['more']}")


def check_failure(step, failure, wrong):
    """Check a step that was expected to fail."""
    if "status" not in step:
        wrong.append(f"it was expected to succeed and it failed: {failure}")
        return
    if failure.status_name != step["status"]:
        wrong.append(
            f"it failed with `{failure.status_name}` and should have failed with "
            f"`{step['status']}` - {failure}"
        )
    if "message_contains" in step and step["message_contains"] not in failure.message:
        wrong.append(
            f"the message is {failure.message!r} and should hold {step['message_contains']!r}"
        )
    if "feature_contains" in step:
        if not failure.feature:
            wrong.append(
                "it named no construct, and a refusal that is `unsupported` has to name "
                "one or an application cannot say what it hit"
            )
        elif step["feature_contains"] not in failure.feature:
            wrong.append(
                f"it named {failure.feature!r} and should have named something holding "
                f"{step['feature_contains']!r}"
            )
    if failure.status == inillucent.UNSUPPORTED and not isinstance(failure, inillucent.Unsupported):
        wrong.append(
            "an unsupported refusal did not arrive as the Unsupported exception, which is "
            "the whole of this design arriving in Python"
        )
    if failure.status == inillucent.UNSUPPORTED and not failure.feature:
        wrong.append("an `unsupported` refusal must carry a feature")


def scratch(name):
    """Return a database path nothing else is using."""
    return os.path.join(
        tempfile.gettempdir(),
        f"inillucent-conformance-py-{name}-{os.getpid()}-{time.time_ns()}.rdb",
    )


def remove_database(path):
    """Remove a database and every file the engine wrote beside it.

    The engine's log is numbered segments, `<path>-wal.0000000001` and on, and
    removing only `path` left one set of them in the temporary directory for
    every case of every run (task-2110, bug 1).
    """
    for candidate in [path, path + "-wal", path + "-journal", path + "-shm"] + glob.glob(
            glob.escape(path) + "-wal.*"):
        try:
            os.remove(candidate)
        except OSError:
            pass


def record(ran, failures):
    """Writes what this runner ran, for the tooling guard to read.

    **Four runners, one specification, and until task-2036 no way to tell which
    of them had actually run it.** npm, Go and PHP each had a round trip of
    their own; this binding and the Rust driver read the suite.
    ``tooling::every_binding_runs_the_whole_suite`` reads these records and
    fails when a runner ran fewer cases than its declared capabilities allow.

    This binding links the C ABI and holds a connection, so it skips nothing:
    ``lacks`` is empty and every case is in ``ran``.

    @param ran - the cases this run graded
    @param failures - what did not hold
    """
    root = os.path.dirname(os.path.dirname(os.path.dirname(
        os.path.dirname(os.path.abspath(__file__)))))
    directory = os.path.join(root, "_agent_output", "conformance")
    os.makedirs(directory, exist_ok=True)
    with open(os.path.join(directory, "python.json"), "w", encoding="utf-8") as handle:
        json.dump({"language": "python", "lacks": [], "ran": ran, "skipped": [],
                   "failures": failures}, handle, indent=2)
        handle.write("\n")


def main():
    """Run every case and report."""
    here = os.path.dirname(os.path.abspath(__file__))
    suite_path = os.path.abspath(os.path.join(here, "..", "..", "conformance", "suite.json"))
    with open(suite_path, encoding="utf-8") as handle:
        suite = json.load(handle)

    print(f"{inillucent.version()}  ABI {inillucent.abi_version()}")
    print(f"suite: {suite_path}\n")

    failures = []
    ran = 0
    for case in suite["cases"]:
        name = case.get("name", "(unnamed)")
        path = scratch(name)
        wrong = []
        database = inillucent.Database(path)
        try:
            connection = database.connect()
            for statement in case.get("setup", []):
                try:
                    connection.execute(statement)
                except inillucent.DriverError as why:
                    wrong.append(f"the setup statement `{statement}` was refused: {why}")
            if not wrong:
                for step in case.get("steps", []):
                    sql = step["sql"]
                    params = [value_of(value) for value in step.get("params", [])]
                    limit = step.get("limit")
                    said = []
                    try:
                        rows = connection.execute(sql, params, limit)
                        check_success(step, rows, said)
                    except inillucent.DriverError as failure:
                        check_failure(step, failure, said)
                    ran += 1
                    for problem in said:
                        wrong.append(f"`{sql}`: {problem}")
        finally:
            database.close()
            remove_database(path)

        print(f"  {'FAIL' if wrong else 'ok  '}  {name}")
        for problem in wrong:
            print(f"          {problem}")
            failures.append(f"{name}: {problem}")

    print(f"\n{len(suite['cases'])} cases, {ran} steps, {len(failures)} failures")
    record([case["name"] for case in suite["cases"]], failures)
    if failures:
        return 1

    # The capability table is the other half of the surface, and a binding that
    # could not read it would have half a driver. Reading it here also proves
    # the C strings it hands back survive being copied out, which is the rule a
    # binding is most likely to get wrong.
    rows = inillucent.capabilities()
    print(f"{len(rows)} capabilities reported")
    unsupported = [row for row in rows if row["supported"] == inillucent.SUPPORT_NO]
    for row in unsupported:
        print(f"  not supported: {row['name']}")
    assert inillucent.supports("cancel") == inillucent.SUPPORT_PARTIAL
    assert inillucent.supports("time_travel") == inillucent.SUPPORT_UNKNOWN, (
        "a capability nobody declared must answer UNKNOWN rather than NO - they mean "
        "different things, and one of them is a checked absence"
    )
    check_encryption()
    check_execute_many()
    check_execute_many_failure()
    check_one_call()
    return 0


def check_encryption():
    """An encrypted database through this binding: written with a key, not
    readable without it or with another one, and holding no plaintext row.

    Outside the suite because the suite opens every case's database the same
    way, and the one thing it cannot show is what happens with a key.
    """
    key = "x'" + "44" * 32 + "'"
    path = scratch("encrypted")
    secret = "the vault code is 7461"
    try:
        database = inillucent.Database(path, key=key)
        connection = database.connect()
        connection.execute("CREATE TABLE vault (code TEXT)")
        connection.execute("INSERT INTO vault VALUES (?1)", [secret])
        database.close()
        for candidate in [path] + glob.glob(glob.escape(path) + "-wal.*"):
            with open(candidate, "rb") as handle:
                assert secret.encode("utf-8") not in handle.read(), f"{candidate} holds the row"
        for attempt in [None, "x'" + "45" * 32 + "'"]:
            try:
                inillucent.Database(path, key=attempt).close()
            except inillucent.DriverError as why:
                assert why.status == inillucent.CORRUPT, f"key {attempt!r} gave status {why.status}"
            else:
                raise AssertionError(f"the database opened with key {attempt!r}")
        database = inillucent.Database(path, key=key)
        rows = database.connect().execute("SELECT code FROM vault")
        assert list(rows)[0][0] == secret, "the row did not come back"
        database.close()
        print("  ok    an encrypted database opens only with its key")
    finally:
        remove_database(path)



def stored_by(rows, through_objects):
    """Insert rows with ``execute_many`` through one of the two paths, and
    return what the table then holds, or the failure as (type, status).

    @param rows - the batch
    @param through_objects - whether the library reads the Python objects
        itself, or the rows cross as JSON
    """
    kept = inillucent._PARAMS
    if not through_objects:
        inillucent._PARAMS = None
    path = scratch("execute_many")
    database = inillucent.Database(path)
    try:
        connection = database.connect()
        connection.execute("CREATE TABLE batch (n INTEGER PRIMARY KEY, a, b)")
        try:
            connection.execute_many("INSERT INTO batch (a, b) VALUES (?1, ?2)", rows)
        except inillucent.DriverError as why:
            return ("failed", why.status)
        return [tuple(row) for row in connection.execute("SELECT a, b, typeof(a) FROM batch ORDER BY n")]
    finally:
        inillucent._PARAMS = kept
        database.close()
        remove_database(path)


def check_execute_many():
    """``execute_many`` stores the same rows whether the library reads the
    Python objects itself or they cross as JSON, and a batch only JSON can
    carry still crosses that way.

    Outside the suite because the suite binds one execution at a time, and the
    question here is the batch call's two ways in.
    """
    if inillucent._PARAMS is None:
        print("  skip  execute_many through Python objects: not available under this Python")
        return
    plain = [
        (None, 0),
        (-1, 2**63 - 1),
        (-(2**63), True),
        (False, 1.5),
        (-0.0, float("inf")),
        ("", "\u00e9t\u00e9 \U0001f600"),
        (b"", b"\x00\xff"),
    ]
    for shape, rows in [
        ("a list of tuples", plain),
        ("a list of lists", [list(row) for row in plain]),
        ("a tuple of tuples", tuple(plain)),
    ]:
        assert inillucent._PARAMS.inillucent_py_params(rows), f"{shape} was not read directly"
        direct, json_path = stored_by(rows, True), stored_by(rows, False)
        assert direct == json_path, f"{shape}: {direct!r} through objects, {json_path!r} as JSON"
    assert stored_by(plain, True)[1][1] == 2**63 - 1
    # Neither of these can be read directly, so both cross as JSON and come
    # out the way they did before there was a second path.
    for name, rows in [
        ("an integer wider than 64 bits", [(2**64, 0)]),
        ("a bytearray", [(bytearray(b"ab"), 0)]),
    ]:
        assert not inillucent._PARAMS.inillucent_py_params(rows), f"{name} was read directly"
        assert stored_by(rows, True) == stored_by(rows, False), f"{name} differs between the paths"
    print("  ok    execute_many stores the same rows through Python objects and through JSON")


def kept_after_failure(rows, through_objects):
    """Run a batch that fails part way inside a transaction, commit what is
    left, and return the failure's status and what the table then holds.

    @param rows - the batch, with a duplicate key in it
    @param through_objects - whether the library reads the Python objects
        itself, or the rows cross as JSON
    """
    kept = inillucent._PARAMS
    if not through_objects:
        inillucent._PARAMS = None
    path = scratch("execute_many_failure")
    database = inillucent.Database(path)
    try:
        connection = database.connect()
        connection.execute("CREATE TABLE batch (n INTEGER PRIMARY KEY, a TEXT)")
        connection.execute("BEGIN")
        status = None
        try:
            connection.execute_many("INSERT INTO batch (n, a) VALUES (?1, ?2)", rows)
        except inillucent.DriverError as why:
            status = why.status
        connection.execute("COMMIT")
        return status, [tuple(row) for row in connection.execute("SELECT n, a FROM batch ORDER BY n")]
    finally:
        inillucent._PARAMS = kept
        database.close()
        remove_database(path)


def check_execute_many_failure():
    """A batch whose one statement fails reports the failing row and keeps
    the rows before it, through Python objects as through JSON.

    The rows read from Python objects move into the engine, so when their one
    statement fails the library reads them from the list again to run them one
    at a time. A second read that came back empty or short would lose the rows
    before the duplicate, which is what this asks about.
    """
    if inillucent._PARAMS is None:
        print("  skip  a failing execute_many through Python objects: not available under this Python")
        return
    rows = [(1, "one"), (2, "two"), (2, "again"), (3, "three")]
    direct, json_path = kept_after_failure(rows, True), kept_after_failure(rows, False)
    assert direct == json_path, f"{direct!r} through objects, {json_path!r} as JSON"
    assert direct[0] is not None, "the duplicate key did not fail the batch"
    assert direct[1] == [(1, "one"), (2, "two")], f"kept {direct[1]!r}"
    print("  ok    a failing execute_many keeps the rows before the failing one, both ways")


def answers_by(statements, one_call):
    """Run statements through one of the two ways an execution crosses, and
    return each one's rows, or its failure as (type, status, message).

    @param statements - (sql, params) pairs, params None for no parameters
    @param one_call - whether each execution is one foreign call, or the
        calls it took before ABI 1.4.0
    """
    kept = (inillucent._STMT_EXECUTE, inillucent._CONN_EXECUTE)
    if not one_call:
        inillucent._STMT_EXECUTE = inillucent._CONN_EXECUTE = None
    path = scratch("one_call")
    database = inillucent.Database(path)
    answers = []
    try:
        connection = database.connect()
        connection.execute("CREATE TABLE t (n INTEGER PRIMARY KEY, a TEXT UNIQUE, b)")
        for sql, params in statements:
            try:
                rows = connection.execute(sql) if params is None else connection.execute(sql, params)
                answers.append((rows.columns, [tuple(row) for row in rows], rows.affected, rows.tag))
            except (inillucent.DriverError, TypeError) as why:
                answers.append((type(why).__name__, getattr(why, "status", None), str(why)))
        return answers
    finally:
        inillucent._STMT_EXECUTE, inillucent._CONN_EXECUTE = kept
        database.close()
        remove_database(path)


def check_one_call():
    """An execution gives the same rows and the same failures whether it is
    one foreign call or the four it took before ABI 1.4.0.

    The suite above runs every case through the one call when the library has
    it; this asks about the cases the two ways in could disagree on: every
    value type, a constraint failure, a syntax error, too many values, a value
    neither way can bind, and a statement with no parameters.
    """
    if inillucent._STMT_EXECUTE is None:
        print("  skip  one call per execution: not available with this library or Python")
        return
    statements = [
        ("INSERT INTO t (a, b) VALUES (?1, ?2)", ("x", 1)),
        ("INSERT INTO t (a, b) VALUES (?1, ?2)", ("x", 2)),
        ("SELECT ?1, ?2, ?3, ?4, ?5, ?6", (None, 2**63 - 1, -1.5, "\u00e9t\u00e9", b"\x00\xff", True)),
        ("SELEC 1", ()),
        ("SELECT ?1", (1, 2)),
        ("SELECT ?1", (2**64,)),
        ("SELECT ?1", (bytearray(b"ab"),)),
        ("SELECT n, a, b FROM t ORDER BY n", None),
        ("UPDATE t SET b = b + 1", None),
        ("SELECT count(*) FROM t WHERE a = ?1", ["x"]),
    ]
    direct, before = answers_by(statements, True), answers_by(statements, False)
    for nth, (one, other) in enumerate(zip(direct, before)):
        assert one == other, f"statement {nth}: {one!r} in one call, {other!r} the old way"
    print("  ok    one call per execution answers as the calls before it did")


if __name__ == "__main__":
    sys.exit(main())
