package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.CompressPreset
import dev.vitela.pdf.core.CompressedCopy
import java.math.BigInteger
import java.util.Locale

/**
 * The open Compress dialog: the chosen [preset], why the document cannot be
 * compressed ([refusal], a sentence) or null when it can, and whether the
 * compressed copy breaks the file's signature — in which case confirming the
 * dialog is the user's yes to that.
 */
data class CompressEditor(
    val preset: CompressPreset = DEFAULT_COMPRESS_PRESET,
    val refusal: String? = null,
    val signaturesWillBreak: Boolean = false,
)

/** The core's "default offer": visibly unchanged on screen, materially smaller on disk. */
internal val DEFAULT_COMPRESS_PRESET = CompressPreset.Balanced

// Every sentence below is the Windows shell's CompressionWording, which is in
// turn the GTK shell's, so one compression reads the same on every platform.
// `beforeBytes` is "uncompressed", never "the original": it is a save of the
// session, not the file on disk once anything was edited. Nothing predicts a
// saving — the core cannot answer that without running.

internal fun describePreset(preset: CompressPreset): Pair<String, String> = when (preset) {
    CompressPreset.Lossless -> "Lossless" to "Repack the file only. Every page renders exactly as it does now."
    CompressPreset.Balanced -> "Balanced" to "Also bring images down to 150 DPI. Visibly unchanged on screen, materially smaller on disk."
    CompressPreset.Small -> "Small" to "Also bring images down to 96 DPI. For when the file has to fit through something and the pixels matter less than that."
}

/** Powers of ten, the way the other shells count. */
internal fun humanSize(bytes: Long): String = when {
    bytes >= 1_000_000 -> String.format(Locale.ROOT, "%.1f MB", bytes / 1_000_000.0)
    bytes >= 1_000 -> String.format(Locale.ROOT, "%.0f kB", bytes / 1_000.0)
    bytes == 1L -> "1 byte"
    else -> "$bytes bytes"
}

/** Truncated rather than rounded, so the number shown is one the file actually reached: 49.7% reads 49%. */
internal fun percentSmaller(copy: CompressedCopy): Long =
    if (copy.beforeBytes <= 0) 0
    else BigInteger.valueOf(copy.savedBytes).multiply(BigInteger.valueOf(100)).divide(BigInteger.valueOf(copy.beforeBytes)).toLong()

internal fun compressReductionSummary(copy: CompressedCopy): String =
    "${humanSize(copy.beforeBytes)} uncompressed → ${humanSize(copy.afterBytes)} compressed (${percentSmaller(copy)}% smaller). Choose where to write it.${refusalClause(copy)}"

/**
 * No destination is asked for: the bytes on offer are the ones an ordinary
 * save writes, and a picker would invite filing them under a name that
 * promises they are smaller.
 */
internal fun compressNoGainSummary(copy: CompressedCopy): String =
    "Nothing to gain: this document compresses to the same ${humanSize(copy.beforeBytes)} an uncompressed save writes, so no file was written.${refusalClause(copy)}"

/** The Windows summary without its destination: a SAF document has no path to show. */
internal fun compressWrittenSummary(copy: CompressedCopy): String =
    "Compressed PDF written (${humanSize(copy.beforeBytes)} → ${humanSize(copy.afterBytes)}, ${percentSmaller(copy)}% smaller)."

/** The core's clause as a sentence the dialog can show on its own. */
internal fun compressRefusalSentence(clause: String): String = "${clause.replaceFirstChar { it.uppercaseChar() }}."

internal fun compressedFileName(title: String): String = "${documentStem(title)}-compressed.pdf"

/**
 * Both known refusals are gated before the run, so this is normally empty —
 * but the core's list is open-ended, and one this shell did not ask about
 * must still reach the reader.
 */
private fun refusalClause(copy: CompressedCopy): String =
    if (copy.refusals.isEmpty()) "" else " Not everything could be done: ${copy.refusals.joinToString("; ")}."
