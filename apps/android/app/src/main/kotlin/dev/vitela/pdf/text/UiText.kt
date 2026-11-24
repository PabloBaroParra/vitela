package dev.vitela.pdf.text

import androidx.annotation.PluralsRes
import androidx.annotation.StringRes
import androidx.compose.runtime.Composable
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext

/**
 * A sentence for the reader, named but not yet worded: what a ViewModel or a
 * use case publishes without a Context. It is worded where it is drawn, so a
 * change of language words it again. Equal values are the same sentence.
 *
 * An argument is a [UiText], a String or an Int. Strings are passed as they
 * are: file names, document content.
 */
sealed interface UiText {
    data class Resource(@StringRes val id: Int, val args: List<Any>) : UiText {
        constructor(@StringRes id: Int, vararg args: Any) : this(id, args.toList())
    }

    /** [count] only picks the form; a form that shows it takes it in [args] too. */
    data class Plural(@PluralsRes val id: Int, val count: Int, val args: List<Any>) : UiText {
        constructor(@PluralsRes id: Int, count: Int, vararg args: Any) : this(id, count, args.toList())
    }

    /** Text that has no resource: the core's own sentence. */
    data class Verbatim(val text: String) : UiText

    /** Whole sentences, one after another. */
    data class Sentences(val parts: List<UiText>) : UiText {
        constructor(vararg parts: UiText) : this(parts.toList())
    }
}

/** Where the words come from: the app's resources, or the XML itself in a test. */
internal interface UiTextWords {
    fun string(@StringRes id: Int, args: Array<Any>): String
    fun plural(@PluralsRes id: Int, count: Int, args: Array<Any>): String
}

internal fun UiText.resolve(words: UiTextWords): String {
    fun List<Any>.worded(): Array<Any> = map { if (it is UiText) it.resolve(words) else it }.toTypedArray()
    return when (this) {
        is UiText.Resource -> words.string(id, args.worded())
        is UiText.Plural -> words.plural(id, count, args.worded())
        is UiText.Verbatim -> text
        is UiText.Sentences -> parts.joinToString(" ") { it.resolve(words) }
    }
}

@Composable
@ReadOnlyComposable
internal fun UiText.resolve(): String {
    // Read so that a change of language recomposes, as stringResource does.
    LocalConfiguration.current
    val resources = LocalContext.current.resources
    return resolve(
        object : UiTextWords {
            override fun string(id: Int, args: Array<Any>) = if (args.isEmpty()) resources.getString(id) else resources.getString(id, *args)
            override fun plural(id: Int, count: Int, args: Array<Any>) =
                if (args.isEmpty()) resources.getQuantityString(id, count) else resources.getQuantityString(id, count, *args)
        },
    )
}
