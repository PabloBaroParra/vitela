using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls.Primitives;
using Pdf.Windows.Viewer;

namespace Pdf.Windows;

/// <summary>
/// What a command palette entry runs (B27 decision 2): every command walks
/// through the handler its button or accelerator already calls, after at most
/// a screen switch to where that button lives. Nothing here decides a
/// permission — <see cref="PaletteCatalog"/> mirrors the buttons' gates from
/// <see cref="PaletteContextNow"/>, and each handler keeps its own guards.
/// </summary>
public sealed partial class MainWindow
{
    /// <summary>The facts the buttons' gates read, for the palette's availability rules.</summary>
    private PaletteContext PaletteContextNow()
    {
        var session = _session;
        var annotations = _annotationState is { } state && state.SessionId == session?.SessionId ? state : null;
        var forms = _formFieldState is { } fields && fields.SessionId == session?.SessionId ? fields : null;
        var signing = _signingState is { } sign && sign.SessionId == session?.SessionId ? sign : null;
        return new PaletteContext(
            HasDocument: session is not null,
            PageCount: (int)(session?.PageCount ?? 0),
            AnnotationEditingAllowed: annotations?.EditingAllowed,
            ContentEditingAllowed: session?.ContentEditingAllowed == true,
            FormStructureAllowed: forms?.StructureAllowed,
            SigningLoaded: signing is not null,
            SigningRefusal: signing?.Refusal,
            CanUndo: UndoButton.IsEnabled,
            CanRedo: RedoButton.IsEnabled);
    }

    /// <summary>
    /// Runs <paramref name="entry"/> unless its gate refuses it now; then the
    /// refusal goes to the status line and nothing runs. Returns whether it ran.
    /// </summary>
    private async Task<bool> RunPaletteEntryAsync(PaletteEntry entry)
    {
        if (entry.Refusal(PaletteContextNow()) is { } refusal)
        {
            AnnotationStatus.Text = refusal;
            return false;
        }
        await RunPaletteCommandAsync(entry.Command);
        return true;
    }

    private async Task RunPaletteCommandAsync(PaletteCommand command)
    {
        var args = new RoutedEventArgs();
        switch (command)
        {
            case PaletteCommand.ModeHome: ShowHomeView("Home"); break;
            case PaletteCommand.ModeEdit: await NavigateToToolAsync("Edit", args); break;
            case PaletteCommand.ModeAnnotate: await NavigateToToolAsync("Annotate", args); break;
            case PaletteCommand.ModeOrganize: await NavigateToToolAsync("Organize", args); break;
            case PaletteCommand.ModeSign: await NavigateToToolAsync("Sign", args); break;
            case PaletteCommand.ModeProtect: await NavigateToToolAsync("Protect", args); break;

            case PaletteCommand.Open: await PickShellDocumentAsync(); break;
            case PaletteCommand.New: await CreateNewDocumentAsync(); break;
            // The Ctrl+S / Ctrl+P accelerators' own guards.
            case PaletteCommand.Save:
                if (!_isBusy && !_dialogOpen && !_shellPickingFile && SaveButton.IsEnabled) await SaveToPickedFileAsync();
                break;
            case PaletteCommand.Print:
                if (!_isBusy && !_dialogOpen && !_shellPickingFile && _session is { PageCount: > 0 }) PrintButton_Click(PrintButton, args);
                break;

            // Organize has its own history buttons, which also re-read the page layout.
            case PaletteCommand.Undo: await RunPaletteHistoryAsync(undo: true); break;
            case PaletteCommand.Redo: await RunPaletteHistoryAsync(undo: false); break;
            case PaletteCommand.Find: ShowFindFlyout(); break;

            case PaletteCommand.Highlight: ArmPaletteToggle("Annotate", HighlightButton, HighlightButton_Click); break;
            case PaletteCommand.Underline: ArmPaletteToggle("Annotate", UnderlineButton, UnderlineButton_Click); break;
            case PaletteCommand.Strikeout: ArmPaletteToggle("Annotate", StrikeoutButton, StrikeoutButton_Click); break;
            case PaletteCommand.Ink: ArmPaletteToggle("Annotate", InkButton, InkButton_Click); break;
            case PaletteCommand.Note: ArmPaletteToggle("Annotate", NoteButton, NoteButton_Click); break;
            case PaletteCommand.TextBox: ArmPaletteToggle("Annotate", FreeTextButton, FreeTextButton_Click); break;
            case PaletteCommand.Shape: ArmPaletteToggle("Annotate", ShapeButton, ShapeButton_Click); break;
            case PaletteCommand.Stamp: ArmPaletteToggle("Annotate", StampButton, StampButton_Click); break;

            case PaletteCommand.TextField: ArmPaletteToggle("Sign", PlaceTextFieldButton, PlaceTextFieldButton_Click); break;
            case PaletteCommand.Checkbox: ArmPaletteToggle("Sign", PlaceCheckboxButton, PlaceCheckboxButton_Click); break;
            case PaletteCommand.RadioGroup: ArmPaletteToggle("Sign", PlaceRadioGroupButton, PlaceRadioGroupButton_Click); break;
            case PaletteCommand.Dropdown: ArmPaletteToggle("Sign", PlaceDropdownButton, PlaceDropdownButton_Click); break;

            case PaletteCommand.ExportImages: ExportImagesButton_Click(ExportImagesButton, args); break;
            case PaletteCommand.Watermark:
                ShowPaletteToolsPage("Edit");
                if (AddWatermarkButton.IsEnabled) AddWatermarkButton_Click(AddWatermarkButton, args);
                break;
            // A Home tool: from a cold start the rail's door picks a file first.
            case PaletteCommand.Compress: await NavigateToToolAsync("Compress", args); break;

            // Add PDFs reports its progress, and offers its cancel, in the Organize header.
            case PaletteCommand.AddPdfs:
                if (!_organizing) await EnterOrganizeViewAsync();
                if (_organizing && OrganizeAddPdfsButton.IsEnabled) OrganizeAddPdfsButton_Click(OrganizeAddPdfsButton, args);
                break;
            case PaletteCommand.Extract: ExtractPagesButton_Click(ExtractPagesButton, args); break;
            case PaletteCommand.Split: SplitPagesButton_Click(SplitPagesButton, args); break;

            // The signing buttons live on Fill & Sign, and a drawn signature is
            // disarmed as soon as that page is left, so all four start there.
            case PaletteCommand.SignWithFile: RunPaletteSigning(_chooseSigningCertificate); break;
            case PaletteCommand.SignWithToken: RunPaletteSigning(_chooseCardCertificate); break;
            case PaletteCommand.SignWithSystemCertificate: RunPaletteSigning(_chooseComputerCertificate); break;
            case PaletteCommand.DrawSignature:
                ShowPaletteToolsPage("Sign");
                DrawSignatureButton_Click(_drawSignatureButton, args);
                break;
            default: throw new ArgumentOutOfRangeException(nameof(command), command, null);
        }
    }

    private async Task RunPaletteHistoryAsync(bool undo)
    {
        if (_organizing)
        {
            await RunOrganizeHistoryAsync(undo);
            return;
        }
        if (_isBusy || _dialogOpen || !(undo ? UndoButton.IsEnabled : RedoButton.IsEnabled)) return;
        await ApplyHistoryAndMetadataAsync(undo);
    }

    /// <summary>Shows the editor with <paramref name="page"/> up on the tools panel, leaving Organize or Home first.</summary>
    private void ShowPaletteToolsPage(string page)
    {
        if (_organizing) LeaveOrganizeView();
        ShowEditorView();
        SetToolsPanelVisible(true);
        SelectToolPage(page);
    }

    /// <summary>
    /// Switches a tool's toggle on and runs its own Click handler — exactly
    /// what a click on it does. Already on stays on; a disabled toggle (its
    /// gate refused after the screen switch) is left alone.
    /// </summary>
    private void ArmPaletteToggle(string page, ToggleButton button, RoutedEventHandler click)
    {
        ShowPaletteToolsPage(page);
        if (!button.IsEnabled || button.IsChecked == true) return;
        button.IsChecked = true;
        click(button, new RoutedEventArgs());
    }

    private void RunPaletteSigning(Microsoft.UI.Xaml.Controls.Button source)
    {
        ShowPaletteToolsPage("Sign");
        if (source.IsEnabled) ChooseSigningCertificate_Click(source, new RoutedEventArgs());
    }
}
