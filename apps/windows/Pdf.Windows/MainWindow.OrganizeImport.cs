using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Pdf.Windows.Facade;
using Windows.Security.Cryptography;
using Windows.Storage;
using Windows.Storage.Pickers;
using WinRT.Interop;

namespace Pdf.Windows;

/// <summary>
/// "Add PDFs" in Organize: appends every page of each picked PDF, the Linux
/// shell's organize/import.rs reached through the facade.
/// </summary>
/// <remarks>
/// Each file is one <see cref="PdfDocumentFacade.ImportPdfAsync"/> call and so
/// one undo step — the shape the FFI gives every shell, where Linux folds a
/// whole pick into one. Cancel stops before the next file; files already added
/// stay, and undo takes them back out one at a time. A locked file asks for
/// its own password; any other refusal stops the rest, as on Linux.
/// </remarks>
public sealed partial class MainWindow
{
    private const string ImportCancelled = "PDF import cancelled.";

    private bool _importCancelRequested;
    /// <summary>The picked file name behind each imported block, for the session that imported it.</summary>
    private readonly Dictionary<ulong, string> _importedNames = [];
    private string? _importedNamesSessionId;

    private async void OrganizeAddPdfsButton_Click(object sender, RoutedEventArgs e)
    {
        if (!_organizing || _session is not { } session || _isBusy || _organizeBusy || _dialogOpen) return;
        // Two of the core's three gates are already on the session; asking
        // here spares the reader a file pick that could only be refused.
        if (!session.ContentEditingAllowed)
        {
            AnnotationStatus.Text = "This document does not allow adding pages.";
            return;
        }

        var picker = new FileOpenPicker();
        picker.FileTypeFilter.Add(".pdf");
        InitializeWithWindow.Initialize(picker, WindowNative.GetWindowHandle(this));
        var files = await picker.PickMultipleFilesAsync();
        if (files is null || files.Count == 0 || _session?.SessionId != session.SessionId || !_organizing)
        {
            AnnotationStatus.Text = ImportCancelled;
            return;
        }

        await ImportPdfsAsync(session.SessionId, files);
    }

    private void CancelImportButton_Click(object sender, RoutedEventArgs e)
    {
        _importCancelRequested = true;
        CancelImportButton.IsEnabled = false;
        AnnotationStatus.Text = "Cancelling after the current PDF...";
    }

    private async Task ImportPdfsAsync(string sessionId, IReadOnlyList<StorageFile> files)
    {
        _organizeBusy = true;
        _importCancelRequested = false;
        SetBusy(true);
        UpdateOrganizeHeader();
        // Before the first import: it rebuilds the preview, and a thumbnail
        // asked of the old layout must not land after it.
        _thumbnailGeneration++;
        ClearOrganizePageDrag();
        SetImportProgress(0, files.Count);

        var importedFiles = 0;
        uint importedPages = 0;
        var touched = false;
        var warnings = new List<string>();
        string? stopped = null;
        try
        {
            foreach (var file in files)
            {
                if (_importCancelRequested) { stopped = ImportCancelled; break; }
                byte[] bytes;
                try
                {
                    var buffer = await FileIO.ReadBufferAsync(file);
                    CryptographicBuffer.CopyToByteArray(buffer, out bytes);
                }
                catch (Exception)
                {
                    stopped = $"Could not import {file.Name}: the file could not be read.";
                    break;
                }

                var result = await ImportOneAsync(sessionId, file.Name, bytes);
                if (_session?.SessionId != sessionId) return;
                if (result is null) { stopped = ImportCancelled; break; }
                touched = true;
                if (!result.IsSuccess)
                {
                    stopped = $"Could not import {file.Name}: {result.Error!.Message}";
                    break;
                }

                var imported = result.Value!;
                // The next file appends after this one, so its index must
                // come from the layout this import left, not the one before.
                _session = imported.Session;
                RememberImportedName(sessionId, imported.SourceId, file.Name);
                warnings.AddRange(imported.Warnings.Select(warning => $"{file.Name}: {warning}"));
                importedFiles++;
                importedPages += imported.PageCount;
                SetImportProgress(importedFiles, files.Count);
            }
        }
        finally
        {
            ImportProgressPanel.Visibility = Visibility.Collapsed;
            _organizeBusy = false;
            if (!_windowClosed)
            {
                SetBusy(false);
                UpdateAnnotationControls(_annotationState);
                UpdateOrganizeHeader();
            }
        }

        if (touched && _session?.SessionId == sessionId)
        {
            // Re-read rather than trust the last result: a failed preview
            // rebuild still left its file's pages in the document.
            var current = await _facade.SessionAsync(sessionId);
            if (current.IsSuccess) _session = current.Value!;
            _pagesEdited = true;
            _viewerStale = true;
            RefreshSessionCommands();
            if (_organizing) BuildOrganizeCards();
            await RefreshAnnotationStateAsync();
        }

        var done = importedFiles == 0 ? "" : $"Imported {importedPages} {(importedPages == 1 ? "page" : "pages")} from "
            + $"{importedFiles} {(importedFiles == 1 ? "PDF" : "PDFs")}. Changes are pending save.";
        AnnotationStatus.Text = stopped is null ? done : $"{stopped} {done}".Trim();
        if (warnings.Count > 0) await ShowImportWarningsAsync(warnings);
    }

    /// <summary>
    /// Imports one file, asking for its password as often as it takes, or
    /// <c>null</c> when the reader cancelled.
    /// </summary>
    private async Task<OperationResult<ImportedPdf>?> ImportOneAsync(string sessionId, string name, byte[] bytes)
    {
        string? password = null;
        while (true)
        {
            var result = await _facade.ImportPdfAsync(sessionId, bytes, password, _session!.PageCount);
            if (result.IsSuccess || !result.Error!.RequiresPassword) return result;
            if (_importCancelRequested || _session?.SessionId != sessionId) return null;
            password = await AskImportPasswordAsync(name, retry: password is not null);
            if (password is null) return null;
        }
    }

    private async Task<string?> AskImportPasswordAsync(string name, bool retry)
    {
        var passwordBox = new PasswordBox { PlaceholderText = "Password", PasswordRevealMode = PasswordRevealMode.Peek };
        AutomationProperties.SetName(passwordBox, $"Password for {name}");
        var panel = new StackPanel { Spacing = 8 };
        panel.Children.Add(new TextBlock { Text = $"Enter the password for {name}.", TextWrapping = TextWrapping.Wrap });
        panel.Children.Add(passwordBox);
        if (retry)
            panel.Children.Add(new TextBlock { Text = "The password is incorrect. Try again.", TextWrapping = TextWrapping.Wrap,
                Foreground = new SolidColorBrush(Microsoft.UI.Colors.Crimson) });
        var dialog = new ContentDialog
        {
            Title = "Password required",
            Content = panel,
            PrimaryButtonText = "Unlock",
            CloseButtonText = "Cancel import",
            DefaultButton = ContentDialogButton.Primary,
            XamlRoot = Content.XamlRoot,
        };
        dialog.Opened += (_, _) => passwordBox.Focus(FocusState.Programmatic);
        AnnotationStatus.Text = $"Waiting for the password for {name}.";
        try
        {
            return await ShowModalAsync(dialog) == ContentDialogResult.Primary ? passwordBox.Password : null;
        }
        finally
        {
            passwordBox.Password = "";
        }
    }

    private async Task ShowImportWarningsAsync(IReadOnlyList<string> warnings)
    {
        var text = new TextBlock { Text = string.Join("\n", warnings), TextWrapping = TextWrapping.Wrap, IsTextSelectionEnabled = true };
        var panel = new StackPanel { Spacing = 8 };
        panel.Children.Add(new ScrollViewer { Content = text, MaxHeight = 320 });
        panel.Children.Add(new TextBlock { Text = "The pages were added. Undo takes an imported PDF back out.", TextWrapping = TextWrapping.Wrap,
            Foreground = (Brush)Application.Current.Resources["TextFillColorSecondaryBrush"] });
        await ShowModalAsync(new ContentDialog
        {
            Title = "Some document-level information stayed behind",
            Content = panel,
            CloseButtonText = "OK",
            XamlRoot = Content.XamlRoot,
        });
    }

    private void SetImportProgress(int completed, int total)
    {
        ImportProgressPanel.Visibility = Visibility.Visible;
        CancelImportButton.IsEnabled = !_importCancelRequested;
        ImportProgressBar.Maximum = Math.Max(total, 1);
        ImportProgressBar.Value = completed;
        ImportProgressText.Text = $"{completed} of {total} PDFs";
    }

    private void RememberImportedName(string sessionId, ulong sourceId, string name)
    {
        if (_importedNamesSessionId != sessionId)
        {
            _importedNames.Clear();
            _importedNamesSessionId = sessionId;
        }
        _importedNames[sourceId] = name;
    }

    /// <summary>The file an imported block came from, when this session imported it.</summary>
    private string ImportedBlockName(string sessionId, ulong? sourceId) =>
        _importedNamesSessionId == sessionId && sourceId is { } id && _importedNames.TryGetValue(id, out var name) ? name : "Imported PDF";
}
