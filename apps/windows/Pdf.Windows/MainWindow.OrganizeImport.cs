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
/// Prepare each source without editing the document, then apply the whole pick
/// as one undo step. Cancellation drops the prepared sources. Warnings require
/// confirmation before applying; a locked file asks for its own password.
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
        _organizeBusy = true;
        SetBusy(true);
        UpdateOrganizeHeader();
        IReadOnlyList<StorageFile>? files = null;
        try
        {
            var refusal = await _facade.ImportRefusalAsync(session.SessionId);
            if (!refusal.IsSuccess || refusal.Value is not null)
            {
                AnnotationStatus.Text = refusal.Error?.Message ?? refusal.Value!;
                return;
            }
            var picker = new FileOpenPicker();
            picker.FileTypeFilter.Add(".pdf");
            InitializeWithWindow.Initialize(picker, WindowNative.GetWindowHandle(this));
            files = await picker.PickMultipleFilesAsync();
        }
        catch (Exception)
        {
            AnnotationStatus.Text = "Could not select PDFs to import.";
            return;
        }
        finally
        {
            _organizeBusy = false;
            if (!_windowClosed)
            {
                SetBusy(false);
                UpdateAnnotationControls(_annotationState);
                UpdateOrganizeHeader();
            }
        }
        if (_windowClosed) return;
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
        SetImportProgress(0, files.Count);
        AnnotationStatus.Text = "Checking selected PDFs...";
        var sources = new List<IImportSource>();
        var warnings = new List<string>();
        try
        {
            if (files.Count == 0) { AnnotationStatus.Text = ImportCancelled; return; }
            foreach (var file in files)
            {
                if (!ImportIsCurrent(sessionId)) { AnnotationStatus.Text = ImportCancelled; return; }
                byte[] bytes;
                try
                {
                    var buffer = await FileIO.ReadBufferAsync(file);
                    CryptographicBuffer.CopyToByteArray(buffer, out bytes);
                }
                catch (Exception)
                {
                    AnnotationStatus.Text = $"Could not import {file.Name}: the file could not be read.";
                    return;
                }
                if (!ImportIsCurrent(sessionId)) { AnnotationStatus.Text = ImportCancelled; return; }
                var result = await PrepareImportAsync(sessionId, file.Name, bytes);
                if (result is null) { AnnotationStatus.Text = ImportCancelled; return; }
                if (!result.IsSuccess)
                {
                    AnnotationStatus.Text = $"Could not import {file.Name}: {result.Error!.Message}";
                    return;
                }
                sources.Add(result.Value!);
                if (!ImportIsCurrent(sessionId)) { AnnotationStatus.Text = ImportCancelled; return; }
                warnings.AddRange(result.Value!.Warnings.Select(warning => $"{file.Name}: {warning}"));
                SetImportProgress(sources.Count, files.Count);
            }
            if (warnings.Count > 0 && !await ConfirmImportWarningsAsync(warnings))
            {
                AnnotationStatus.Text = ImportCancelled;
                return;
            }
            if (!ImportIsCurrent(sessionId)) { AnnotationStatus.Text = ImportCancelled; return; }

            // Once apply begins it cannot be cancelled: the core commits one
            // atomic command, then the facade rebuilds its preview.
            CancelImportButton.IsEnabled = false;
            AnnotationStatus.Text = "Adding selected PDFs...";
            _thumbnailGeneration++;
            ClearOrganizePageDrag();
            var previousPageCount = _session!.PageCount;
            var imported = await _facade.ImportPreparedAsync(sessionId, sources, previousPageCount);
            if (_session?.SessionId != sessionId || _windowClosed) return;
            if (imported.IsSuccess)
            {
                _session = imported.Value!.Session;
                foreach (var (sourceId, file) in imported.Value.SourceIds.Zip(files))
                    RememberImportedName(sessionId, sourceId, file.Name);
            }
            else
            {
                // Preview failure can follow a successful core commit. Query
                // the session so those pages and their undo remain visible.
                var current = await _facade.SessionAsync(sessionId);
                if (current.IsSuccess) _session = current.Value!;
            }
            if (_session.PageCount != previousPageCount)
            {
                _pagesEdited = true;
                _viewerStale = true;
                RefreshSessionCommands();
                if (_organizing) BuildOrganizeCards();
                await RefreshAnnotationStateAsync();
            }
            AnnotationStatus.Text = imported.IsSuccess
                ? $"Imported {imported.Value!.PageCount} {(imported.Value.PageCount == 1 ? "page" : "pages")} from "
                    + $"{files.Count} {(files.Count == 1 ? "PDF" : "PDFs")}. Changes are pending save."
                : imported.Error!.Message;
        }
        catch (Exception)
        {
            AnnotationStatus.Text = "Could not import the selected PDFs.";
        }
        finally
        {
            foreach (var source in sources) source.Dispose();
            ImportProgressPanel.Visibility = Visibility.Collapsed;
            _organizeBusy = false;
            if (!_windowClosed)
            {
                SetBusy(false);
                UpdateAnnotationControls(_annotationState);
                UpdateOrganizeHeader();
            }
        }
    }

    private bool ImportIsCurrent(string sessionId) =>
        !_importCancelRequested && !_windowClosed && _organizing && _session?.SessionId == sessionId;

    /// <summary>
    /// Prepares one file, asking for its password as often as it takes, or
    /// <c>null</c> when the reader cancelled.
    /// </summary>
    private async Task<OperationResult<IImportSource>?> PrepareImportAsync(string sessionId, string name, byte[] bytes)
    {
        string? password = null;
        while (true)
        {
            if (!ImportIsCurrent(sessionId)) return null;
            var result = await _facade.PrepareImportAsync(bytes, password);
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

    private async Task<bool> ConfirmImportWarningsAsync(IReadOnlyList<string> warnings)
    {
        var text = new TextBlock { Text = string.Join("\n", warnings), TextWrapping = TextWrapping.Wrap, IsTextSelectionEnabled = true };
        var panel = new StackPanel { Spacing = 8 };
        panel.Children.Add(new ScrollViewer { Content = text, MaxHeight = 320 });
        panel.Children.Add(new TextBlock { Text = "No pages have been added. Import anyway to add the whole batch as one undo step.", TextWrapping = TextWrapping.Wrap,
            Foreground = (Brush)Application.Current.Resources["TextFillColorSecondaryBrush"] });
        return await ShowModalAsync(new ContentDialog
        {
            Title = "Some document-level information will stay behind",
            Content = panel,
            PrimaryButtonText = "Import anyway",
            CloseButtonText = "Cancel",
            DefaultButton = ContentDialogButton.Close,
            XamlRoot = Content.XamlRoot,
        }) == ContentDialogResult.Primary;
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
