using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Pdf.Windows.Facade;
using Pdf.Windows.Viewer;
using Windows.Storage;
using Windows.Storage.Pickers;
using WinRT.Interop;

namespace Pdf.Windows;

/// <summary>
/// Export images: writing chosen pages as PNG or JPEG files into a folder,
/// without replacing the open session.
/// </summary>
/// <remarks>
/// <para>
/// The order is choices, then folder, then files. Every choice is checked by
/// <see cref="PdfDocumentFacade.PlanImageExportAsync"/> while the dialog is
/// still open, so a bad range or an oversized page is fixed where it was
/// typed rather than discovered after a folder has been picked.
/// </para>
/// <para>
/// Rendering and encoding are the core's, not WinRT's: a PNG from this shell
/// and one from the GTK shell come out of the same encoder, and JPEG gets the
/// same flattening onto an opaque background.
/// </para>
/// </remarks>
public sealed partial class MainWindow
{
    private bool _exportingImages;

    private async void ExportImagesButton_Click(object sender, RoutedEventArgs e)
    {
        if (_exportingImages || _isBusy || _dialogOpen || _session is not { PageCount: > 0 } session) return;
        _exportingImages = true;
        try
        {
            SetBusy(true);
            if (!await PrepareDocumentLifecycleAsync()) return;
            var plan = await AskImageExportAsync(session);
            if (plan is null)
            {
                AnnotationStatus.Text = "Export cancelled.";
                return;
            }
            if (_session?.SessionId != session.SessionId) return;
            var picker = new FolderPicker();
            picker.FileTypeFilter.Add("*");
            InitializeWithWindow.Initialize(picker, WindowNative.GetWindowHandle(this));
            var folder = await picker.PickSingleFolderAsync();
            if (folder is null)
            {
                AnnotationStatus.Text = "Export cancelled. No file was written.";
                return;
            }
            if (_session?.SessionId != session.SessionId) return;
            await WriteImagesAsync(session.SessionId, plan, folder);
        }
        catch (Exception error)
        {
            AnnotationStatus.Text = _facade.SaveWriteFailure(error).Error!.Message;
        }
        finally
        {
            _exportingImages = false;
            SetBusy(false);
            RestoreAnnotationControls();
        }
    }

    /// <summary>
    /// Writes each planned page, stopping at the first failure. Stopping is
    /// the point: a missing file in a folder of hundreds goes unnoticed until
    /// someone needs it, so the count of what <em>was</em> written is reported
    /// instead of carrying on.
    /// </summary>
    /// <remarks>
    /// Pages come from the facade's export snapshot, which carries the
    /// annotations the canvas only ever draws as an overlay. It is prepared
    /// once, before the first page, and released however the export ends.
    /// </remarks>
    private async Task WriteImagesAsync(string sessionId, ImageExportPlan plan, StorageFolder folder)
    {
        SetBusy(true);
        var written = 0;
        StorageFile? incomplete = null;
        try
        {
            AnnotationStatus.Text = "Preparing to export...";
            var prepared = await _facade.PrepareImageExportAsync(sessionId);
            if (!prepared.IsSuccess)
            {
                AnnotationStatus.Text = $"Could not prepare the document for export: {prepared.Error!.Message}";
                return;
            }

            foreach (var file in plan.Files)
            {
                AnnotationStatus.Text = $"Exporting page {file.PageIndex + 1} ({written + 1} of {plan.Files.Count})...";
                var image = await _facade.ExportPageImageAsync(sessionId, file.PageIndex, plan.Dpi, plan.Format);
                if (!image.IsSuccess)
                {
                    AnnotationStatus.Text = $"Page {file.PageIndex + 1} could not be exported: {image.Error!.Message} {written} written.";
                    return;
                }

                // Never overwrites: an earlier export into the same folder is
                // the reader's, and a unique name costs nothing.
                incomplete = await folder.CreateFileAsync(file.FileName, CreationCollisionOption.GenerateUniqueName);
                await FileIO.WriteBytesAsync(incomplete, image.Value!);
                incomplete = null;
                written++;
            }

            AnnotationStatus.Text = ImageExportWording.Summary(written, plan.Format, folder.Path);
        }
        catch (Exception error)
        {
            AnnotationStatus.Text = $"Export stopped after {written} written: {_facade.SaveWriteFailure(error).Error!.Message}";
        }
        finally
        {
            if (incomplete is not null)
            {
                try { await incomplete.DeleteAsync(); }
                catch { }
            }
            await _facade.ReleaseImageExportAsync(sessionId);
            SetBusy(false);
            RestoreAnnotationControls();
        }
    }

    /// <summary>
    /// Pages, format and resolution in one dialog. Export stays open on a
    /// refused choice and shows the reason under the fields — the core's own
    /// sentence for a bad range — so the reader corrects it in place.
    /// </summary>
    private async Task<ImageExportPlan?> AskImageExportAsync(DocumentSession session)
    {
        var currentPage = session.PageCount == 0 ? 0 : Math.Min((uint)Math.Max(0, _firstVisiblePage), session.PageCount - 1);

        var pages = new RadioButtons { Header = "Pages" };
        var range = new TextBox { PlaceholderText = "1-3,7", MinWidth = 200, IsEnabled = false, Name = "ExportPageRange" };
        Microsoft.UI.Xaml.Automation.AutomationProperties.SetName(range, "Page range");
        pages.Items.Add(new RadioButton { Content = "All pages", Tag = ImageExportPages.All });
        pages.Items.Add(new RadioButton { Content = "Current page", Tag = ImageExportPages.Current });
        var custom = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
        custom.Children.Add(new TextBlock { Text = "Pages", VerticalAlignment = VerticalAlignment.Center });
        custom.Children.Add(range);
        var customChoice = new RadioButton { Content = custom, Tag = ImageExportPages.Custom };
        Microsoft.UI.Xaml.Automation.AutomationProperties.SetName(customChoice, "Pages");
        pages.Items.Add(customChoice);
        pages.SelectedIndex = 0;
        pages.SelectionChanged += (_, _) =>
        {
            range.IsEnabled = SelectedPages(pages) == ImageExportPages.Custom;
            if (range.IsEnabled) range.Focus(FocusState.Programmatic);
        };

        var format = new ComboBox { Header = "Format", MinWidth = 120 };
        foreach (var choice in Enum.GetValues<ImageExportFormat>())
        {
            format.Items.Add(new ComboBoxItem { Content = ImageExportWording.FormatName(choice), Tag = choice });
        }
        format.SelectedIndex = 0;

        var dpi = new NumberBox
        {
            Header = "Resolution (DPI)",
            Minimum = ImageExportLimits.MinDpi,
            Maximum = ImageExportLimits.MaxDpi,
            Value = ImageExportLimits.DefaultDpi,
            SmallChange = 1,
            LargeChange = 50,
            SpinButtonPlacementMode = NumberBoxSpinButtonPlacementMode.Inline,
            ValidationMode = NumberBoxValidationMode.InvalidInputOverwritten,
        };

        var settings = new StackPanel { Spacing = 12 };
        settings.Children.Add(format);
        settings.Children.Add(dpi);

        var refusal = new TextBlock { Name = "ExportValidationError", TextWrapping = TextWrapping.Wrap, Visibility = Visibility.Collapsed };
        refusal.Foreground = (Microsoft.UI.Xaml.Media.Brush)Application.Current.Resources["SystemFillColorCriticalBrush"];

        var panel = new StackPanel { Spacing = 12, MaxWidth = 420 };
        panel.Children.Add(pages);
        panel.Children.Add(settings);
        panel.Children.Add(refusal);

        var dialog = new ContentDialog
        {
            Title = "Export pages as images",
            Content = panel,
            PrimaryButtonText = "Export",
            CloseButtonText = "Cancel",
            DefaultButton = ContentDialogButton.Primary,
            XamlRoot = Content.XamlRoot,
        };

        ImageExportPlan? plan = null;
        dialog.PrimaryButtonClick += async (_, args) =>
        {
            var deferral = args.GetDeferral();
            try
            {
                // A cleared NumberBox reports NaN; fall back to the default
                // rather than letting the cast invent a resolution.
                var chosenDpi = double.IsNaN(dpi.Value) ? ImageExportLimits.DefaultDpi : (uint)Math.Round(dpi.Value);
                var request = new ImageExportRequest(
                    SelectedPages(pages),
                    range.Text,
                    currentPage,
                    chosenDpi,
                    (format.SelectedItem as ComboBoxItem)?.Tag is ImageExportFormat chosen ? chosen : ImageExportFormat.Png);
                var planned = await _facade.PlanImageExportAsync(session.SessionId, request);
                if (planned.IsSuccess)
                {
                    plan = planned.Value;
                    return;
                }

                refusal.Text = planned.Error!.Message;
                refusal.Visibility = Visibility.Visible;
                args.Cancel = true;
            }
            finally
            {
                deferral.Complete();
            }
        };

        return await ShowModalAsync(dialog) == ContentDialogResult.Primary ? plan : null;
    }

    private static ImageExportPages SelectedPages(RadioButtons pages) =>
        (pages.SelectedItem as RadioButton)?.Tag is ImageExportPages chosen ? chosen : ImageExportPages.All;
}
