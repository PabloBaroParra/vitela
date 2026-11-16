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
        // The search's own permission question (/P bit 5), asked of the facade as text selection asks it.
        var extraction = session is null ? null : _facade.SelectionAllowed(session.SessionId);
        return new PaletteContext(
            HasDocument: session is not null,
            PageCount: (int)(session?.PageCount ?? 0),
            AnnotationEditingAllowed: annotations?.EditingAllowed,
            ContentEditingAllowed: session?.ContentEditingAllowed == true,
            FormStructureAllowed: forms?.StructureAllowed,
            SigningLoaded: signing is not null,
            SigningRefusal: signing?.Refusal,
            CanUndo: UndoButton.IsEnabled,
            CanRedo: RedoButton.IsEnabled,
            TextExtractionRefusal: extraction is { IsSuccess: false } ? extraction.Error!.Message : null);
    }

    /// <summary>
    /// Runs <paramref name="entry"/> unless the shell is busy, a dialog is up
    /// or a picker is open — the rail's own guard, which every entry keeps,
    /// screen switches included. An entry whose button turns out to be off
    /// once its screen is up says so on the status line. Returns whether it
    /// ran; only then does it count as recent.
    /// </summary>
    private async Task<bool> RunPaletteEntryAsync(PaletteEntry entry)
    {
        if (_isBusy || _dialogOpen || _shellPickingFile) return false;
        if (await RunPaletteCommandAsync(entry.Command)) return true;
        AnnotationStatus.Text = $"{entry.Title.TrimEnd('…')} is not available right now.";
        return false;
    }

    /// <summary>Runs <paramref name="command"/>; false when the button it walks through was off.</summary>
    private async Task<bool> RunPaletteCommandAsync(PaletteCommand command)
    {
        var args = new RoutedEventArgs();
        switch (command)
        {
            case PaletteCommand.ModeHome: ShowHomeView("Home"); return true;
            case PaletteCommand.ModeEdit: await NavigateToToolAsync("Edit", args); return true;
            case PaletteCommand.ModeAnnotate: await NavigateToToolAsync("Annotate", args); return true;
            case PaletteCommand.ModeOrganize: await NavigateToToolAsync("Organize", args); return true;
            case PaletteCommand.ModeSign: await NavigateToToolAsync("Sign", args); return true;
            case PaletteCommand.ModeProtect: await NavigateToToolAsync("Protect", args); return true;

            case PaletteCommand.Open: await PickShellDocumentAsync(); return true;
            case PaletteCommand.New: await CreateNewDocumentAsync(); return true;
            // The Ctrl+S / Ctrl+P accelerators' own guards.
            case PaletteCommand.Save: return await SaveFromShellAsync();
            case PaletteCommand.Print: return PrintFromShell();

            // Organize has its own history buttons, which also re-read the page layout.
            case PaletteCommand.Undo: return await RunPaletteHistoryAsync(undo: true);
            case PaletteCommand.Redo: return await RunPaletteHistoryAsync(undo: false);
            case PaletteCommand.Find: return ShowFindFlyout();

            case PaletteCommand.Highlight: return ArmPaletteToggle("Annotate", HighlightButton, HighlightButton_Click);
            case PaletteCommand.Underline: return ArmPaletteToggle("Annotate", UnderlineButton, UnderlineButton_Click);
            case PaletteCommand.Strikeout: return ArmPaletteToggle("Annotate", StrikeoutButton, StrikeoutButton_Click);
            case PaletteCommand.Ink: return ArmPaletteToggle("Annotate", InkButton, InkButton_Click);
            case PaletteCommand.Note: return ArmPaletteToggle("Annotate", NoteButton, NoteButton_Click);
            case PaletteCommand.TextBox: return ArmPaletteToggle("Annotate", FreeTextButton, FreeTextButton_Click);
            case PaletteCommand.Shape: return ArmPaletteToggle("Annotate", ShapeButton, ShapeButton_Click);
            case PaletteCommand.Stamp: return ArmPaletteToggle("Annotate", StampButton, StampButton_Click);

            case PaletteCommand.TextField: return ArmPaletteToggle("Sign", PlaceTextFieldButton, PlaceTextFieldButton_Click);
            case PaletteCommand.Checkbox: return ArmPaletteToggle("Sign", PlaceCheckboxButton, PlaceCheckboxButton_Click);
            case PaletteCommand.RadioGroup: return ArmPaletteToggle("Sign", PlaceRadioGroupButton, PlaceRadioGroupButton_Click);
            case PaletteCommand.Dropdown: return ArmPaletteToggle("Sign", PlaceDropdownButton, PlaceDropdownButton_Click);

            case PaletteCommand.ExportImages: return ClickIfEnabled(ExportImagesButton, ExportImagesButton_Click);
            case PaletteCommand.Watermark:
                ShowPaletteToolsPage("Edit");
                return ClickIfEnabled(AddWatermarkButton, AddWatermarkButton_Click);
            // A Home tool: from a cold start the rail's door picks a file first.
            case PaletteCommand.Compress: await NavigateToToolAsync("Compress", args); return true;

            // Add PDFs reports its progress, and offers its cancel, in the Organize header.
            case PaletteCommand.AddPdfs:
                if (!_organizing) await EnterOrganizeViewAsync();
                return _organizing && ClickIfEnabled(OrganizeAddPdfsButton, OrganizeAddPdfsButton_Click);
            case PaletteCommand.Extract: return ClickIfEnabled(ExtractPagesButton, ExtractPagesButton_Click);
            case PaletteCommand.Split: return ClickIfEnabled(SplitPagesButton, SplitPagesButton_Click);

            // The signing buttons live on Fill & Sign, and a drawn signature is
            // disarmed as soon as that page is left, so all four start there.
            case PaletteCommand.SignWithFile: return RunPaletteSigning(_chooseSigningCertificate);
            case PaletteCommand.SignWithToken: return RunPaletteSigning(_chooseCardCertificate);
            case PaletteCommand.SignWithSystemCertificate: return RunPaletteSigning(_chooseComputerCertificate);
            case PaletteCommand.DrawSignature:
                ShowPaletteToolsPage("Sign");
                return ClickIfEnabled(_drawSignatureButton, DrawSignatureButton_Click);
            default: throw new ArgumentOutOfRangeException(nameof(command), command, null);
        }
    }

    private async Task<bool> RunPaletteHistoryAsync(bool undo)
    {
        if (_organizing)
        {
            // Organize's own button, off while its grid is busy, is the gate here.
            if (_organizeBusy || !(undo ? OrganizeUndoButton.IsEnabled : OrganizeRedoButton.IsEnabled)) return false;
            await RunOrganizeHistoryAsync(undo);
            return true;
        }
        if (_isBusy || _dialogOpen || !(undo ? UndoButton.IsEnabled : RedoButton.IsEnabled)) return false;
        await ApplyHistoryAndMetadataAsync(undo);
        return true;
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
    /// gate refused after the screen switch) is left alone and reported.
    /// </summary>
    private bool ArmPaletteToggle(string page, ToggleButton button, RoutedEventHandler click)
    {
        ShowPaletteToolsPage(page);
        if (!button.IsEnabled) return false;
        if (button.IsChecked == true) return true;
        button.IsChecked = true;
        click(button, new RoutedEventArgs());
        return true;
    }

    /// <summary>What a click on <paramref name="button"/> does, unless it is off.</summary>
    private static bool ClickIfEnabled(ButtonBase button, RoutedEventHandler click)
    {
        if (!button.IsEnabled) return false;
        click(button, new RoutedEventArgs());
        return true;
    }

    private bool RunPaletteSigning(Microsoft.UI.Xaml.Controls.Button source)
    {
        ShowPaletteToolsPage("Sign");
        return ClickIfEnabled(source, ChooseSigningCertificate_Click);
    }
}
