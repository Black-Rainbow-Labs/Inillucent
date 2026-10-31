package com.blackrainbowlabs.inillucent

import java.io.Closeable
import java.io.File

/**
 * One open database file.
 *
 * **Every call on a database, and on the connections, statements and transactions made from it, takes
 * this database's lock.** The engine is single threaded per file and the C library has no lock of its
 * own; it asks the binding to confine a database to one thread or to serialise every call. Android
 * code calls from the main thread, from coroutines and from worker threads, so this binding
 * serialises. [Connection.cancel] is the one exception, because the C library allows it from another
 * thread and it would otherwise wait behind the statement it is meant to stop.
 *
 * [close] refuses with [Status.INVALID_STATE] while a connection made from this database is still
 * open, and leaves the database usable, so close connections first or use [use] blocks.
 */
class Database private constructor(private var handle: Long) : Closeable {
    internal val lock = Any()

    /** The file this database is in. */
    val path: String = Native.path(handle)?.let(::utf8) ?: ""

    /**
     * Opens a connection. A connection is the scope `temp.` tables, `ATTACH` and connection pragmas
     * belong to.
     */
    fun session(): Connection = synchronized(lock) { Connection(this, Native.connect(live())) }

    /** Makes everything written so far durable in the file. */
    fun checkpoint() = synchronized(lock) { Native.checkpoint(live()) }

    /** Walks every tree and throws [InillucentException] for the first thing that is wrong. */
    fun integrityCheck() = synchronized(lock) { Native.integrityCheck(live()) }

    /**
     * Copies the database to a file, then opens and checks the copy before returning.
     * @param destination - the file to write
     */
    fun backupTo(destination: File) = synchronized(lock) { Native.backupTo(live(), cString(destination.path, "the backup path")) }

    /** Whether [close] has run. */
    val isClosed: Boolean get() = synchronized(lock) { handle == 0L }

    /**
     * Folds the log into the file when this database wrote something, then closes. Closing a closed
     * database does nothing.
     */
    override fun close() {
        synchronized(lock) {
            if (handle == 0L) return
            Native.close(handle)
            handle = 0L
        }
    }

    /** The handle, or a throw when the database is closed. */
    internal fun live(): Long {
        if (handle == 0L) throw InillucentException(Status.INVALID_STATE, "this database is closed")
        return handle
    }

    companion object {
        private const val CREATE = 0x0001
        private const val READONLY = 0x0002
        private const val DIAGNOSTICS = 0x0004

        /**
         * Opens a database, creating the file when it is absent.
         *
         * On Android, pass a file under `context.filesDir` or `context.noBackupFilesDir`. The engine
         * writes its log beside the file as `<name>-wal.<number>`, so give each database a folder or
         * a name of its own.
         *
         * @param path - the file
         * @param readOnly - refuse any statement that is not a query
         * @param key - encrypt the file with this key: `x'<64 hex digits>'` is a raw key, anything else is a passphrase
         * @param busyTimeoutMs - how long to wait for a lock another process holds; null for the engine's 5000 ms
         * @param diagnostics - fill [InillucentException.detail], which may hold paths and bound values
         * @param create - create the file when it is absent
         */
        @JvmStatic
        @JvmOverloads
        fun open(path: String, readOnly: Boolean = false, key: String? = null, busyTimeoutMs: Int? = null, diagnostics: Boolean = false, create: Boolean = true): Database {
            require(busyTimeoutMs == null || busyTimeoutMs >= 0) { "busyTimeoutMs must not be negative" }
            require(key == null || busyTimeoutMs == null) { "the C library takes a key or a busy timeout when it opens, not both" }
            var flags = 0
            if (create) flags = flags or CREATE
            if (readOnly) flags = flags or READONLY
            if (diagnostics) flags = flags or DIAGNOSTICS
            val handle = Native.open(cString(path, "the path"), flags, key?.let { cString(it, "the key") }, busyTimeoutMs ?: -1)
            return Database(handle)
        }

        /**
         * Opens a database in a file.
         * @param file - the file
         * @param readOnly - refuse any statement that is not a query
         * @param key - encrypt the file with this key
         */
        @JvmStatic
        @JvmOverloads
        fun open(file: File, readOnly: Boolean = false, key: String? = null): Database = open(file.path, readOnly, key)
    }
}
