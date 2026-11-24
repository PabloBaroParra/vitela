package dev.vitela.pdf.text

import dev.vitela.pdf.resources.StringResources
import org.junit.Assert.assertEquals

/** What the reader sees of this text in English, worded from `values/`. */
internal fun UiText.english(): String = resolve(XmlWords(StringResources.english))

internal fun UiText.spanish(): String = resolve(XmlWords(StringResources.spanish))

/** English and Spanish agree on the plural form of every whole number the app counts: `one` is exactly one. */
private class XmlWords(private val resources: StringResources) : UiTextWords {
    override fun string(id: Int, args: Array<Any>): String {
        assertEquals("arguments of ${resources.name(id)}", resources.argumentCount(id), args.size)
        return resources.text(id, *args)
    }

    override fun plural(id: Int, count: Int, args: Array<Any>): String {
        assertEquals("arguments of ${resources.name(id)}", resources.argumentCount(id), args.size)
        return resources.plural(id, if (count == 1) "one" else "other", *args)
    }
}
