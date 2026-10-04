namespace Pdf.Windows.Facade;

public enum DocumentBlockSource { Base, Blank, Imported }

public sealed record DocumentBlock(DocumentBlockSource Source, uint? Part, uint Start, uint Count, ulong? ImportedSourceId = null);
public sealed record DocumentBlocksSnapshot(string SessionId, ulong Revision, IReadOnlyList<DocumentBlock> Blocks);
public enum DocumentBlockAction { Move, RotateLeft, RotateRight, Delete }
