namespace Pdf.Windows.Facade;

/// <summary>
/// The output snapshot print and export render from: the session as a save
/// would write it, reopened by the core as a throwaway document.
/// </summary>
/// <remarks>
/// <para>
/// Neither may render an annotated session from its own document. What that
/// document renders is the preview <see cref="IPdfCore.RefreshPreview"/>
/// builds, and the preview leaves this session's annotations out on purpose:
/// the shell draws them itself, as an overlay, so baking them in would paint
/// each one twice. Print and export have no overlay pass, so a page rendered
/// from it silently loses every annotation the reader added — saved or not,
/// because this shell never reopens after a save.
/// </para>
/// <para>
/// The core builds the snapshot (<see cref="IPdfCore.OutputSnapshot"/>), not
/// this facade: reopening an encrypted document's save needs the password it
/// was opened with, and the core already holds that one.
/// </para>
/// </remarks>
public sealed partial class PdfDocumentFacade
{
    /// <summary>
    /// A snapshot of <paramref name="session"/> when it has annotations the
    /// preview leaves out; otherwise <c>null</c>, and the caller renders the
    /// session's own document — there is nothing missing from it, and no save
    /// that could fail. Blocking: call it off the UI thread, behind the
    /// document gate.
    /// </summary>
    private IPdfCoreDocument? SnapshotIfAnnotated(SessionEntry session) =>
        _core.Annotations(session.Document).Count > 0 ? _core.OutputSnapshot(session.Document) : null;
}
