package com.blackrainbowlabs.inillucent

/**
 * One row of the engine's capability table.
 *
 * @property name - the capability, such as `cancel` or `fts5`
 * @property support - whether the engine does it
 * @property note - a sentence about it; read it when [support] is [Support.PARTIAL]
 */
data class Capability(val name: String, val support: Support, val note: String)

/** A capability's support level. */
enum class Support(val code: Int) {
    NO(0),
    YES(1),

    /** Yes, with the limit the note names. */
    PARTIAL(-1),

    /** No capability by that name in this build. Treat it as no: a capability nobody declared was never checked. */
    UNKNOWN(-2);

    companion object {
        /** The level for a C support code. @param code - the code */
        fun of(code: Int): Support = entries.firstOrNull { it.code == code } ?: UNKNOWN
    }
}

/**
 * The library itself: its version and its capability table.
 */
object Inillucent {
    /** The ABI major version this binding was written against. */
    const val ABI_MAJOR = 1

    /** The C library's ABI version as `major.minor.patch`. */
    @JvmStatic
    fun abiVersion(): String {
        val reported = Native.abiVersion()
        return "${reported / 1_000_000}.${(reported / 1000) % 1000}.${reported % 1000}"
    }

    /** What the driver calls itself, such as `inillucent-driver 2.9.0`. */
    @JvmStatic
    fun version(): String = utf8(Native.version())

    /** Every capability the engine declares, in the engine's order. */
    @JvmStatic
    fun capabilities(): List<Capability> = (0 until Native.capabilityCount()).map { nth ->
        val parts = Native.capability(nth)
        Capability(
            name = (parts[0] as ByteArray?)?.let(::utf8) ?: "",
            support = Support.of((parts[2] as IntArray)[0]),
            note = (parts[1] as ByteArray?)?.let(::utf8) ?: "",
        )
    }

    /**
     * Looks one capability up by name.
     * @param name - the capability
     */
    @JvmStatic
    fun supports(name: String): Support = Support.of(Native.supports(cString(name, "the capability name")))
}
