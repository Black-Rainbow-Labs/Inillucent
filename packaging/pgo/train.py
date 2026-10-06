"""The Python half of the profile guided build's training run.

python packaging/pgo/train.py <binding .py> <C library> <work dir>

`packaging/pgo/train.mjs` runs this with the instrumented C library. It makes
the calls `docs/performance.md` measures for Python: a table built with
`execute_many`, point selects, a scan, ranges, aggregates, inserts in one
transaction and one at a time, updates, and opening and closing. It times
nothing, and any error stops it, because a binding that fails here would
train the library on a path no application takes.
"""

import importlib.util
import os
import sys

ROWS = 10000


def load_binding(module_path, library):
    """Import the binding from its file with the C library it should load.

    @param module_path - the binding's .py file
    @param library - the C library
    """
    os.environ["INILLUCENT_DRIVER_LIB"] = library
    spec = importlib.util.spec_from_file_location("inillucent_train", module_path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def remove_database(path):
    """Delete a database and every file beside it whose name starts with it: the rollback journal
    and the numbered log segments.

    @param path - the database path
    """
    folder, name = os.path.split(path)
    for entry in os.listdir(folder):
        if entry == name or entry.startswith(name + "-"):
            os.remove(os.path.join(folder, entry))


def build_table(conn):
    """Create the note table, fill it in one transaction and index it.

    @param conn - an open connection
    """
    conn.execute_batch("CREATE TABLE note (id INTEGER PRIMARY KEY, title TEXT, body TEXT, tag TEXT, created INTEGER)")
    rows = [(i, f"title {i}", f"body of note {i} " * 8, f"t{i % 50}", 1700000000 + i * 37) for i in range(1, ROWS + 1)]
    conn.execute_batch("BEGIN")
    conn.execute_many("INSERT INTO note VALUES (?1, ?2, ?3, ?4, ?5)", rows)
    conn.execute_batch("COMMIT")
    conn.execute_batch("CREATE INDEX note_tag ON note(tag); CREATE INDEX note_created ON note(created)")


def read_and_write(conn):
    """The reads and writes an application makes on a built table.

    @param conn - an open connection
    """
    ids = [1 + (i * 7919) % ROWS for i in range(5000)]
    for k in ids:
        conn.execute("SELECT id, title, tag FROM note WHERE id = ?1", (k,)).rows
    conn.execute("SELECT * FROM note").rows
    for k in range(200):
        conn.execute("SELECT id, title, created FROM note WHERE tag = ?1 ORDER BY created", (f"t{k % 50}",)).rows
    for _ in range(100):
        conn.execute("SELECT tag, count(*), max(created) FROM note GROUP BY tag").rows
    rows = [(f"new {i}", "body", f"t{i % 50}", i) for i in range(ROWS)]
    conn.execute_batch("BEGIN")
    conn.execute_many("INSERT INTO note (title, body, tag, created) VALUES (?1, ?2, ?3, ?4)", rows)
    conn.execute_batch("COMMIT")
    for row in rows[:100]:
        conn.execute("INSERT INTO note (title, body, tag, created) VALUES (?1, ?2, ?3, ?4)", row)
    for k in ids[:300]:
        conn.execute("UPDATE note SET created = created + 1 WHERE id = ?1", (k,))


def main():
    """Load the binding, build the table twice, and run the reads and writes."""
    module_path, library, work = sys.argv[1:4]
    binding = load_binding(module_path, library)
    path = os.path.join(work, "python.rdb")
    for _ in range(2):
        remove_database(path)
        db = binding.Database(path)
        conn = db.connect()
        build_table(conn)
        conn.close()
        db.close()
    db = binding.Database(path)
    conn = db.connect()
    read_and_write(conn)
    conn.close()
    db.close()
    for _ in range(100):
        binding.Database(path).close()


if __name__ == "__main__":
    main()
