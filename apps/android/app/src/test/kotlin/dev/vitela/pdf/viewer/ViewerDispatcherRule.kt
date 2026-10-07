package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.PdfCore
import dev.vitela.pdf.core.RenderedPage
import dev.vitela.pdf.home.NoRecentStore
import dev.vitela.pdf.home.RecentStore
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.setMain
import org.junit.rules.TestWatcher
import org.junit.runner.Description

/**
 * Keeps every coroutine a [ViewerViewModel] starts on the test thread.
 *
 * `Dispatchers.Main` becomes an unconfined test dispatcher, and the core's
 * work queues on a standard one sharing its scheduler, which `runTest` then
 * drives. Build the ViewModel with [viewModel], never with the bare
 * constructor: its production dispatchers hop to a real
 * `Dispatchers.Default` worker, and since an unconfined Main never
 * dispatches back, everything after the hop ran on that worker. The
 * ViewModel's `state.value = state.value.copy(...)` then raced the test
 * thread's own writes; a lost update left a `state.first { ... }` waiting
 * until `runTest` timed out, and work still in flight after `resetMain`
 * resumed onto a Main the JVM does not have.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class ViewerDispatcherRule : TestWatcher() {
    private val main = UnconfinedTestDispatcher()
    private val work = StandardTestDispatcher(main.scheduler)

    override fun starting(description: Description) = Dispatchers.setMain(main)

    override fun finished(description: Description) = Dispatchers.resetMain()

    fun viewModel(
        core: PdfCore?,
        signatures: SignatureStore = MemorySignatureStore(),
        recents: RecentStore = NoRecentStore,
        encodePreview: (RenderedPage) -> ByteArray? = { null },
        clock: () -> Long = { 0L },
    ) = ViewerViewModel(core, compute = work, io = work, signatures = signatures, recents = recents, encodePreview = encodePreview, clock = clock)
}
