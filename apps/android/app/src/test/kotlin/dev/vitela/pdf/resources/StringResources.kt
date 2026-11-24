package dev.vitela.pdf.resources

import dev.vitela.pdf.R
import org.w3c.dom.Element
import java.io.File
import java.util.Locale
import javax.xml.parsers.DocumentBuilderFactory

/**
 * Every `strings*.xml` of one qualifier, read straight from the sources: a
 * JVM test has no Resources to ask. Entries are unescaped the way aapt2 does for the escapes
 * the app uses.
 */
internal class StringResources(qualifier: String) {
    /** The file names, so a language can be held to having the same ones. */
    val files: List<String> = File("src/main/res/$qualifier").list().orEmpty().filter { it.startsWith("strings") && it.endsWith(".xml") }.sorted()

    private val children = files.flatMap { file ->
        val root = DocumentBuilderFactory.newInstance().newDocumentBuilder().parse(File("src/main/res/$qualifier/$file")).documentElement
        (0 until root.childNodes.length).map { root.childNodes.item(it) }.filterIsInstance<Element>()
    }

    val strings: Map<String, String> = children.filter { it.tagName == "string" }.associate { it.name to unescape(it.textContent) }

    /** Plural name to its quantities. */
    val plurals: Map<String, Map<String, String>> = children.filter { it.tagName == "plurals" }.associate { plural ->
        val items = plural.getElementsByTagName("item")
        plural.name to (0 until items.length).map { items.item(it) as Element }.associate { it.getAttribute("quantity") to unescape(it.textContent) }
    }

    /** Every `name`, duplicates included. */
    val names: List<String> = children.map { it.name }

    /** The entry [id] names, a string or a plural. */
    fun name(id: Int): String = (R.string::class.java.fields + R.plurals::class.java.fields).single { it.getInt(null) == id }.name

    /** How many arguments the entry [id] takes: its highest placeholder, in any of a plural's forms. */
    fun argumentCount(id: Int): Int {
        val name = name(id)
        val texts = strings[name]?.let(::listOf) ?: plurals.getValue(name).values
        return texts.flatMap { text -> Regex("""%(\d+)\$""").findAll(text).map { it.groupValues[1].toInt() } }.maxOrNull() ?: 0
    }

    fun text(id: Int, vararg args: Any): String = format(strings.getValue(nameOf(R.string::class.java, id)), args)

    fun plural(id: Int, quantity: String, vararg args: Any): String =
        format(plurals.getValue(nameOf(R.plurals::class.java, id)).getValue(quantity), args)

    private fun format(text: String, args: Array<out Any>) = if (args.isEmpty()) text else String.format(Locale.ROOT, text, *args)

    private fun nameOf(type: Class<*>, id: Int) = type.fields.single { it.getInt(null) == id }.name

    private val Element.name: String get() = getAttribute("name")

    private fun unescape(raw: String) = Regex("""\\(.)""").replace(raw) { if (it.groupValues[1] == "n") "\n" else it.groupValues[1] }

    companion object {
        val english by lazy { StringResources("values") }
        val spanish by lazy { StringResources("values-es") }
    }
}
