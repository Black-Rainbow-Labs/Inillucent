package com.blackrainbowlabs.inillucent

/**
 * Everything one statement produced. The engine materialises a result, so every count here was
 * taken rather than estimated.
 *
 * A cell is `null`, `Long`, `Double`, `String` or `ByteArray`.
 *
 * @property columns - the result's column names, in order
 * @property types - each column's declared type, or "" for an expression, which has none
 * @property rows - the rows handed back, at most the limit asked for
 * @property total - how many rows the statement produced, exactly, whatever the limit
 * @property more - whether the limit left rows out
 * @property affected - rows changed, or null for a statement that changed nothing (a query)
 * @property elapsedMicros - how long the engine took
 * @property tag - a one line summary such as "SELECT 27"
 */
class Rows(
    val columns: List<String>,
    val types: List<String>,
    val rows: List<List<Any?>>,
    val total: Long,
    val more: Boolean,
    val affected: Long?,
    val elapsedMicros: Long,
    val tag: String,
) : Iterable<List<Any?>> {
    override fun iterator(): Iterator<List<Any?>> = rows.iterator()

    /** How many rows were handed back. */
    val size: Int get() = rows.size

    /**
     * One row as a map from column name to value. A later column with the same name as an earlier
     * one wins, as it does in most drivers, so name the columns when that matters.
     * @param nth - the row's position
     */
    fun named(nth: Int): Map<String, Any?> = columns.zip(rows[nth]).toMap()

    companion object {
        /**
         * Reads the positional layout `read_rows` in `inillucent_jni.c` writes. That function is the
         * only writer and this is the only reader, so the layout is documented once, there.
         * @param parts - the layout
         */
        internal fun read(parts: Array<Any?>): Rows {
            @Suppress("UNCHECKED_CAST")
            val names = (parts[0] as Array<ByteArray?>).map { it?.let(::utf8) ?: "" }
            @Suppress("UNCHECKED_CAST")
            val types = (parts[1] as Array<ByteArray?>).map { it?.let(::utf8) ?: "" }
            val kinds = parts[2] as IntArray
            val ints = parts[3] as LongArray
            val reals = parts[4] as DoubleArray
            @Suppress("UNCHECKED_CAST")
            val bytes = parts[5] as Array<ByteArray?>
            val meta = parts[6] as LongArray
            val tag = (parts[7] as ByteArray?)?.let(::utf8) ?: ""
            val count = meta[0].toInt()
            val width = meta[1].toInt()
            val rows = ArrayList<List<Any?>>(count)
            for (r in 0 until count) {
                val row = ArrayList<Any?>(width)
                for (c in 0 until width) {
                    row.add(cell(kinds, ints, reals, bytes, r * width + c))
                }
                rows.add(row)
            }
            return Rows(names, types, rows, meta[2], meta[3] != 0L, meta[4].takeIf { it >= 0 }, meta[5], tag)
        }

        /**
         * One cell, as the Kotlin value its kind maps to.
         * @param kinds - the cells' kinds
         * @param ints - the integer cells
         * @param reals - the real cells
         * @param bytes - the text and blob cells
         * @param at - the cell's position
         */
        private fun cell(kinds: IntArray, ints: LongArray, reals: DoubleArray, bytes: Array<ByteArray?>, at: Int): Any? = when (kinds[at]) {
            KIND_INTEGER -> ints[at]
            KIND_REAL -> reals[at]
            KIND_TEXT -> utf8(bytes[at] ?: ByteArray(0))
            KIND_BLOB -> bytes[at] ?: ByteArray(0)
            else -> null
        }

        private const val KIND_INTEGER = 1
        private const val KIND_REAL = 2
        private const val KIND_TEXT = 3
        private const val KIND_BLOB = 4
    }
}
