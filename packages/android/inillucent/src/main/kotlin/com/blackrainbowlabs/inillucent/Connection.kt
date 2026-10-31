package com.blackrainbowlabs.inillucent

import java.io.Closeable

/**
 * A connection to a [Database]: the session `temp.` tables, `ATTACH` and connection pragmas belong
 * to. Every call takes the database's lock, except [cancel].
 *
 * A statement or transaction made from a connection keeps working after the connection is closed,
 * because the C library keeps the session alive until the last of them is freed.
 */
class Connection internal constructor(private val database: Database, private var handle: Long) : Closeable {
    // Read without the lock by cancel, so it is volatile.
    @Volatile private var cancelHandle: Long = handle

    /**
     * Runs one statement and returns everything it produced.
     * @param sql - one statement; [executeBatch] runs several
     * @param params - values bound to ?1, ?2, ...: null, Long, Int, Short, Byte, Double, Float, Boolean, String or ByteArray
     */
    fun execute(sql: String, vararg params: Any?): Rows = query(sql, params.asList())

    /**
     * Runs one statement and returns at most [limit] of its rows. [Rows.total] is exact whatever the
     * limit, so a list can say "1 to 200 of 4,317".
     * @param sql - one statement
     * @param params - values bound to ?1, ?2, ...
     * @param limit - rows to hand back; null for all
     */
    @JvmOverloads
    fun query(sql: String, params: List<Any?> = emptyList(), limit: Long? = null): Rows {
        require(limit == null || limit >= 0) { "limit must not be negative" }
        val text = cString(sql, "the statement")
        return synchronized(database.lock) {
            if (params.isEmpty()) {
                Rows.read(Native.execute(live(), text, limit ?: -1))
            } else {
                val stmt = Native.prepare(live(), text)
                try {
                    bindAll(stmt, params)
                    Rows.read(Native.stmtExecute(stmt, limit ?: -1))
                } finally {
                    Native.stmtFree(stmt)
                }
            }
        }
    }

    /**
     * Runs several statements separated by semicolons, for their effect.
     * @param sql - the statements
     */
    fun executeBatch(sql: String) {
        val text = cString(sql, "the statements")
        synchronized(database.lock) { Native.executeBatch(live(), text) }
    }

    /**
     * Runs one statement once for each row of values and returns the rows changed in all. Opens no
     * transaction: outside one, each execution commits by itself, and the first failure stops the run
     * with the executions before it kept. Wrap it in [transaction] to keep all or nothing.
     * @param sql - one statement
     * @param rows - one list of values for each execution
     */
    fun executeMany(sql: String, rows: Iterable<List<Any?>>): Long = prepare(sql).use { statement ->
        var changed = 0L
        for (row in rows) changed += statement.query(row).affected ?: 0L
        changed
    }

    /**
     * Prepares a statement to run more than once.
     * @param sql - one statement
     */
    fun prepare(sql: String): Statement {
        val text = cString(sql, "the statement")
        return synchronized(database.lock) { Statement(database, Native.prepare(live(), text)) }
    }

    /**
     * Begins a transaction you commit or roll back yourself. Closing it without [Transaction.commit]
     * rolls it back.
     */
    fun begin(): Transaction = synchronized(database.lock) { Transaction(database, Native.txnBegin(live())) }

    /**
     * Runs [work] in a transaction: commits when it returns and rolls back when it throws.
     * @param work - the statements, run through the transaction it is given
     */
    fun <T> transaction(work: (Transaction) -> T): T = begin().use { txn ->
        val answer = work(txn)
        txn.commit()
        answer
    }

    /** The rowid of the last row inserted on this connection. */
    val lastInsertRowid: Long get() = synchronized(database.lock) { Native.lastInsertRowid(live()) }

    /** Rows changed since this connection opened. */
    val totalChanges: Long get() = synchronized(database.lock) { Native.totalChanges(live()) }

    /** Whether a transaction is open on this connection. */
    val inTransaction: Boolean get() = synchronized(database.lock) { Native.inTransaction(live()) }

    /** The schema's generation, which changes when the schema does. */
    val schemaCookie: Long get() = synchronized(database.lock) { Native.schemaCookie(live()) }

    /**
     * Asks the statement running on this connection to stop. Call it from another thread: the running
     * statement holds the lock, and this does not take it. The statement then fails with
     * [Status.INTERRUPTED] and the connection stays usable. It stops at the next leaf of a scan or the
     * next batch of results, so it is prompt and not instant. With nothing running it does nothing.
     */
    fun cancel() {
        val target = cancelHandle
        if (target != 0L) Native.cancel(target)
    }

    /** Frees the connection. Closing a closed connection does nothing. */
    override fun close() {
        synchronized(database.lock) {
            if (handle == 0L) return
            cancelHandle = 0L
            Native.connFree(handle)
            handle = 0L
        }
    }

    /** The handle, or a throw when the connection or its database is closed. */
    private fun live(): Long {
        database.live()
        if (handle == 0L) throw InillucentException(Status.INVALID_STATE, "this connection is closed")
        return handle
    }
}

/**
 * Replaces what a statement has bound with [params], bound to ?1, ?2, ... in order.
 * @param stmt - the statement's handle
 * @param params - the values
 */
internal fun bindAll(stmt: Long, params: List<Any?>) {
    Native.clearBindings(stmt)
    params.forEachIndexed { at, value -> bindOne(stmt, at + 1, value) }
}

/**
 * Binds one value. Integers of every width bind as an integer and a Boolean binds as 1 or 0, the way
 * SQLite's drivers bind them.
 * @param stmt - the statement's handle
 * @param index - one based
 * @param value - the value
 */
private fun bindOne(stmt: Long, index: Int, value: Any?) {
    when (value) {
        null -> Native.bindNull(stmt, index)
        is Long -> Native.bindInt(stmt, index, value)
        is Int -> Native.bindInt(stmt, index, value.toLong())
        is Short -> Native.bindInt(stmt, index, value.toLong())
        is Byte -> Native.bindInt(stmt, index, value.toLong())
        is Boolean -> Native.bindInt(stmt, index, if (value) 1L else 0L)
        is Double -> Native.bindReal(stmt, index, value)
        is Float -> Native.bindReal(stmt, index, value.toDouble())
        is String -> Native.bindText(stmt, index, value.toByteArray(Charsets.UTF_8))
        is ByteArray -> Native.bindBlob(stmt, index, value)
        else -> throw IllegalArgumentException("parameter $index is a ${value::class.java.name}, which has no engine value; pass null, a number, a Boolean, a String or a ByteArray")
    }
}
