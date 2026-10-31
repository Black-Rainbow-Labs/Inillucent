import Foundation
import XCTest
@testable import Inillucent

/// What the conformance suite cannot ask, because it opens every case's database the same way and
/// runs on one thread: a key, a reopened file, a transaction rolled back by a throw, text Swift's own
/// string comparison would merge, a cancel from a second thread, and the rows a failing statement
/// produced before it failed. The same list as the Android binding's `DatabaseTest.kt`.
final class DatabaseTests: XCTestCase {
    private var folder: URL!

    /// A folder of the test's own, so the engine's log segments cannot collide with another test's.
    override func setUpWithError() throws {
        folder = FileManager.default.temporaryDirectory.appendingPathComponent("database-tests-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
    }

    /// Removes the folder and everything the engine wrote in it.
    override func tearDownWithError() throws {
        try? FileManager.default.removeItem(at: folder)
    }

    /// The library loads and names an ABI this binding was written for.
    func testTheLibraryReportsItsVersion() {
        XCTAssertTrue(Inillucent.version.hasPrefix("inillucent"), Inillucent.version)
        XCTAssertEqual(Inillucent.abiVersion.split(separator: ".").first.map(String.init), String(Inillucent.abiMajor))
    }

    /// Rows written and closed are there when the file is opened again, and a read only open refuses
    /// a write.
    func testAClosedDatabaseOpensAgainWithItsRows() throws {
        let file = folder.appendingPathComponent("reopen.rdb")
        do {
            let db = try Database(url: file)
            let conn = try db.session()
            try conn.execute("CREATE TABLE people (id INTEGER PRIMARY KEY, name TEXT)")
            XCTAssertEqual(try conn.executeMany("INSERT INTO people (name) VALUES (?1)", [["Ada"], ["Grace"], ["Hedy"]]), 3)
            XCTAssertEqual(try conn.lastInsertRowid, 3)
            conn.close()
            try db.checkpoint()
            try db.integrityCheck()
            try db.close()
        }
        let db = try Database(url: file, readOnly: true)
        let conn = try db.session()
        XCTAssertEqual(try conn.query("SELECT name FROM people ORDER BY id").rows, [["Ada"], ["Grace"], ["Hedy"]])
        XCTAssertThrowsError(try conn.execute("DELETE FROM people")) { error in
            XCTAssertEqual((error as? InillucentError)?.status, .readonly)
        }
        conn.close()
        try db.close()
    }

    /// An encrypted file holds no plaintext, and opens only with its own key.
    func testAnEncryptedDatabaseOpensOnlyWithItsKey() throws {
        let file = folder.appendingPathComponent("vault.rdb")
        let key = "x'" + String(repeating: "44", count: 32) + "'"
        let secret = "the vault code is 7461"
        do {
            let db = try Database(url: file, key: key)
            let conn = try db.session()
            try conn.execute("CREATE TABLE vault (code TEXT)")
            try conn.execute("INSERT INTO vault VALUES (?1)", [.text(secret)])
            conn.close()
            try db.close()
        }
        for name in try FileManager.default.contentsOfDirectory(atPath: folder.path) {
            let bytes = try Data(contentsOf: folder.appendingPathComponent(name))
            XCTAssertNil(bytes.range(of: Data(secret.utf8)), "\(name) holds the row in plaintext")
        }
        for wrong in [nil, "x'" + String(repeating: "45", count: 32) + "'"] {
            XCTAssertThrowsError(try Database(url: file, key: wrong)) { error in
                XCTAssertEqual((error as? InillucentError)?.status, .corrupt)
            }
        }
        let db = try Database(url: file, key: key)
        let conn = try db.session()
        XCTAssertEqual(try conn.query("SELECT code FROM vault").rows[0][0], .text(secret))
        conn.close()
        try db.close()
    }

    /// A throw inside `transaction` keeps none of its writes; a return keeps all of them.
    func testATransactionRollsBackWhenItsWorkThrows() throws {
        struct ChangedMind: Error {}
        let db = try Database(url: folder.appendingPathComponent("txn.rdb"))
        let conn = try db.session()
        try conn.execute("CREATE TABLE t (n INTEGER PRIMARY KEY)")
        XCTAssertThrowsError(try conn.transaction { txn -> Void in
            XCTAssertEqual(try txn.execute("INSERT INTO t VALUES (1)"), 1)
            XCTAssertTrue(try conn.inTransaction)
            throw ChangedMind()
        })
        XCTAssertFalse(try conn.inTransaction)
        XCTAssertEqual(try conn.query("SELECT count(*) FROM t").rows[0][0], 0)
        let changed = try conn.transaction { txn -> [UInt64] in
            try txn.execute("INSERT INTO t VALUES (1)")
            try txn.execute("INSERT INTO t VALUES (2)")
            return txn.affected
        }
        XCTAssertEqual(changed, [1, 1])
        XCTAssertEqual(try conn.query("SELECT count(*) FROM t").rows[0][0], 2)
        conn.close()
        try db.close()
    }

    /// Text with an emoji and a NUL byte, the two forms of an accented letter, and a blob all come back
    /// as they went in, and the two forms stay two values.
    func testTextComesBackByteForByte() throws {
        let db = try Database(url: folder.appendingPathComponent("text.rdb"))
        let conn = try db.session()
        try conn.execute("CREATE TABLE \"t\u{e4}ble \u{1F600}\" (v)")
        let text = "\u{e9}t\u{e9} \u{1F600} a\u{0}b"
        let composed = "\u{e9}"
        let decomposed = "e\u{301}"
        try conn.execute("INSERT INTO \"t\u{e4}ble \u{1F600}\" VALUES (?1), (?2), (?3), (?4), (?5), (?6)", [.text(text), .blob(Data([0, 255, 1, 0])), .integer(Int64.min), .real(-0.0), .text(composed), .text(decomposed)])
        let rows = try conn.query("SELECT v, typeof(v) FROM \"t\u{e4}ble \u{1F600}\" ORDER BY rowid")
        XCTAssertEqual(rows.columns, ["v", "typeof(v)"])
        XCTAssertEqual(rows.rows[0][0].text.map { Array($0.utf8) }, Array(text.utf8))
        XCTAssertEqual(rows.rows[1][0], .blob(Data([0, 255, 1, 0])))
        XCTAssertEqual(rows.rows[2][0], .integer(Int64.min))
        XCTAssertEqual(rows.rows[3][1], "real")
        XCTAssertEqual(rows.rows[4][0].text.map { Array($0.utf8) }, Array(composed.utf8))
        XCTAssertEqual(rows.rows[5][0].text.map { Array($0.utf8) }, Array(decomposed.utf8))
        XCTAssertNotEqual(rows.rows[4][0], rows.rows[5][0], "Value merged the two forms the engine keeps apart")
        XCTAssertEqual(try conn.query("SELECT count(DISTINCT v) FROM \"t\u{e4}ble \u{1F600}\" WHERE rowid > 4").rows[0][0], 2)
        conn.close()
        try db.close()
    }

    /// The capability table reads, `cancel` is partial, and a name nobody declared is unknown.
    func testTheCapabilityTableReads() {
        let table = Inillucent.capabilities
        XCTAssertGreaterThanOrEqual(table.count, 40)
        XCTAssertTrue(table.allSatisfy { !$0.name.isEmpty && !$0.note.isEmpty })
        XCTAssertEqual(Inillucent.supports("cancel"), .partial)
        XCTAssertEqual(Inillucent.supports("time_travel"), .unknown)
    }

    /// A cancel from a second thread stops a long statement, and the connection still works after.
    func testACancelFromAnotherThreadStopsALongStatement() throws {
        let db = try Database(url: folder.appendingPathComponent("cancel.rdb"))
        let conn = try db.session()
        let finished = Flag()
        let canceller = Thread {
            // A cancel before the statement starts is cleared when it starts, so keep asking until
            // it has stopped.
            while !finished.isSet {
                Thread.sleep(forTimeInterval: 0.1)
                try? conn.cancel()
            }
        }
        canceller.start()
        let started = Date()
        XCTAssertThrowsError(try conn.query("WITH RECURSIVE c(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM c WHERE n < 100000000) SELECT count(*) FROM c")) { error in
            XCTAssertEqual((error as? InillucentError)?.status, .interrupted)
        }
        finished.set()
        XCTAssertLessThan(Date().timeIntervalSince(started), 60)
        XCTAssertEqual(try conn.query("SELECT 1").rows[0][0], 1)
        conn.close()
        try db.close()
    }

    /// A statement that fails part way hands back the rows before the failure, with the failure.
    func testAFailingStatementCarriesTheRowsBeforeIt() throws {
        let db = try Database(url: folder.appendingPathComponent("partial.rdb"))
        let conn = try db.session()
        try conn.executeBatch("CREATE TABLE t(a); WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM c WHERE x<5000) INSERT INTO t SELECT x FROM c")
        XCTAssertThrowsError(try conn.query("SELECT a, CASE WHEN a = 3000 THEN json('x') END FROM t", limit: 10)) { error in
            guard let partial = (error as? InillucentError)?.partialRows else { return XCTFail("the failure carried no rows") }
            XCTAssertEqual(partial.count, 10)
            XCTAssertEqual(partial.total, 2999)
            XCTAssertTrue(partial.more)
            XCTAssertEqual(partial.rows[0][0], 1)
        }
        conn.close()
        try db.close()
    }

    /// An unbuilt construct is `isUnsupported` and names itself, and a typo is not.
    func testUnsupportedIsItsOwnStatus() throws {
        let db = try Database(url: folder.appendingPathComponent("status.rdb"))
        let conn = try db.session()
        XCTAssertThrowsError(try conn.query("EXPLAIN EXPLAIN SELECT 1")) { error in
            let failure = error as? InillucentError
            XCTAssertEqual(failure?.isUnsupported, true)
            XCTAssertFalse((failure?.feature ?? "").isEmpty)
        }
        XCTAssertThrowsError(try conn.query("SELEC 1")) { error in
            XCTAssertEqual((error as? InillucentError)?.status, .syntax)
        }
        XCTAssertThrowsError(try conn.query("SELECT 1\u{0}; DROP TABLE x")) { error in
            XCTAssertEqual((error as? InillucentError)?.status, .invalidState)
        }
        conn.close()
        try db.close()
    }

    /// A database refuses to close while a connection is open, and stays usable.
    func testCloseRefusesWhileAConnectionIsOpen() throws {
        let db = try Database(url: folder.appendingPathComponent("close.rdb"))
        let conn = try db.session()
        XCTAssertThrowsError(try db.close()) { error in
            XCTAssertEqual((error as? InillucentError)?.status, .invalidState)
        }
        XCTAssertEqual(try conn.query("SELECT 1").rows[0][0], 1)
        conn.close()
        try db.close()
        XCTAssertTrue(db.isClosed)
    }
}

/// A flag two threads share. A class with a lock, because a closure a `Thread` runs is `@Sendable`
/// and may not capture a `var`.
final class Flag: @unchecked Sendable {
    private let lock = NSLock()
    private var value = false

    /// Whether `set()` has run.
    var isSet: Bool {
        lock.lock()
        defer { lock.unlock() }
        return value
    }

    /// Sets the flag.
    func set() {
        lock.lock()
        value = true
        lock.unlock()
    }
}
