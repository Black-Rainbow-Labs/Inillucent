package com.blackrainbowlabs.inillucent

import java.io.Closeable

/**
 * A prepared statement, for running one statement many times with new values. Every call takes the
 * database's lock. Close it when done; it keeps its connection's session alive until then.
 */
class Statement internal constructor(private val database: Database, private var handle: Long) : Closeable {
    /**
     * Runs the statement with these values bound to ?1, ?2, ...
     * @param params - the values
     */
    fun execute(vararg params: Any?): Rows = query(params.asList())

    /**
     * Runs the statement with these values and returns at most [limit] rows.
     * @param params - the values
     * @param limit - rows to hand back; null for all
     */
    @JvmOverloads
    fun query(params: List<Any?> = emptyList(), limit: Long? = null): Rows {
        require(limit == null || limit >= 0) { "limit must not be negative" }
        return synchronized(database.lock) {
            val stmt = live()
            bindAll(stmt, params)
            Rows.read(Native.stmtExecute(stmt, limit ?: -1))
        }
    }

    /** Frees the statement. Closing a closed statement does nothing. */
    override fun close() {
        synchronized(database.lock) {
            if (handle == 0L) return
            Native.stmtFree(handle)
            handle = 0L
        }
    }

    /** The handle, or a throw when the statement is closed. */
    private fun live(): Long {
        if (handle == 0L) throw InillucentException(Status.INVALID_STATE, "this statement is closed")
        return handle
    }
}

/**
 * An open transaction. The point of holding one is to read what each statement changed, through
 * [affected], and decide before [commit] rather than after it.
 *
 * A statement that fails rolls the whole transaction back before the exception reaches you, and the
 * transaction is then finished. Closing it without [commit] rolls it back.
 */
class Transaction internal constructor(private val database: Database, private var handle: Long) : Closeable {
    private val changed = ArrayList<Long>()

    /** The rows each statement run through this transaction changed, in order. */
    val affected: List<Long> get() = synchronized(database.lock) { changed.toList() }

    /**
     * Runs one statement inside the transaction and returns the rows it changed.
     * @param sql - the statement
     */
    fun execute(sql: String): Long {
        val text = cString(sql, "the statement")
        return synchronized(database.lock) {
            val txn = live()
            try {
                Native.txnExecute(txn, text).also { changed.add(it) }
            } catch (failure: InillucentException) {
                // The C library has rolled back and the handle is spent; free it now so a
                // later close does not touch it.
                Native.txnRollback(txn)
                handle = 0L
                throw failure
            }
        }
    }

    /** Commits. The transaction is finished whether or not the commit succeeds. */
    fun commit() = synchronized(database.lock) {
        val txn = live()
        handle = 0L
        try {
            Native.txnCommit(txn)
        } finally {
            // A committed handle is still a handle: the C library frees it here.
            Native.txnRollback(txn)
        }
    }

    /** Rolls back. Rolling back a finished transaction does nothing. */
    fun rollback() {
        synchronized(database.lock) {
            if (handle == 0L) return
            Native.txnRollback(handle)
            handle = 0L
        }
    }

    /** Rolls back unless [commit] ran. */
    override fun close() = rollback()

    /** The handle, or a throw when the transaction is finished. */
    private fun live(): Long {
        if (handle == 0L) throw InillucentException(Status.INVALID_STATE, "this transaction is finished")
        return handle
    }
}
