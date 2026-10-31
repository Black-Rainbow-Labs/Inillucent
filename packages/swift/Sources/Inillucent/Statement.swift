import Foundation
import InillucentFFI

/// A prepared statement, for running one statement many times with new values. Every call takes the
/// database's lock. It keeps its connection's session alive until it is released or closed.
public final class Statement: @unchecked Sendable {
    private let database: Database
    private var handle: OpaquePointer?

    /// Wraps a statement handle.
    /// - Parameters:
    ///   - database: the database it belongs to
    ///   - handle: the handle
    init(database: Database, handle: OpaquePointer) {
        self.database = database
        self.handle = handle
    }

    deinit {
        inillucent_stmt_free(handle)
    }

    /// Runs the statement with these values bound to ?1, ?2, ... and returns at most `limit` rows.
    /// - Parameters:
    ///   - params: the values
    ///   - limit: rows to hand back; nil for all
    @discardableResult
    public func query(_ params: [Value] = [], limit: UInt64? = nil) throws -> Rows {
        try database.locked {
            guard let handle else { throw InillucentError(status: .invalidState, message: "this statement is closed") }
            return try runPrepared(handle, params, limit ?? UInt64.max)
        }
    }

    /// Frees the statement. Closing a closed statement does nothing.
    public func close() {
        database.locked {
            inillucent_stmt_free(handle)
            handle = nil
        }
    }
}

/// An open transaction. The point of holding one is to read what each statement changed, through
/// `affected`, and decide before `commit()` rather than after it.
///
/// A statement that fails rolls the whole transaction back before the error reaches you, and the
/// transaction is then finished. Releasing it without `commit()` rolls it back.
public final class Transaction: @unchecked Sendable {
    private let database: Database
    private var handle: OpaquePointer?
    private var changed: [UInt64] = []

    /// Wraps a transaction handle.
    /// - Parameters:
    ///   - database: the database it belongs to
    ///   - handle: the handle
    init(database: Database, handle: OpaquePointer) {
        self.database = database
        self.handle = handle
    }

    deinit {
        // Rolls back and frees, and is a no-op on nil.
        inillucent_txn_rollback(handle)
    }

    /// The rows each statement run through this transaction changed, in order.
    public var affected: [UInt64] { database.locked { changed } }

    /// Runs one statement inside the transaction and returns the rows it changed.
    /// - Parameter sql: the statement
    @discardableResult
    public func execute(_ sql: String) throws -> UInt64 {
        try refuseNul(sql, "the statement")
        return try database.locked {
            let txn = try live()
            var count: UInt64 = 0
            var error: OpaquePointer?
            let status = inillucent_txn_execute(txn, sql, &count, &error)
            if status != INILLUCENT_OK {
                // The library has rolled back and the handle is spent.
                inillucent_txn_rollback(txn)
                handle = nil
                throw InillucentError(status: status, error: error)
            }
            changed.append(count)
            return count
        }
    }

    /// Commits. The transaction is finished whether or not the commit succeeds.
    public func commit() throws {
        try database.locked {
            let txn = try live()
            handle = nil
            var error: OpaquePointer?
            let status = inillucent_txn_commit(txn, &error)
            // A committed handle is still a handle: the library frees it here.
            inillucent_txn_rollback(txn)
            try check(status, error)
        }
    }

    /// Rolls back. Rolling back a finished transaction does nothing.
    public func rollback() {
        database.locked {
            inillucent_txn_rollback(handle)
            handle = nil
        }
    }

    /// The handle, or a throw when the transaction is finished.
    private func live() throws -> OpaquePointer {
        guard let handle else { throw InillucentError(status: .invalidState, message: "this transaction is finished") }
        return handle
    }
}
