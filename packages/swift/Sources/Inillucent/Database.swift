import Foundation
import InillucentFFI

/// One open database file.
///
/// **Every call on a database, and on the connections, statements and transactions made from it,
/// takes this database's lock.** The engine is single threaded per file and the C library has no lock
/// of its own; it asks the binding to confine a database to one thread or to serialise every call.
/// iOS code calls from the main thread, from tasks and from dispatch queues, so this binding
/// serialises. `Connection.cancel()` is the one exception, because the C library allows it from
/// another thread and it would otherwise wait behind the statement it is meant to stop.
///
/// `close()` throws `.invalidState` while a connection made from this database is still alive, and
/// leaves the database usable. A database still open when it is released is closed then.
public final class Database: @unchecked Sendable {
    let lock = NSLock()
    private var handle: OpaquePointer?

    /// The file this database is in.
    public let path: String

    /// Opens a database, creating the file when it is absent.
    ///
    /// On iOS, put the file in the app's Application Support or Documents folder. The engine writes
    /// its log beside the file as `<name>-wal.<number>`, so give each database a folder or a name of
    /// its own.
    ///
    /// - Parameters:
    ///   - path: the file
    ///   - readOnly: refuse any statement that is not a query
    ///   - key: encrypt the file with this key: `x'<64 hex digits>'` is a raw key, anything else is a passphrase
    ///   - busyTimeoutMs: how long to wait for a lock another process holds; nil for the engine's 5000 ms
    ///   - diagnostics: fill `InillucentError.detail`, which may hold paths and bound values
    ///   - create: create the file when it is absent
    public init(path: String, readOnly: Bool = false, key: String? = nil, busyTimeoutMs: UInt32? = nil, diagnostics: Bool = false, create: Bool = true) throws {
        try refuseNul(path, "the path")
        if let key { try refuseNul(key, "the key") }
        if key != nil && busyTimeoutMs != nil {
            throw InillucentError(status: .invalidState, message: "the C library takes a key or a busy timeout when it opens, not both")
        }
        var flags: UInt32 = 0
        if create { flags |= UInt32(INILLUCENT_OPEN_CREATE) }
        if readOnly { flags |= UInt32(INILLUCENT_OPEN_READONLY) }
        if diagnostics { flags |= UInt32(INILLUCENT_OPEN_DIAGNOSTICS) }
        var opened: OpaquePointer?
        var error: OpaquePointer?
        let status: Int32
        if let key {
            status = inillucent_open_with_key(path, flags, key, &opened, &error)
        } else if let busyTimeoutMs {
            status = inillucent_open_with_timeout(path, flags, busyTimeoutMs, &opened, &error)
        } else {
            status = inillucent_open(path, flags, &opened, &error)
        }
        try check(status, error)
        handle = opened
        self.path = cText(inillucent_path(opened)) ?? path
    }

    /// Opens a database in a file URL.
    /// - Parameters:
    ///   - url: the file
    ///   - readOnly: refuse any statement that is not a query
    ///   - key: encrypt the file with this key
    public convenience init(url: URL, readOnly: Bool = false, key: String? = nil) throws {
        try self.init(path: url.path, readOnly: readOnly, key: key)
    }

    deinit {
        if let handle { _ = inillucent_close(handle, nil) }
    }

    /// Opens a connection: the scope `temp.` tables, `ATTACH` and connection pragmas belong to.
    public func session() throws -> Connection {
        try locked {
            var conn: OpaquePointer?
            var error: OpaquePointer?
            let db = try live()
            try check(inillucent_connect(db, &conn, &error), error)
            return Connection(database: self, handle: conn!)
        }
    }

    /// Makes everything written so far durable in the file.
    public func checkpoint() throws {
        try locked {
            var error: OpaquePointer?
            let db = try live()
            try check(inillucent_checkpoint(db, &error), error)
        }
    }

    /// Walks every tree and throws for the first thing that is wrong.
    public func integrityCheck() throws {
        try locked {
            var error: OpaquePointer?
            let db = try live()
            try check(inillucent_integrity_check(db, &error), error)
        }
    }

    /// Copies the database to a file, then opens and checks the copy before returning.
    /// - Parameter destination: the file to write
    public func backup(to destination: String) throws {
        try refuseNul(destination, "the backup path")
        try locked {
            var error: OpaquePointer?
            let db = try live()
            try check(inillucent_backup_to(db, destination, &error), error)
        }
    }

    /// Whether `close()` has run.
    public var isClosed: Bool { locked { handle == nil } }

    /// Folds the log into the file when this database wrote something, then closes. Closing a closed
    /// database does nothing.
    public func close() throws {
        try locked {
            guard let open = handle else { return }
            var error: OpaquePointer?
            try check(inillucent_close(open, &error), error)
            handle = nil
        }
    }

    /// The handle, or a throw when the database is closed.
    func live() throws -> OpaquePointer {
        guard let handle else { throw InillucentError(status: .invalidState, message: "this database is closed") }
        return handle
    }

    /// Runs work while holding the database's lock.
    /// - Parameter work: the work
    func locked<T>(_ work: () throws -> T) rethrows -> T {
        lock.lock()
        defer { lock.unlock() }
        return try work()
    }
}
