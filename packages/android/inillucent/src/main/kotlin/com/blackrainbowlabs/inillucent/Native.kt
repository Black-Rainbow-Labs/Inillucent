package com.blackrainbowlabs.inillucent

/**
 * The JNI functions in `src/main/cpp/inillucent_jni.c`, one for each C library call the Kotlin
 * classes make.
 *
 * Every string crosses as a `ByteArray` of UTF-8, never as a `String`, because JNI's own string
 * functions use "modified UTF-8" and would change an emoji or a NUL byte on the way through. The
 * file header in `inillucent_jni.c` has the details. A handle is a pointer in a `Long`.
 *
 * Nothing outside this package calls these: the public classes hold the lock and the handles.
 */
internal object Native {
    init {
        System.loadLibrary("inillucent_jni")
    }

    @JvmStatic external fun abiVersion(): Int
    @JvmStatic external fun version(): ByteArray
    @JvmStatic external fun capabilityCount(): Int
    @JvmStatic external fun capability(nth: Int): Array<Any?>
    @JvmStatic external fun supports(name: ByteArray): Int

    @JvmStatic external fun open(path: ByteArray, flags: Int, key: ByteArray?, timeoutMs: Int): Long
    @JvmStatic external fun close(db: Long)
    @JvmStatic external fun checkpoint(db: Long)
    @JvmStatic external fun integrityCheck(db: Long)
    @JvmStatic external fun backupTo(db: Long, path: ByteArray)
    @JvmStatic external fun path(db: Long): ByteArray?

    @JvmStatic external fun connect(db: Long): Long
    @JvmStatic external fun connFree(conn: Long)
    @JvmStatic external fun execute(conn: Long, sql: ByteArray, limit: Long): Array<Any?>
    @JvmStatic external fun executeBatch(conn: Long, sql: ByteArray)
    @JvmStatic external fun lastInsertRowid(conn: Long): Long
    @JvmStatic external fun totalChanges(conn: Long): Long
    @JvmStatic external fun inTransaction(conn: Long): Boolean
    @JvmStatic external fun schemaCookie(conn: Long): Long
    @JvmStatic external fun cancel(conn: Long)

    @JvmStatic external fun prepare(conn: Long, sql: ByteArray): Long
    @JvmStatic external fun stmtFree(stmt: Long)
    @JvmStatic external fun clearBindings(stmt: Long)
    @JvmStatic external fun bindNull(stmt: Long, index: Int)
    @JvmStatic external fun bindInt(stmt: Long, index: Int, value: Long)
    @JvmStatic external fun bindReal(stmt: Long, index: Int, value: Double)
    @JvmStatic external fun bindText(stmt: Long, index: Int, value: ByteArray)
    @JvmStatic external fun bindBlob(stmt: Long, index: Int, value: ByteArray)
    @JvmStatic external fun stmtExecute(stmt: Long, limit: Long): Array<Any?>

    @JvmStatic external fun txnBegin(conn: Long): Long
    @JvmStatic external fun txnExecute(txn: Long, sql: ByteArray): Long
    @JvmStatic external fun txnCommit(txn: Long)
    @JvmStatic external fun txnRollback(txn: Long)

    /**
     * Builds the exception for a failed call. The C glue calls this and throws what it returns, so
     * the exception's class and fields are decided here rather than in C.
     *
     * @param status - the C status code
     * @param message - the engine's message, as UTF-8
     * @param feature - the construct an `unsupported` refusal names, or null
     * @param detail - the diagnostic text, present only when the database was opened with diagnostics
     * @param offset - the byte offset into the statement, or -1
     * @param partial - the rows produced before the failure, in the layout [Rows.read] reads, or null
     */
    @JvmStatic
    fun failure(status: Int, message: ByteArray?, feature: ByteArray?, detail: ByteArray?, offset: Int, partial: Array<Any?>?): Throwable {
        val kind = Status.of(status)
        val said = message?.let(::utf8) ?: "the call failed with ${kind.wireName}"
        val rows = partial?.let(Rows::read)
        val named = feature?.let(::utf8)
        val diagnostic = detail?.let(::utf8)
        val at = if (offset >= 0) offset else null
        return if (kind == Status.UNSUPPORTED) {
            UnsupportedException(said, named, diagnostic, at, rows)
        } else {
            InillucentException(kind, said, named, diagnostic, at, rows)
        }
    }
}

/**
 * Decodes UTF-8 bytes from the C library into a `String`.
 * @param bytes - the bytes
 */
internal fun utf8(bytes: ByteArray): String = String(bytes, Charsets.UTF_8)

/**
 * Encodes a string the C library reads as a C string: SQL, a path, a key, a capability name.
 *
 * A NUL inside one would end it early in C and run a different statement from the one written, so
 * it is refused here, before anything runs.
 *
 * @param text - the string
 * @param what - what it is, for the message
 */
internal fun cString(text: String, what: String): ByteArray {
    require(text.indexOf('\u0000') < 0) { "$what holds a NUL character, which the C library cannot read" }
    return text.toByteArray(Charsets.UTF_8)
}
