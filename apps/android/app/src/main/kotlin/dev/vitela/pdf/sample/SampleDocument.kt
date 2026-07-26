package dev.vitela.pdf.sample

import android.content.res.AssetManager

/**
 * The sample document packaged inside the APK, so a fresh install can render,
 * scroll, search, and print without the user first picking a PDF through the
 * Storage Access Framework.
 *
 * The file itself is not stored in this module: `build.gradle.kts` registers
 * the repository's shared `assets/sample/` directory as an asset source, so
 * Android, Windows, and Linux all ship the very same bytes.
 */
object SampleDocument {
    /** Asset path inside the APK — the file name as it exists in `assets/sample/`. */
    const val ASSET_NAME: String = "vitela-sample.pdf"

    /** Title shown in the viewer and used as the print job name. */
    const val DISPLAY_NAME: String = "Vitela sample.pdf"

    /**
     * Reads the packaged sample. Does blocking I/O off the asset manager, so
     * callers must invoke it from a background dispatcher — the same rule the
     * Storage Access Framework read path already follows.
     */
    fun read(assets: AssetManager): ByteArray = assets.open(ASSET_NAME).use { it.readBytes() }
}
