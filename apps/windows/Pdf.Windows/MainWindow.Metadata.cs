using Microsoft.UI.Xaml;
using Pdf.Windows.Facade;

namespace Pdf.Windows;

public sealed partial class MainWindow
{
    private async Task RefreshDocumentInfoAsync()
    {
        if (_session is null) return;

        var result = await _facade.DocumentInfoAsync(_session.SessionId);
        if (_session is null || !result.IsSuccess)
        {
            MetadataPermission.Text = result.Error?.Message ?? "";
            return;
        }

        var info = result.Value!;
        MetadataTitle.Text = info.Title ?? "";
        MetadataAuthor.Text = info.Author ?? "";
        MetadataSubject.Text = info.Subject ?? "";
        MetadataKeywords.Text = info.Keywords ?? "";
        MetadataCreator.Text = info.Creator ?? "";
        MetadataProducer.Text = info.Producer ?? "";
        MetadataPermission.Text = _session.ContentEditingAllowed
            ? ""
            : "This document does not permit metadata changes.";
        ApplyMetadataButton.IsEnabled = !_isBusy && _session.ContentEditingAllowed;
    }

    private async void ApplyMetadataButton_Click(object sender, RoutedEventArgs e)
    {
        if (_session is null) return;

        SetBusy(true);
        var result = await _facade.SetDocumentInfoAsync(_session.SessionId, new DocumentInfo(
            MetadataTitle.Text,
            MetadataAuthor.Text,
            MetadataSubject.Text,
            MetadataKeywords.Text,
            MetadataCreator.Text,
            MetadataProducer.Text));
        SetBusy(false);

        if (!result.IsSuccess)
        {
            MetadataPermission.Text = result.Error!.Message;
            return;
        }

        MetadataPermission.Text = "Document properties updated. Changes are pending save.";
        await RefreshAnnotationStateAsync();
    }
}
