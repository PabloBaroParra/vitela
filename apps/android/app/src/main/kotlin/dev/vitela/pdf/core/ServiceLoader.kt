package dev.vitela.pdf.core

import java.util.ServiceLoader as JavaServiceLoader

internal object ServiceLoader {
    fun <T> load(type: Class<T>): Sequence<T> = JavaServiceLoader.load(type).iterator().asSequence()
}
