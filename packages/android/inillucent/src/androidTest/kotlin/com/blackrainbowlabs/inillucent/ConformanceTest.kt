package com.blackrainbowlabs.inillucent

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File

/**
 * Runs `drivers/conformance/suite.json` through the Kotlin binding and the JNI glue, on a device.
 *
 * It asserts what `drivers/bindings/python/run_conformance.py` and the Rust driver's
 * `tests/conformance.rs` assert, from the same file, which Gradle packs into the test APK. When they
 * agree, the C ABI is followable from Kotlin; when they disagree, one binding is wrong.
 *
 * This binding holds a connection, so it lacks no capability and must run every case. The test fails
 * when it ran fewer cases than the suite holds, so a suite that grew without this runner noticing is a
 * failure and not a pass. It also writes the record the other runners write, `android.json`, which
 * `packages/android/run-tests.sh` pulls off the device into `_agent_output/conformance/`.
 */
@RunWith(AndroidJUnit4::class)
class ConformanceTest {
    private val instrumentation = InstrumentationRegistry.getInstrumentation()

    /** Every case of the suite, then the record. */
    @Test
    fun theWholeSuitePasses() {
        val suite = JSONObject(instrumentation.context.assets.open("suite.json").bufferedReader(Charsets.UTF_8).use { it.readText() })
        val cases = suite.getJSONArray("cases")
        val failures = ArrayList<String>()
        val ran = ArrayList<String>()
        for (nth in 0 until cases.length()) {
            val case = cases.getJSONObject(nth)
            val name = case.getString("name")
            runCase(case).forEach { failures.add("$name: $it") }
            ran.add(name)
        }
        writeRecord(ran, failures)
        assertTrue("the suite did not hold:\n  " + failures.joinToString("\n  "), failures.isEmpty())
        assertEquals("ran ${ran.size} of the suite's ${cases.length()} cases", cases.length(), ran.size)
        assertTrue("the suite has ${ran.size} cases, so the read is wrong rather than the suite short", ran.size >= 30)
    }

    /**
     * Runs one case on a scratch database and returns what did not hold.
     *
     * A case marked `"connection": "per_call"` asks for a fresh connection handle for every statement,
     * all continuing one session. The suite says the C library already does that on every call made
     * through one `inillucent_conn`, so one [Connection] for the whole case satisfies it, as it does for
     * the Python runner. A new [Connection] per statement would be a new `inillucent_conn` and a new
     * session, which is a different case: the first run of this runner did that and the temporary
     * table was gone on the next statement.
     *
     * @param case - the case
     */
    private fun runCase(case: JSONObject): List<String> {
        val wrong = ArrayList<String>()
        val folder = File(instrumentation.targetContext.cacheDir, "conformance-${case.getString("name")}-${System.nanoTime()}")
        folder.mkdirs()
        Database.open(File(folder, "case.rdb")).use { database ->
            database.session().use { conn ->
                val setup = case.optJSONArray("setup") ?: JSONArray()
                for (s in 0 until setup.length()) {
                    val sql = setup.getString(s)
                    try {
                        conn.execute(sql)
                    } catch (why: InillucentException) {
                        wrong.add("the setup statement `$sql` was refused: ${why.message}")
                    }
                }
                if (wrong.isEmpty()) {
                    val steps = case.optJSONArray("steps") ?: JSONArray()
                    for (s in 0 until steps.length()) {
                        val step = steps.getJSONObject(s)
                        runStep(conn, step).forEach { wrong.add("`${step.getString("sql")}`: $it") }
                    }
                }
            }
        }
        folder.deleteRecursively()
        return wrong
    }

    /**
     * Runs one step and returns what did not hold.
     * @param conn - the connection
     * @param step - the step
     */
    private fun runStep(conn: Connection, step: JSONObject): List<String> {
        val said = ArrayList<String>()
        val params = step.optJSONArray("params")?.let { list -> (0 until list.length()).map { valueOf(list.getJSONObject(it)) } } ?: emptyList()
        val limit = if (step.has("limit")) step.getLong("limit") else null
        try {
            val rows = conn.query(step.getString("sql"), params, limit)
            checkSuccess(step, rows, said)
        } catch (failure: InillucentException) {
            checkFailure(step, failure, said)
        }
        return said
    }

    /**
     * Checks a step that was expected to succeed.
     * @param step - the step
     * @param rows - what it produced
     * @param said - where problems go
     */
    private fun checkSuccess(step: JSONObject, rows: Rows, said: MutableList<String>) {
        if (step.has("status")) {
            said.add("expected it to fail with `${step.getString("status")}` and it succeeded")
            return
        }
        if (step.has("columns")) {
            val want = step.getJSONArray("columns").let { list -> (0 until list.length()).map { list.getString(it) } }
            if (want != rows.columns) said.add("columns are ${rows.columns} and should be $want")
        }
        if (step.has("rows")) {
            val want = step.getJSONArray("rows")
            if (want.length() != rows.size) {
                said.add("there are ${rows.size} rows and there should be ${want.length()}")
            } else {
                for (r in 0 until want.length()) {
                    val row = want.getJSONArray(r)
                    val got = rows.rows[r]
                    if (row.length() != got.size) {
                        said.add("row $r has ${got.size} cells and should have ${row.length()}")
                        continue
                    }
                    for (c in 0 until row.length()) {
                        val expected = valueOf(row.getJSONObject(c))
                        if (!same(expected, got[c])) said.add("row $r column $c is ${shown(got[c])} and should be ${shown(expected)}")
                    }
                }
            }
        }
        if (step.has("affected")) {
            val want: Long? = if (step.isNull("affected")) null else step.getLong("affected")
            if (rows.affected != want) said.add("affected is ${rows.affected} and should be $want")
        }
        if (step.has("total") && rows.total != step.getLong("total")) said.add("total is ${rows.total} and should be ${step.getLong("total")}")
        if (step.has("more") && rows.more != step.getBoolean("more")) said.add("more is ${rows.more} and should be ${step.getBoolean("more")}")
    }

    /**
     * Checks a step that was expected to fail.
     * @param step - the step
     * @param failure - how it failed
     * @param said - where problems go
     */
    private fun checkFailure(step: JSONObject, failure: InillucentException, said: MutableList<String>) {
        if (!step.has("status")) {
            said.add("it was expected to succeed and it failed: ${failure.message}")
            return
        }
        val want = step.getString("status")
        if (failure.status.wireName != want) said.add("it failed with `${failure.status.wireName}` and should have failed with `$want` - ${failure.message}")
        if (step.has("message_contains") && !failure.engineMessage.contains(step.getString("message_contains"))) {
            said.add("the message is \"${failure.engineMessage}\" and should hold \"${step.getString("message_contains")}\"")
        }
        if (step.has("feature_contains")) {
            val feature = failure.feature
            if (feature.isNullOrEmpty()) {
                said.add("it named no construct, and an unsupported refusal has to name one")
            } else if (!feature.contains(step.getString("feature_contains"))) {
                said.add("it named \"$feature\" and should have named something holding \"${step.getString("feature_contains")}\"")
            }
        }
        if (failure.status == Status.UNSUPPORTED && failure !is UnsupportedException) said.add("an unsupported refusal did not arrive as UnsupportedException")
        if (failure.status == Status.UNSUPPORTED && failure.feature.isNullOrEmpty()) said.add("an unsupported refusal must carry a feature")
    }

    /**
     * Writes the record every conformance runner writes, for the tooling guard to read.
     * @param ran - the cases this run graded
     * @param failures - what did not hold
     */
    private fun writeRecord(ran: List<String>, failures: List<String>) {
        val record = JSONObject()
            .put("language", "android")
            .put("lacks", JSONArray())
            .put("ran", JSONArray(ran))
            .put("skipped", JSONArray())
            .put("failures", JSONArray(failures))
        File(instrumentation.targetContext.filesDir, "android.json").writeText(record.toString(2) + "\n", Charsets.UTF_8)
    }
}

/**
 * Reads a value out of the suite's one key object form.
 * @param described - the object
 */
internal fun valueOf(described: JSONObject): Any? = when {
    described.has("null") -> null
    described.has("int") -> described.getLong("int")
    described.has("real") -> described.getDouble("real")
    described.has("text") -> described.getString("text")
    described.has("blob") -> described.getJSONArray("blob").let { bytes -> ByteArray(bytes.length()) { bytes.getInt(it).toByte() } }
    else -> throw IllegalArgumentException("$described names no value kind")
}

/**
 * Compares an expected value to what came back, exactly and by kind: a Long is never equal to a
 * Double, the same rule the Python runner applies.
 * @param want - the expected value
 * @param got - the value the binding returned
 */
internal fun same(want: Any?, got: Any?): Boolean = when {
    want == null || got == null -> want == null && got == null
    want is ByteArray || got is ByteArray -> want is ByteArray && got is ByteArray && want.contentEquals(got)
    want is Double && got is Double -> want == got
    else -> want::class == got::class && want == got
}

/**
 * Renders a value for a failure message.
 * @param value - the value
 */
internal fun shown(value: Any?): String = when (value) {
    null -> "NULL"
    is ByteArray -> "${value.size} bytes ${value.map { it.toInt() and 0xff }}"
    is String -> "\"$value\" (String)"
    else -> "$value (${value::class.simpleName})"
}
