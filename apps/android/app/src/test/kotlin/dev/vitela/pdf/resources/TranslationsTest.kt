package dev.vitela.pdf.resources

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** A missing key falls back to English silently, and a mismatched placeholder crashes only in that language. */
class TranslationsTest {
    private val english = StringResources.english
    private val spanish = StringResources.spanish

    @Test
    fun spanishHasExactlyTheEnglishNames() {
        assertTrue(english.strings.isNotEmpty())
        assertEquals(listOf("strings.xml", "strings_status.xml"), english.files)
        assertEquals(english.files, spanish.files)
        assertEquals(english.strings.keys, spanish.strings.keys)
        assertEquals(english.plurals.keys, spanish.plurals.keys)
    }

    @Test
    fun noNameIsDefinedTwice() {
        listOf(english, spanish).forEach { assertEquals(it.names.distinct(), it.names) }
    }

    @Test
    fun everyPluralCoversOneAndOther() {
        listOf(english, spanish).forEach { resources ->
            resources.plurals.forEach { (name, items) -> assertTrue(name, items.keys.containsAll(setOf("one", "other"))) }
        }
    }

    @Test
    fun nothingIsBlank() {
        listOf(english, spanish).forEach { resources ->
            resources.strings.forEach { (name, text) -> assertTrue(name, text.isNotBlank()) }
            resources.plurals.forEach { (name, items) -> items.values.forEach { assertTrue(name, it.isNotBlank()) } }
        }
    }

    @Test
    fun placeholdersMatchAndArePositional() {
        english.strings.forEach { (name, text) -> assertEquals(name, placeholders(text), placeholders(spanish.strings.getValue(name))) }
        english.plurals.forEach { (name, items) ->
            val expected = placeholders(items.getValue("other"))
            // The `one` form may leave the number out ("Page moved."), in both languages or in neither.
            val one = placeholders(items.getValue("one"))
            assertTrue(name, one == expected || one == expected - "%1\$s")
            listOf(items, spanish.plurals.getValue(name)).forEach { forms ->
                forms.forEach { (quantity, text) -> assertEquals(name, if (quantity == "one") one else expected, placeholders(text)) }
            }
        }
    }

    /** The format specifiers of [text], sorted: a translation may reorder them. `%%` is text, not a specifier. */
    private fun placeholders(text: String): List<String> {
        val found = Regex("""%(?:%|\d+\$[a-z]|.?)""").findAll(text).map { it.value }.filter { it != "%%" }.toList()
        found.forEach { assertTrue("$it in \"$text\" is not positional", Regex("""%\d+\$[a-z]""").matches(it)) }
        return found.sorted()
    }
}
