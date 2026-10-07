package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.RenderedPage
import dev.vitela.pdf.home.RecentDocument
import dev.vitela.pdf.home.RecentStore
import dev.vitela.pdf.home.remembering
import dev.vitela.pdf.home.without
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext

/** The longer side of a Recent card's preview, in pixels: a 140dp frame at ~3x density. */
internal const val RECENT_PREVIEW_PX = 420

/**
 * Home's Recent list. Loaded on first use rather than at construction — a
 * ViewModel must not reach for `viewModelScope` just by existing — and
 * updated when an open succeeds or the user removes a card. One lane of its
 * own, so a remember and a removal never interleave their writes, and the
 * document lane is never held for storage.
 *
 * [encodePng] turns a rendered first page into the preview kept on disk;
 * tests pass their own so this runs on the JVM.
 */
internal class RecentRemembering(
    private val session: ViewerSession,
    private val store: RecentStore,
    private val encodePng: (RenderedPage) -> ByteArray?,
) {
    private val _entries = MutableStateFlow<List<RecentDocument>>(emptyList())
    val entries: StateFlow<List<RecentDocument>> = _entries.asStateFlow()
    private val lane = Mutex()
    private var loaded = false

    fun load() {
        session.scope.launch { lane.withLock { ensureLoaded() } }
    }

    /**
     * Puts [entry] first. A null [preview] deletes any old one: a document
     * reopened under a password must not leave its earlier first page behind.
     */
    fun remember(entry: RecentDocument, preview: RenderedPage?) {
        session.scope.launch {
            lane.withLock {
                ensureLoaded()
                val previous = _entries.value
                val next = previous.remembering(entry)
                _entries.value = next
                withContext(session.io) {
                    val png = preview?.let(encodePng)
                    if (png == null || !store.saveThumbnail(entry.uri, png)) store.deleteThumbnail(entry.uri)
                    store.save(next)
                    val kept = next.mapTo(HashSet()) { it.uri }
                    previous.filter { it.uri !in kept }.forEach { store.deleteThumbnail(it.uri) }
                }
            }
        }
    }

    /** Takes [uri] off the list and deletes its preview; it comes back only when opened again. */
    fun remove(uri: String) {
        session.scope.launch {
            lane.withLock {
                ensureLoaded()
                val next = _entries.value.without(uri)
                _entries.value = next
                withContext(session.io) {
                    store.save(next)
                    store.deleteThumbnail(uri)
                }
            }
        }
    }

    suspend fun thumbnail(uri: String): ByteArray? = withContext(session.io) { store.thumbnail(uri) }

    private suspend fun ensureLoaded() {
        if (loaded) return
        _entries.value = withContext(session.io) { store.load() }
        loaded = true
    }
}
