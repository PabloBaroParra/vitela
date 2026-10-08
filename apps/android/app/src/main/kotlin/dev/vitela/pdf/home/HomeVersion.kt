package dev.vitela.pdf.home

/**
 * Home's footer: which build of Vitela is running. A release's versionName is
 * the tag's semver (android.yml passes it from scripts/release-version.sh);
 * every other build's is "dev", so it never claims a number it does not have.
 */
internal fun homeVersionLabel(versionName: String?): String =
    "Vitela ${versionName?.trim()?.takeIf { it.isNotEmpty() } ?: "dev"}"
