package com.blackrainbowlabs.inillucent

/**
 * Why a call failed, by the names the conformance suite and every other binding use.
 *
 * @property code - the C library's status number, frozen at ABI 1.0
 * @property wireName - the name as `drivers/conformance/suite.json` writes it
 */
enum class Status(val code: Int, val wireName: String) {
    OK(0, "ok"),
    UNSUPPORTED(1, "unsupported"),
    SYNTAX(2, "syntax"),
    NOT_FOUND(3, "not_found"),
    CONSTRAINT(4, "constraint"),
    READONLY(5, "readonly"),
    BUSY(6, "busy"),
    INTERRUPTED(7, "interrupted"),
    CORRUPT(8, "corrupt"),
    IO(9, "io"),
    FULL(10, "full"),
    TOO_BIG(11, "too_big"),
    INVALID_STATE(12, "invalid_state"),
    INTERNAL(13, "internal");

    companion object {
        /**
         * The status for a C status number. A number this build does not know is reported as
         * [INTERNAL], since a library newer than the binding is the only way to get one.
         * @param code - the number
         */
        fun of(code: Int): Status = entries.firstOrNull { it.code == code } ?: INTERNAL
    }
}

/**
 * A call the engine refused.
 *
 * Catch [UnsupportedException] on its own before this: the engine refuses what it has not built with
 * a status of its own, and an application should be able to say "not built yet" rather than "wrong".
 *
 * @property status - why it failed
 * @property feature - the construct an `unsupported` refusal names, otherwise null
 * @property detail - diagnostic text, only when the database was opened with diagnostics. It may hold
 *   a path or a bound value, so do not show it to a person
 * @property offset - the byte offset into the statement, or null
 * @property partialRows - the rows the statement produced before it failed, or null
 */
open class InillucentException(
    val status: Status,
    message: String,
    val feature: String? = null,
    val detail: String? = null,
    val offset: Int? = null,
    val partialRows: Rows? = null,
) : RuntimeException(if (offset != null) "$message [${status.wireName}] at byte $offset" else "$message [${status.wireName}]") {
    /** The engine's own message, without the status and offset this exception's message adds. */
    val engineMessage: String = message
}

/**
 * The engine has not built the construct the statement uses. [feature] names it.
 */
class UnsupportedException(
    message: String,
    feature: String?,
    detail: String? = null,
    offset: Int? = null,
    partialRows: Rows? = null,
) : InillucentException(Status.UNSUPPORTED, message, feature, detail, offset, partialRows)
