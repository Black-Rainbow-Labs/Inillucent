import Foundation
import InillucentFFI

/// A connection to a `Database`: the session `temp.` tables, `ATTACH` and connection pragmas belong
/// to. Every call takes the database's lock, except `cancel()`.
///
/// A statement or transaction made from a connection keeps working after the connection is closed,
/// because the C library keeps the session alive until the last of them is freed.
public final class Connection: @unchecked Sendable {
    private let database: Database
    private var handle: OpaquePointer?
    // Read without the lock by cancel. A connection is closed under the lock, and cancel on a freed
    // handle is the one race left; it is closed only by close() and deinit, after which nothing
    // should call cancel.
    private let cancelLock = NSLock()

    /// Wraps a connection handle.
    /// - Parameters:
    ///   - database: the database it was opened on
    ///   - handle: the handle
    init(database: Database, handle: OpaquePointer) {
        self.database = database
        self.handle = handle
    }

    deinit {
        inillucent_conn_free(handle)
    }

    /// Runs one statement and returns at most `limit` of its rows. `Rows.total` is exact whatever the
    /// limit.
    /// - Parameters:
    ///   - sql: one statement; `executeBatch` runs several
    ///   - params: values bound to ?1, ?2, ...
    ///   - limit: rows to hand back; nil for all
    @discardableResult
    public func query(_ sql: String, _ params: [Value] = [], limit: UInt64? = nil) throws -> Rows {
        try refuseNul(sql, "the statement")
        return try database.locked {
            let conn = try live()
            let cap = limit ?? UInt64.max
            if params.isEmpty {
                var rows: OpaquePointer?
                var error: OpaquePointer?
                try check(inillucent_execute(conn, sql, cap, &rows, &error), error)
                return Rows(taking: rows!)
            }
            var stmt: OpaquePointer?
            var error: OpaquePointer?
            try check(inillucent_prepare(conn, sql, &stmt, &error), error)
            defer { inillucent_stmt_free(stmt) }
            return try runPrepared(stmt!, params, cap)
        }
    }

    /// Runs one statement and returns everything it produced.
    /// - Parameters:
    ///   - sql: one statement
    ///   - params: values bound to ?1, ?2, ...
    @discardableResult
    public func execute(_ sql: String, _ params: [Value] = []) throws -> Rows {
        try query(sql, params)
    }

    /// Runs several statements separated by semicolons, for their effect.
    /// - Parameter sql: the statements
    public func executeBatch(_ sql: String) throws {
        try refuseNul(sql, "the statements")
        try database.locked {
            var error: OpaquePointer?
            let conn = try live()
            try check(inillucent_execute_batch(conn, sql, &error), error)
        }
    }

    /// Runs one statement once for each row of values and returns the rows changed in all. Opens no
    /// transaction: outside one, each execution commits by itself.
    /// - Parameters:
    ///   - sql: one statement
    ///   - rows: one list of values for each execution
    @discardableResult
    public func executeMany(_ sql: String, _ rows: [[Value]]) throws -> Int64 {
        let statement = try prepare(sql)
        var changed: Int64 = 0
        for row in rows { changed += try statement.query(row).affected ?? 0 }
        return changed
    }

    /// Prepares a statement to run more than once.
    /// - Parameter sql: one statement
    public func prepare(_ sql: String) throws -> Statement {
        try refuseNul(sql, "the statement")
        return try database.locked {
            var stmt: OpaquePointer?
            var error: OpaquePointer?
            let conn = try live()
            try check(inillucent_prepare(conn, sql, &stmt, &error), error)
            return Statement(database: database, handle: stmt!)
        }
    }

    /// Begins a transaction you commit or roll back yourself. Releasing it without `commit()` rolls
    /// it back.
    public func begin() throws -> Transaction {
        try database.locked {
            var txn: OpaquePointer?
            var error: OpaquePointer?
            let conn = try live()
            try check(inillucent_txn_begin(conn, &txn, &error), error)
            return Transaction(database: database, handle: txn!)
        }
    }

    /// Runs `work` in a transaction: commits when it returns and rolls back when it throws.
    /// - Parameter work: the statements, run through the transaction it is given
    public func transaction<T>(_ work: (Transaction) throws -> T) throws -> T {
        let txn = try begin()
        do {
            let answer = try work(txn)
            try txn.commit()
            return answer
        } catch {
            txn.rollback()
            throw error
        }
    }

    /// The rowid of the last row inserted on this connection.
    public var lastInsertRowid: Int64 { get throws { try database.locked { let conn = try live(); return inillucent_last_insert_rowid(conn) } } }

    /// Rows changed since this connection opened.
    public var totalChanges: Int64 { get throws { try database.locked { let conn = try live(); return inillucent_total_changes(conn) } } }

    /// Whether a transaction is open on this connection.
    public var inTransaction: Bool { get throws { try database.locked { let conn = try live(); return inillucent_in_transaction(conn) != 0 } } }

    /// The schema's generation, which changes when the schema does.
    public var schemaCookie: UInt64 { get throws { try database.locked { let conn = try live(); return inillucent_schema_cookie(conn) } } }

    /// Asks the statement running on this connection to stop. Call it from another thread: the
    /// running statement holds the database's lock, and this does not take it. The statement then
    /// fails with `.interrupted` and the connection stays usable. With nothing running it does nothing.
    public func cancel() throws {
        cancelLock.lock()
        defer { cancelLock.unlock() }
        guard let handle else { return }
        var error: OpaquePointer?
        try check(inillucent_cancel(handle, &error), error)
    }

    /// Frees the connection. Closing a closed connection does nothing.
    public func close() {
        database.locked {
            cancelLock.lock()
            defer { cancelLock.unlock() }
            inillucent_conn_free(handle)
            handle = nil
        }
    }

    /// The handle, or a throw when the connection or its database is closed.
    private func live() throws -> OpaquePointer {
        _ = try database.live()
        guard let handle else { throw InillucentError(status: .invalidState, message: "this connection is closed") }
        return handle
    }
}

/// Binds `params` to ?1, ?2, ... in place of what was bound, runs the statement, and reads its rows.
/// The caller holds the database's lock.
/// - Parameters:
///   - stmt: the statement's handle
///   - params: the values
///   - limit: rows to hand back
func runPrepared(_ stmt: OpaquePointer, _ params: [Value], _ limit: UInt64) throws -> Rows {
    inillucent_clear_bindings(stmt)
    for (at, value) in params.enumerated() {
        try bind(stmt, UInt32(at + 1), value)
    }
    var rows: OpaquePointer?
    var error: OpaquePointer?
    try check(inillucent_stmt_execute(stmt, limit, &rows, &error), error)
    return Rows(taking: rows!)
}

/// One byte for an empty text or blob to point at, because a zero length value still needs a
/// pointer that is not null.
private let emptyByte: [UInt8] = [0]

/// Binds one value.
/// - Parameters:
///   - stmt: the statement's handle
///   - index: one based
///   - value: the value
private func bind(_ stmt: OpaquePointer, _ index: UInt32, _ value: Value) throws {
    let status: Int32
    switch value {
    case .null:
        status = inillucent_bind_null(stmt, index)
    case .integer(let number):
        status = inillucent_bind_int(stmt, index, number)
    case .real(let number):
        status = inillucent_bind_real(stmt, index, number)
    case .text(let text):
        let bytes = Array(text.utf8)
        status = (bytes.isEmpty ? emptyByte : bytes).withUnsafeBufferPointer { buffer in
            buffer.baseAddress!.withMemoryRebound(to: CChar.self, capacity: buffer.count) {
                inillucent_bind_text(stmt, index, $0, bytes.count)
            }
        }
    case .blob(let data):
        let bytes = [UInt8](data)
        status = (bytes.isEmpty ? emptyByte : bytes).withUnsafeBufferPointer { buffer in
            inillucent_bind_blob(stmt, index, buffer.baseAddress!, bytes.count)
        }
    }
    if status != INILLUCENT_OK {
        throw InillucentError(status: Status(code: status), message: "parameter \(index) could not be bound")
    }
}
