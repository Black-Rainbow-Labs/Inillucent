"""Defects found in the Python driver and its command line helpers, one test each.

Invariant: **a call either does what its name says or raises an error that says
why.** Each test here was a call that did something else: ran one statement of
two and reported success, decoded a result with the wrong code page, leaked a
database after a refused close, or bound a dict by its keys.

Unlike ``test_wheel.py`` this suite does not need a staged wheel. It builds a
package in a temporary directory from the source files, the reference binding
and two built artifacts it is told about:

- ``INILLUCENT_BIN``, the ``inillucent`` command line program;
- ``INILLUCENT_DRIVER_LIB``, the C ABI library.

When either is missing the suite skips with the command that builds it, the way
the other package suites do. Run it with::

    python packages/python/tests/test_driver_defects.py

or with pytest.
"""

from __future__ import annotations

import ctypes
import gc
import importlib
import os
import shutil
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
SOURCE = HERE.parent / "src" / "inillucent"
REFERENCE = ROOT / "drivers" / "bindings" / "python" / "inillucent.py"
SUFFIX = ".exe" if os.name == "nt" else ""

#: The package once it has been staged, or None before the first test needs it.
_STAGED: dict = {}


def _built(name: str, variable: str) -> Path | None:
    """Return a built artifact, from its environment variable or ``target/``.

    :param name: the file name to look for under ``target/debug`` and ``target/release``
    :param variable: the environment variable that names it
    """
    named = os.environ.get(variable)
    if named:
        return Path(named)
    for profile in ("debug", "release"):
        candidate = ROOT / "target" / profile / name
        if candidate.is_file():
            return candidate
    return None


def _library() -> Path | None:
    """Return the C ABI library, or None when it has not been built."""
    names = {
        "nt": "inillucent_driver_capi.dll",
        "posix": "libinillucent_driver_capi.so",
    }
    found = _built(names[os.name], "INILLUCENT_DRIVER_LIB")
    if found is None and sys.platform == "darwin":
        found = _built("libinillucent_driver_capi.dylib", "INILLUCENT_DRIVER_LIB")
    return found


def _stage() -> dict | None:
    """Build the package in a temporary directory and import it, once.

    The package is the source ``__init__.py`` beside the reference binding
    copied in as ``driver.py``, which is what ``build.py`` makes, and the
    command line program under ``_bin``. Returns None when a prerequisite is
    missing.
    """
    if _STAGED:
        return _STAGED
    program = _built("inillucent" + SUFFIX, "INILLUCENT_BIN")
    library = _library()
    if program is None or library is None or not program.is_file() or not library.is_file():
        return None
    root = Path(tempfile.mkdtemp(prefix="inillucent-defects-"))
    package = root / "inillucent"
    (package / "_bin").mkdir(parents=True)
    shutil.copy2(SOURCE / "__init__.py", package / "__init__.py")
    shutil.copy2(SOURCE / "__main__.py", package / "__main__.py")
    shutil.copy2(REFERENCE, package / "driver.py")
    shutil.copy2(program, package / "_bin" / ("inillucent" + SUFFIX))
    os.environ["INILLUCENT_DRIVER_LIB"] = str(library)
    sys.path.insert(0, str(root))
    module = importlib.import_module("inillucent")
    _STAGED.update(root=root, module=module, driver=module._driver)
    return _STAGED


def _need() -> dict:
    """Return the staged package, or skip the calling test."""
    staged = _stage()
    if staged is None:
        message = (
            "INILLUCENT_BIN and INILLUCENT_DRIVER_LIB are not both available; build them with "
            "`cargo build -p inillucent-cli -p inillucent-driver-capi`"
        )
        try:
            import pytest  # noqa: PLC0415

            pytest.skip(message)
        except ImportError:
            raise _Skip(message) from None
    return staged


class _Skip(Exception):
    """Raised by ``_need`` under the plain runner, which reports it as a skip."""


def _database(tag: str):
    """Return a new Database in a directory of its own, and the directory.

    :param tag: a word for the directory name
    """
    staged = _need()
    directory = Path(tempfile.mkdtemp(prefix=f"inillucent-{tag}-", dir=staged["root"]))
    return staged["module"].Database(str(directory / "probe.rdb")), directory


def test_a_second_statement_is_refused_and_nothing_runs() -> None:
    """``INSERT; INSERT; garbage`` raised nothing and ran only the first insert.

    Python's ``sqlite3`` raises for more than one statement. ``execute_batch`` is
    the call for several, so a script is not lost by this.
    """
    module = _need()["module"]
    database, _ = _database("multi")
    connection = database.connect()
    connection.execute("CREATE TABLE t (id INTEGER PRIMARY KEY, a)")
    try:
        connection.execute("INSERT INTO t VALUES (1,'a'); INSERT INTO t VALUES (2,'b'); not sql")
    except module.DriverError as refused:
        assert refused.status == module.driver.INVALID_STATE, refused
    else:
        raise AssertionError("two statements were accepted")
    assert connection.execute("SELECT count(*) FROM t").rows == [[0]]
    try:
        connection.execute("INSERT INTO t VALUES (3,?1); INSERT INTO t VALUES (4,?1)", ["z"])
    except module.DriverError as refused:
        assert refused.status == module.driver.INVALID_STATE, refused
    else:
        raise AssertionError("two statements with parameters were accepted")
    # One statement may end in a semicolon, a comment or white space.
    connection.execute("INSERT INTO t VALUES (5,'x');")
    connection.execute("INSERT INTO t VALUES (6,'y'); -- done\n")
    connection.execute("CREATE TABLE log (n); ")
    # A trigger body holds semicolons and is still one statement.
    connection.execute(
        "CREATE TRIGGER t_after AFTER INSERT ON t BEGIN INSERT INTO log VALUES (new.id); "
        "INSERT INTO log VALUES (new.id); END"
    )
    connection.execute("INSERT INTO t VALUES (7,'z')")
    assert connection.execute("SELECT count(*) FROM log").rows == [[2]]
    connection.execute_batch("INSERT INTO t VALUES (8,'a'); INSERT INTO t VALUES (9,'b')")
    assert connection.execute("SELECT count(*) FROM t").rows == [[5]]
    database.close()


def test_the_helpers_read_utf8_and_send_it_as_utf8() -> None:
    """``query`` decoded the program's output as cp1252 on Windows."""
    module = _need()["module"]
    database, directory = _database("utf8")
    path = str(directory / "probe.rdb")
    database.close()
    for text in ("Ł", "Ā", "中文", "héllo 日本語 \U0001F600"):
        rows = module.query("SELECT ?1 AS v", db=path, params=[text])
        assert rows == [{"v": text}], (text, rows)
    rows = module.query("SELECT 'héllo 日本'  AS v", db=path)
    assert rows == [{"v": "héllo 日本"}], rows
    made = module.run("exec", db=path, sql="CREATE TABLE t (a)")
    assert made["ok"], made
    written = module.run("exec", db=path, sql="INSERT INTO t VALUES (?1)", params=["日本"])
    assert written["ok"], written
    assert module.query("SELECT a FROM t", db=path) == [{"a": "日本"}]


def test_the_helpers_send_bytes_nan_and_many_values() -> None:
    """``run`` and ``query`` could not send bytes, NaN, many values or long text."""
    module = _need()["module"]
    database, directory = _database("helper")
    path = str(directory / "probe.rdb")
    database.close()
    rows = module.query("SELECT typeof(?1) AS t, hex(?1) AS h", db=path, params=[b"ab"])
    assert rows == [{"t": "blob", "h": "6162"}], rows
    rows = module.query("SELECT typeof(?1) AS t", db=path, params=[float("nan")])
    assert rows == [{"t": "null"}], "NaN is stored as NULL, as SQLite does"
    rows = module.query("SELECT ?1 AS v", db=path, params=[-0.0])
    assert rows == [{"v": -0.0}], rows
    count = module.query("SELECT ?1 AS v, ?20000 AS w", db=path, params=list(range(20000)))
    assert count == [{"v": 0, "w": 19999}], count
    long_text = "x" * 1_000_000
    rows = module.query("SELECT length(?1) AS n", db=path, params=[long_text])
    assert rows == [{"n": 1_000_000}], rows
    for refused, kind in (
        ([[1, 2]], TypeError),
        ([{"blob": "00"}], TypeError),
        ([float("inf")], ValueError),
        (["a\ud800b"], ValueError),
    ):
        try:
            module.query("SELECT ?1", db=path, params=refused)
        except kind:
            continue
        raise AssertionError(f"{refused!r} was accepted")
    try:
        module.query("SELECT " + "+".join(["1"] * 20000), db=path)
    except ValueError as too_long:
        assert "command line" in str(too_long), too_long
    else:
        raise AssertionError("a statement past the command line limit was sent")


def test_a_refused_close_keeps_the_handle_so_a_later_close_works() -> None:
    """A refused ``Database.close`` dropped the handle and leaked the database."""
    module = _need()["module"]
    database, _ = _database("close")
    connection = database.connect()
    connection.execute("CREATE TABLE t (a)")
    statement = connection.prepare("SELECT * FROM t")
    try:
        database.close()
    except module.DriverError as refused:
        assert refused.status == module.driver.INVALID_STATE, refused
    else:
        raise AssertionError("a close with a live statement was accepted")
    assert database._handle, "the refused close dropped the handle"
    statement.close()
    database.close()
    assert not database._handle
    database.close()


def test_a_database_closes_when_the_with_block_ends_and_the_statement_is_gone() -> None:
    """``with Database() as db`` must not leak when a statement was alive at exit."""
    module = _need()["module"]
    staged = _need()
    directory = Path(tempfile.mkdtemp(prefix="inillucent-with-", dir=staged["root"]))
    path = str(directory / "probe.rdb")

    def use() -> None:
        try:
            with module.Database(path) as database:
                connection = database.connect()
                connection.execute("CREATE TABLE t (a)")
                connection.execute("INSERT INTO t VALUES (1)")
                held = connection.prepare("SELECT * FROM t")
                assert held
        except module.DriverError:
            pass

    use()
    gc.collect()
    with module.Database(path) as again:
        assert again.connect().execute("SELECT count(*) FROM t").rows == [[1]]


def test_closed_connections_are_not_kept_by_the_database() -> None:
    """``Database._connections`` kept every connection ever made."""
    database, _ = _database("leak")
    for _ in range(200):
        connection = database.connect()
        connection.execute("SELECT 1")
        connection.close()
    del connection
    gc.collect()
    assert len(database._connections) == 0, len(database._connections)
    held = database.connect()
    assert len(database._connections) == 1
    database.close()
    assert held._handle is None


def test_nan_is_bound_as_null_on_every_path() -> None:
    """A NaN parameter was stored as a REAL; SQLite's ``sqlite3_bind_double`` stores NULL."""
    staged = _need()
    driver = staged["driver"]
    names = ("_PY", "_CALLS", "_PARAMS", "_STMT_EXECUTE", "_CONN_EXECUTE", "_JSON")
    saved = {name: getattr(driver, name) for name in names}
    nan = float("nan")
    try:
        for mode in ("fast", "json", "accessors"):
            if mode != "fast":
                for name in names[:-1]:
                    setattr(driver, name, None)
            if mode == "accessors":
                driver._JSON = False
            database, _ = _database("nan" + mode)
            connection = database.connect()
            connection.execute("CREATE TABLE t (i INTEGER PRIMARY KEY, a)")
            connection.execute("INSERT INTO t VALUES (1, ?1)", [nan])
            connection.execute_many("INSERT INTO t VALUES (?1, ?2)", [[2, nan], [3, 1.5]])
            rows = connection.execute("SELECT i, typeof(a) FROM t ORDER BY i").rows
            assert rows == [[1, "null"], [2, "null"], [3, "real"]], (mode, rows)
            database.close()
    finally:
        for name, value in saved.items():
            setattr(driver, name, value)


def test_parameters_are_bound_by_position_and_a_dict_is_refused() -> None:
    """A dict was bound by its keys, a string by its characters."""
    database, _ = _database("params")
    connection = database.connect()
    connection.execute("CREATE TABLE t (id INTEGER PRIMARY KEY, a, b)")
    for refused in ({"a": 1}, "ab", b"ab"):
        try:
            connection.execute("SELECT ?1, ?2", refused)
        except TypeError:
            continue
        raise AssertionError(f"{refused!r} was accepted as parameters")
    rows = ((50 + i, i, i) for i in range(3))
    assert connection.execute_many("INSERT INTO t VALUES (?1, ?2, ?3)", rows) == 3
    assert connection.execute("SELECT count(*) FROM t").rows == [[3]]
    try:
        connection.execute_many("INSERT INTO t VALUES (?1, ?2, ?3)", ["abc"])
    except TypeError:
        pass
    else:
        raise AssertionError("a string was accepted as a row")
    assert connection.execute("SELECT count(*) FROM t").rows == [[3]]
    database.close()


def test_paths_and_statements_of_the_wrong_type_raise_type_error() -> None:
    """``Database(Path)`` and ``execute(bytes)`` raised ``AttributeError``."""
    module = _need()["module"]
    staged = _need()
    directory = Path(tempfile.mkdtemp(prefix="inillucent-types-", dir=staged["root"]))
    database = module.Database(directory / "probe.rdb")
    connection = database.connect()
    assert connection.execute("SELECT 1").rows == [[1]]
    database.backup_to(directory / "copy.rdb")
    assert (directory / "copy.rdb").is_file()
    for bad in (b"SELECT 1", None, 7):
        for call in (connection.execute, connection.prepare, connection.execute_batch):
            try:
                call(bad)
            except TypeError:
                continue
            raise AssertionError(f"{call.__name__}({bad!r}) was accepted")
    # Text that reaches a C string is checked for a NUL, which would end it early.
    for call in (connection.prepare, connection.execute_batch):
        try:
            call("SELECT 1; \x00 SELECT 2")
        except ValueError:
            continue
        raise AssertionError(f"{call.__name__} accepted a NUL in the SQL")
    for bad in (7, None, 1.5):
        try:
            module.Database(bad)
        except TypeError:
            continue
        raise AssertionError(f"Database({bad!r}) was accepted")
    try:
        module.Database(str(directory / "junk") + "\x00tail")
    except ValueError:
        pass
    else:
        raise AssertionError("a path with a NUL was accepted")
    assert not (directory / "junk").exists(), "the path was truncated at the NUL"
    database.close()
    module.Database(str(directory / "probe.rdb").encode()).close()


def test_a_missing_file_opened_read_only_is_not_found() -> None:
    """Opening a missing file read only answered ``syntax``."""
    staged = _need()
    module = staged["module"]
    directory = Path(tempfile.mkdtemp(prefix="inillucent-ro-", dir=staged["root"]))
    for create in (True, False):
        try:
            module.Database(directory / "missing.rdb", create=create, read_only=True)
        except module.DriverError as refused:
            assert refused.status == module.driver.NOT_FOUND, (create, refused)
        else:
            raise AssertionError("a missing file opened")
    assert not (directory / "missing.rdb").exists()


def test_a_limit_past_the_unsigned_range_is_refused() -> None:
    """``limit=2**64`` wrapped to 0 and returned no rows with a success status."""
    database, _ = _database("limit")
    connection = database.connect()
    connection.execute("CREATE TABLE n (i INTEGER PRIMARY KEY)")
    connection.execute_batch("INSERT INTO n VALUES (1); INSERT INTO n VALUES (2); INSERT INTO n VALUES (3)")
    for refused in (2**64, 2**100):
        try:
            connection.execute("SELECT i FROM n", limit=refused)
        except ValueError:
            continue
        raise AssertionError(f"limit {refused} was accepted")
    assert len(connection.execute("SELECT i FROM n", limit=2**64 - 1).rows) == 3
    assert len(connection.execute("SELECT i FROM n", limit=-1).rows) == 3
    assert len(connection.execute("SELECT i FROM n WHERE i > ?1", [0], limit=2).rows) == 2
    try:
        connection.execute("SELECT i FROM n WHERE i > ?1", [0], limit=2**64)
    except ValueError:
        pass
    else:
        raise AssertionError("a limit past 2**64 - 1 was accepted with parameters")
    database.close()


def test_a_transaction_statement_in_the_wrong_state_is_invalid_state() -> None:
    """``BEGIN`` inside a transaction and ``COMMIT`` with none answered ``syntax``."""
    module = _need()["module"]
    database, _ = _database("state")
    connection = database.connect()
    for sql in ("COMMIT", "ROLLBACK"):
        try:
            connection.execute(sql)
        except module.DriverError as refused:
            assert refused.status == module.driver.INVALID_STATE, (sql, refused)
        else:
            raise AssertionError(f"{sql} with no transaction was accepted")
    connection.execute("BEGIN")
    try:
        connection.execute("BEGIN")
    except module.DriverError as refused:
        assert refused.status == module.driver.INVALID_STATE, refused
    else:
        raise AssertionError("BEGIN inside a transaction was accepted")
    connection.execute("ROLLBACK")
    try:
        connection.execute("SELEC 1")
    except module.DriverError as refused:
        assert refused.status == module.driver.SYNTAX, refused
    database.close()


def test_a_bind_with_an_impossible_length_returns_an_error() -> None:
    """``inillucent_bind_text`` aborted the process for a length of 2**64 - 1 or 2**40."""
    staged = _need()
    driver = staged["driver"]
    database, _ = _database("bind")
    connection = database.connect()
    statement = connection.prepare("SELECT ?1")
    too_big = driver.TOO_BIG
    for length in (2**64 - 1, 2**40, 2**31):
        got = driver._LIB.inillucent_bind_text(statement._handle, 1, b"abc", length)
        assert got == too_big, (length, got)
        buffer = (ctypes.c_uint8 * 3)(1, 2, 3)
        got = driver._LIB.inillucent_bind_blob(statement._handle, 1, buffer, length)
        assert got == too_big, (length, got)
    assert statement.execute(["still fine"]).rows == [["still fine"]]
    statement.close()
    database.close()


def test_two_threads_never_run_inside_one_database_at_once() -> None:
    """A second thread on a connection ran inside the engine with the first.

    ``ctypes`` lets the interpreter lock go while the engine runs, and nothing
    else stopped a second thread, so a call made while another statement ran
    reached the engine's busy cell and failed with "already running a
    statement", or corrupted its counts. Every call on a database now takes one
    lock that the database owns: the second thread waits and then runs, and
    neither fails.
    """
    import threading
    import time

    module = _need()["module"]
    database, _ = _database("threads")
    connection = database.connect()
    slow = (
        "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 3000000) "
        "SELECT count(*) FROM n"
    )
    started = threading.Event()
    stamps: dict = {}
    failures: list = []

    def first() -> None:
        stamps["begin"] = time.monotonic()
        started.set()
        try:
            stamps["first"] = connection.execute(slow).rows
        except Exception as failure:  # noqa: BLE001 - the test reports whatever happened
            failures.append(("first", failure))
        stamps["first_end"] = time.monotonic()

    def second() -> None:
        started.wait()
        time.sleep(0.2)
        try:
            stamps["second"] = connection.execute("SELECT 41 + 1").rows
        except Exception as failure:  # noqa: BLE001
            failures.append(("second", failure))
        stamps["second_end"] = time.monotonic()

    threads = [threading.Thread(target=first), threading.Thread(target=second)]
    for thread in threads:
        thread.start()
    for thread in threads:
        thread.join()
    assert not failures, failures
    assert stamps["first"] == [[3000000]], stamps
    assert stamps["second"] == [[42]], stamps
    # The first call must have run long enough for the second to arrive in the middle of it.
    assert stamps["first_end"] - stamps["begin"] > 0.5, stamps
    # A margin for the instant between the first call returning and its time being read.
    assert stamps["second_end"] >= stamps["first_end"] - 0.05, "the second call ran inside the first"
    # `cancel` is the one call that does not wait, so it can be made while a statement runs.
    assert callable(connection.cancel)
    database.close()
    assert module is not None


def test_two_connections_of_one_database_have_a_transaction_each() -> None:
    """Two connections of one ``Database`` shared one transaction.

    A ``BEGIN`` on one was joined by the other, the other read its uncommitted
    rows, and a write through the second was undone by the first one's
    ``ROLLBACK``, so an acknowledged write was lost. Python's ``sqlite3`` and
    SQLite isolate two connections and refuse the second writer with busy.
    """
    module = _need()["module"]
    staged = _need()
    directory = Path(tempfile.mkdtemp(prefix="inillucent-isolation-", dir=staged["root"]))
    database = module.Database(str(directory / "probe.rdb"), busy_timeout_ms=200)
    first = database.connect()
    second = database.connect()
    first.execute("CREATE TABLE t (a INTEGER)")
    first.execute("BEGIN")
    first.execute("INSERT INTO t VALUES (1)")
    assert first.execute("SELECT count(*) FROM t").rows == [[1]]
    assert second.execute("SELECT count(*) FROM t").rows == [[0]], "an uncommitted row was read"
    assert first.in_transaction and not second.in_transaction
    try:
        second.execute("INSERT INTO t VALUES (2)")
    except module.DriverError as refused:
        assert refused.status == module.driver.BUSY, refused
    else:
        raise AssertionError("a second writer was accepted while the first held the file")
    first.execute("ROLLBACK")
    assert second.execute("SELECT count(*) FROM t").rows == [[0]]
    # A write through the second connection is kept when the first rolls back.
    first.execute("BEGIN")
    second.execute("INSERT INTO t VALUES (7)")
    first.execute("ROLLBACK")
    assert first.execute("SELECT count(*) FROM t").rows == [[1]], "an acknowledged write was lost"
    assert second.execute("SELECT count(*) FROM t").rows == [[1]]
    # Closing a connection gives up its transaction, and the file goes to the next writer.
    first.execute("BEGIN")
    first.execute("INSERT INTO t VALUES (8)")
    first.close()
    second.execute("INSERT INTO t VALUES (9)")
    assert second.execute("SELECT count(*) FROM t").rows == [[2]]
    database.close()


def main() -> int:
    """Run every test in this file, for a machine with no pytest."""
    failures = []
    skipped = 0
    for name, test in sorted(globals().items()):
        if not name.startswith("test_") or not callable(test):
            continue
        try:
            test()
            print(f"  ok   {name}")
        except _Skip as why:
            skipped += 1
            print(f"  skip {name}: {why}", file=sys.stderr)
        except Exception as failure:  # noqa: BLE001 - a runner reports everything
            failures.append(f"{name}: {type(failure).__name__}: {failure}")
            print(f"  FAIL {name}")
    for failure in failures:
        print(f"\n{failure}", file=sys.stderr)
    if skipped and os.environ.get("INILLUCENT_STRICT") == "1":
        return 1
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
