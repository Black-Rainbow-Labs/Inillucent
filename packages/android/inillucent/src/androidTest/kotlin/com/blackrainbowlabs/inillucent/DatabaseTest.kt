package com.blackrainbowlabs.inillucent

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.After
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean

/**
 * What the conformance suite cannot ask, because it opens every case's database the same way and
 * runs on one thread: a key, a reopened file, a transaction rolled back by a throw, text JNI's own
 * string functions would change, a cancel from a second thread, and the rows a failing statement
 * produced before it failed.
 *
 * Every test runs on the device, through the JNI glue, against a file in the app's own storage.
 */
@RunWith(AndroidJUnit4::class)
class DatabaseTest {
    private lateinit var folder: File

    /** A folder of the test's own, so the engine's log segments cannot collide with another test's. */
    @Before
    fun makeFolder() {
        folder = File(InstrumentationRegistry.getInstrumentation().targetContext.filesDir, "database-test-${System.nanoTime()}")
        assertTrue(folder.mkdirs())
    }

    /** Removes the folder and everything the engine wrote in it. */
    @After
    fun removeFolder() {
        folder.deleteRecursively()
    }

    /** The library loads and names an ABI this binding was written for. */
    @Test
    fun theLibraryLoadsAndReportsItsVersion() {
        assertTrue(Inillucent.version(), Inillucent.version().startsWith("inillucent"))
        assertEquals(Inillucent.ABI_MAJOR, Inillucent.abiVersion().substringBefore('.').toInt())
    }

    /** Rows written and closed are there when the file is opened again. */
    @Test
    fun aClosedDatabaseOpensAgainWithItsRows() {
        val file = File(folder, "reopen.rdb")
        Database.open(file).use { db ->
            db.session().use { conn ->
                conn.execute("CREATE TABLE people (id INTEGER PRIMARY KEY, name TEXT)")
                conn.executeMany("INSERT INTO people (name) VALUES (?1)", listOf(listOf("Ada"), listOf("Grace"), listOf("Hedy")))
                assertEquals(3L, conn.lastInsertRowid)
            }
            db.checkpoint()
            db.integrityCheck()
        }
        Database.open(file, readOnly = true).use { db ->
            db.session().use { conn ->
                val rows = conn.query("SELECT name FROM people ORDER BY id")
                assertEquals(listOf(listOf("Ada"), listOf("Grace"), listOf("Hedy")), rows.rows)
                try {
                    conn.execute("DELETE FROM people")
                    fail("a read only database accepted a write")
                } catch (expected: InillucentException) {
                    assertEquals(Status.READONLY, expected.status)
                }
            }
        }
    }

    /** An encrypted file holds no plaintext, and opens only with its own key. */
    @Test
    fun anEncryptedDatabaseOpensOnlyWithItsKey() {
        val file = File(folder, "vault.rdb")
        val key = "x'" + "44".repeat(32) + "'"
        val secret = "the vault code is 7461"
        Database.open(file, key = key).use { db ->
            db.session().use { conn ->
                conn.execute("CREATE TABLE vault (code TEXT)")
                conn.execute("INSERT INTO vault VALUES (?1)", secret)
            }
        }
        val needle = secret.toByteArray(Charsets.UTF_8)
        for (written in folder.listFiles()!!) {
            assertFalse("${written.name} holds the row in plaintext", contains(written.readBytes(), needle))
        }
        for (wrong in listOf(null, "x'" + "45".repeat(32) + "'")) {
            try {
                Database.open(file, key = wrong).close()
                fail("the database opened with the key $wrong")
            } catch (expected: InillucentException) {
                assertEquals(Status.CORRUPT, expected.status)
            }
        }
        Database.open(file, key = key).use { db ->
            db.session().use { conn -> assertEquals(secret, conn.query("SELECT code FROM vault").rows[0][0]) }
        }
    }

    /** A throw inside [Connection.transaction] keeps none of its writes; a return keeps all of them. */
    @Test
    fun aTransactionRollsBackWhenItsWorkThrows() {
        Database.open(File(folder, "txn.rdb")).use { db ->
            db.session().use { conn ->
                conn.execute("CREATE TABLE t (n INTEGER PRIMARY KEY)")
                try {
                    conn.transaction { txn ->
                        assertEquals(1L, txn.execute("INSERT INTO t VALUES (1)"))
                        assertTrue(conn.inTransaction)
                        throw IllegalStateException("the application changed its mind")
                    }
                } catch (expected: IllegalStateException) {
                }
                assertFalse(conn.inTransaction)
                assertEquals(0L, conn.query("SELECT count(*) FROM t").rows[0][0])
                val changed = conn.transaction { txn ->
                    txn.execute("INSERT INTO t VALUES (1)")
                    txn.execute("INSERT INTO t VALUES (2)")
                    txn.affected
                }
                assertEquals(listOf(1L, 1L), changed)
                assertEquals(2L, conn.query("SELECT count(*) FROM t").rows[0][0])
            }
        }
    }

    /**
     * Text with a character outside the Basic Multilingual Plane and a NUL byte, and a blob, come back
     * as they went in. JNI's own string functions would change the first two, which is why every
     * string crosses as UTF-8 bytes.
     */
    @Test
    fun textJniWouldChangeComesBackUnchanged() {
        Database.open(File(folder, "text.rdb")).use { db ->
            db.session().use { conn ->
                conn.execute("CREATE TABLE \"täble 😀\" (v)")
                val text = "été 😀 a\u0000b"
                val blob = byteArrayOf(0, -1, 1, 0)
                conn.execute("INSERT INTO \"täble 😀\" VALUES (?1), (?2), (?3), (?4), (?5)", text, blob, Long.MIN_VALUE, -0.0, null)
                val rows = conn.query("SELECT v, length(v), typeof(v) FROM \"täble 😀\" ORDER BY rowid")
                assertEquals(listOf("v", "length(v)", "typeof(v)"), rows.columns)
                assertEquals(text, rows.rows[0][0])
                assertEquals(text.substringBefore('\u0000').codePointCount(0, text.substringBefore('\u0000').length).toLong(), rows.rows[0][1])
                assertArrayEquals(blob, rows.rows[1][0] as ByteArray)
                assertEquals(Long.MIN_VALUE, rows.rows[2][0])
                assertEquals("real", rows.rows[3][2])
                assertNull(rows.rows[4][0])
                assertEquals(5L, rows.total)
            }
        }
    }

    /** The capability table reads, `cancel` is partial, and a name nobody declared is unknown. */
    @Test
    fun theCapabilityTableReads() {
        val table = Inillucent.capabilities()
        assertTrue("${table.size} capabilities", table.size >= 40)
        assertTrue(table.all { it.name.isNotEmpty() && it.note.isNotEmpty() })
        assertEquals(Support.PARTIAL, Inillucent.supports("cancel"))
        assertEquals(Support.UNKNOWN, Inillucent.supports("time_travel"))
    }

    /** A cancel from a second thread stops a long statement, and the connection still works after. */
    @Test
    fun aCancelFromAnotherThreadStopsALongStatement() {
        Database.open(File(folder, "cancel.rdb")).use { db ->
            db.session().use { conn ->
                val finished = AtomicBoolean(false)
                val canceller = Executors.newSingleThreadExecutor()
                // A cancel before the statement starts is cleared when it starts, so keep asking
                // until it has stopped.
                canceller.submit {
                    while (!finished.get()) {
                        Thread.sleep(100)
                        conn.cancel()
                    }
                }
                val started = System.nanoTime()
                try {
                    conn.query("WITH RECURSIVE c(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM c WHERE n < 100000000) SELECT count(*) FROM c")
                    fail("the statement counted to a hundred million instead of stopping")
                } catch (expected: InillucentException) {
                    assertEquals(Status.INTERRUPTED, expected.status)
                } finally {
                    finished.set(true)
                    canceller.shutdown()
                    canceller.awaitTermination(10, TimeUnit.SECONDS)
                }
                assertTrue("the cancel took too long", System.nanoTime() - started < TimeUnit.SECONDS.toNanos(60))
                assertEquals(1L, conn.query("SELECT 1").rows[0][0])
            }
        }
    }

    /** A statement that fails part way hands back the rows before the failure, with the failure. */
    @Test
    fun aFailingStatementCarriesTheRowsBeforeIt() {
        Database.open(File(folder, "partial.rdb")).use { db ->
            db.session().use { conn ->
                conn.executeBatch("CREATE TABLE t(a); WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM c WHERE x<5000) INSERT INTO t SELECT x FROM c")
                try {
                    conn.query("SELECT a, CASE WHEN a = 3000 THEN json('x') END FROM t", limit = 10)
                    fail("json('x') is not JSON and the statement succeeded")
                } catch (failure: InillucentException) {
                    val partial = failure.partialRows ?: error("the failure carried no rows")
                    assertEquals(10, partial.size)
                    assertEquals(2999L, partial.total)
                    assertTrue(partial.more)
                    assertEquals(1L, partial.rows[0][0])
                }
            }
        }
    }

    /** An unbuilt construct arrives as [UnsupportedException] naming it, and a typo does not. */
    @Test
    fun unsupportedIsItsOwnException() {
        Database.open(File(folder, "status.rdb")).use { db ->
            db.session().use { conn ->
                try {
                    conn.query("EXPLAIN EXPLAIN SELECT 1")
                    fail("EXPLAIN EXPLAIN ran")
                } catch (expected: UnsupportedException) {
                    assertTrue(!expected.feature.isNullOrEmpty())
                }
                try {
                    conn.query("SELEC 1")
                    fail("a typo ran")
                } catch (expected: InillucentException) {
                    assertFalse(expected is UnsupportedException)
                    assertEquals(Status.SYNTAX, expected.status)
                }
                try {
                    conn.query("SELECT ?1", listOf(Any()))
                    fail("an object with no engine value was bound")
                } catch (expected: IllegalArgumentException) {
                }
            }
        }
    }

    /** A database refuses to close while a connection is open, and stays usable. */
    @Test
    fun closeRefusesWhileAConnectionIsOpen() {
        val db = Database.open(File(folder, "close.rdb"))
        val conn = db.session()
        try {
            db.close()
            fail("the database closed under an open connection")
        } catch (expected: InillucentException) {
            assertEquals(Status.INVALID_STATE, expected.status)
        }
        assertEquals(1L, conn.query("SELECT 1").rows[0][0])
        conn.close()
        db.close()
        assertTrue(db.isClosed)
    }

    /**
     * Whether a byte array holds another.
     * @param haystack - the bytes to search
     * @param needle - the bytes to find
     */
    private fun contains(haystack: ByteArray, needle: ByteArray): Boolean {
        outer@ for (at in 0..haystack.size - needle.size) {
            for (k in needle.indices) if (haystack[at + k] != needle[k]) continue@outer
            return true
        }
        return false
    }
}
