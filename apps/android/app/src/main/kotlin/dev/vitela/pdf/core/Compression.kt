package dev.vitela.pdf.core

/**
 * How hard a compression tries — pdf-ffi's `FfiCompressPreset`, strongest-preserving
 * first. Three, not a slider: the DPI and JPEG quality behind each one stay in
 * `pdf-compress`, so this shell cannot drift into its own idea of "smaller".
 */
enum class CompressPreset { Lossless, Balanced, Small }

/**
 * A compressed save: the [bytes] to write and what the core measured. [beforeBytes]
 * is an uncompressed save of the session, not the file on disk; [savedBytes] and
 * [reduced] are the core's, carried rather than re-derived. [refusals] are the
 * core's clauses for anything it could not do.
 */
class CompressedCopy(
    val bytes: ByteArray,
    val beforeBytes: Long,
    val afterBytes: Long,
    val savedBytes: Long,
    val reduced: Boolean,
    val refusals: List<String>,
)
