package dev.vitela.pdf.sample

import java.io.File
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Guards the asset wiring rather than the reader: [SampleDocument.read] needs
 * a real [android.content.res.AssetManager], but what actually breaks silently
 * is the `assets.srcDir` in `build.gradle.kts` drifting from the shared
 * `assets/sample/` directory — the APK would then ship without the sample and
 * "Open sample" would fail only at runtime, on a device.
 */
class SampleDocumentTest {
    /** Mirrors the `assets.srcDir` declared in `app/build.gradle.kts`. */
    private val packagedAssetDir = File("../../../assets/sample")

    @Test
    fun packagedAssetDirectoryContainsTheSampleDocument() {
        val sample = File(packagedAssetDir, SampleDocument.ASSET_NAME)
        assertTrue(
            "expected the shared sample at ${sample.absolutePath}; " +
                "regenerate it with `cargo run -p gen-sample`",
            sample.isFile(),
        )
    }

    @Test
    fun packagedSampleIsAPdf() {
        val header = File(packagedAssetDir, SampleDocument.ASSET_NAME).readBytes().take(5)
        assertTrue("the packaged sample must be a PDF", header == "%PDF-".toByteArray().toList())
    }
}
