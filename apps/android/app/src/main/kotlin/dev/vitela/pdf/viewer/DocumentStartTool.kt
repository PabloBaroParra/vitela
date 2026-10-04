package dev.vitela.pdf.viewer

/** A Home shortcut applied only after its chosen PDF opens successfully. */
enum class DocumentStartTool { EditText, Highlight, Sign, Organize, Compress }

internal fun initialReaderMode(tool: DocumentStartTool?): ReaderMode = when (tool) {
    DocumentStartTool.EditText, DocumentStartTool.Highlight -> ReaderMode.Edit
    DocumentStartTool.Sign -> ReaderMode.Sign
    else -> ReaderMode.Read
}
