using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Pdf.Windows.Facade;
using Windows.System;

namespace Pdf.Windows;

/// <summary>Document properties presentation; the facade serializes edits against the effective Info dictionary.</summary>
public sealed partial class MainWindow
{
    private bool _metadataSyncing = true;
    private string? _metadataSessionId;
    private long _metadataVersion;

    private TextBox[] MetadataEntries => [MetadataTitle, MetadataAuthor, MetadataSubject, MetadataKeywords,
        MetadataCreator, MetadataProducer, MetadataCreated, MetadataModified];

    private void UpdateMetadataControls()
    {
        var enabled = !_isBusy && _session?.ContentEditingAllowed == true && _session.SessionId == _metadataSessionId;
        foreach (var entry in MetadataEntries) entry.IsEnabled = enabled;
        MetadataPages.Text = _session is null ? "Pages: —" : $"Pages: {_session.PageCount}";
    }

    private async Task RefreshDocumentInfoAsync()
    {
        var sessionId = _session?.SessionId;
        var version = ++_metadataVersion;
        if (sessionId is null)
        {
            _metadataSessionId = null;
            _metadataSyncing = true;
            foreach (var entry in MetadataEntries) entry.Text = "";
            _metadataSyncing = false;
            MetadataPermission.Text = "Open a PDF before editing its properties.";
            UpdateMetadataControls();
            return;
        }

        var result = await _facade.DocumentInfoAsync(sessionId);
        if (_session?.SessionId != sessionId || version != _metadataVersion) return;
        if (!result.IsSuccess)
        {
            _metadataSessionId = null;
            MetadataPermission.Text = result.Error!.Message;
            UpdateMetadataControls();
            return;
        }

        _metadataSyncing = true;
        try
        {
            var info = result.Value!;
            MetadataTitle.Text = info.Title ?? "";
            MetadataAuthor.Text = info.Author ?? "";
            MetadataSubject.Text = info.Subject ?? "";
            MetadataKeywords.Text = info.Keywords ?? "";
            MetadataCreator.Text = info.Creator ?? "";
            MetadataProducer.Text = info.Producer ?? "";
            MetadataCreated.Text = MetadataDateText.Format(info.CreationDate);
            MetadataModified.Text = MetadataDateText.Format(info.ModDate);
            _metadataSessionId = sessionId;
            MetadataPermission.Text = _session.ContentEditingAllowed ? "" : "This document does not permit metadata changes.";
        }
        finally { _metadataSyncing = false; }
        UpdateMetadataControls();
    }

    private async void MetadataText_TextChanged(object sender, TextChangedEventArgs e)
    {
        if (_metadataSyncing || _isBusy || _session?.ContentEditingAllowed != true || sender is not TextBox entry || !entry.IsEnabled) return;
        var sessionId = _session.SessionId;
        var version = ++_metadataVersion;
        var property = Enum.Parse<DocumentProperty>((string)entry.Tag);
        var result = await _facade.SetDocumentPropertyAsync(sessionId, property, entry.Text);
        if (_session?.SessionId != sessionId || version != _metadataVersion) return;
        ReportMetadataResult(result);
        if (!result.IsSuccess) await RefreshDocumentInfoAsync();
        // Do not write text back on success: it would reset the active caret.
        await RefreshAnnotationStateAsync();
    }

    private async void MetadataDate_KeyDown(object sender, KeyRoutedEventArgs e)
    {
        if (e.Key != VirtualKey.Enter) return;
        e.Handled = true;
        await CommitMetadataDateAsync((TextBox)sender);
    }

    private async void MetadataDate_LostFocus(object sender, RoutedEventArgs e) => await CommitMetadataDateAsync((TextBox)sender);

    private async Task CommitMetadataDateAsync(TextBox entry)
    {
        if (_metadataSyncing || _isBusy || _session?.ContentEditingAllowed != true || !entry.IsEnabled) return;
        var sessionId = _session.SessionId;
        var version = ++_metadataVersion;
        var property = Enum.Parse<DocumentDateProperty>((string)entry.Tag);
        var result = await _facade.SetDocumentDateAsync(sessionId, property, entry.Text);
        if (_session?.SessionId != sessionId || version != _metadataVersion) return;
        ReportMetadataResult(result);
        // Restore invalid input or show the normalized accepted date. Leave other active entries alone.
        var current = result.IsSuccess ? result : await _facade.DocumentInfoAsync(sessionId);
        if (_session?.SessionId != sessionId || version != _metadataVersion || !current.IsSuccess) return;
        entry.Text = MetadataDateText.Format(property == DocumentDateProperty.Created ? current.Value!.CreationDate : current.Value!.ModDate);
        await RefreshAnnotationStateAsync();
    }

    private void ReportMetadataResult(OperationResult<DocumentInfo> result)
    {
        AnnotationStatus.Text = result.IsSuccess ? "Document properties updated. Changes are pending save." : result.Error!.Message;
    }
}
