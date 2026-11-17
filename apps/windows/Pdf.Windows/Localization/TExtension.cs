using Microsoft.UI.Xaml.Markup;

namespace Pdf.Windows.Localization;

/// <summary>
/// <c>{i18n:T Key=message-id}</c>: a localized string straight from XAML, for
/// any string property (Text, Content, Header, PlaceholderText,
/// ToolTipService.ToolTip, AutomationProperties.Name). Evaluated once when the
/// XAML loads; the shell never switches language at run time.
/// </summary>
/// <remarks>WinUI markup extensions take named arguments only, hence <c>Key=</c>.</remarks>
[MarkupExtensionReturnType(ReturnType = typeof(string))]
public sealed partial class TExtension : MarkupExtension
{
    /// <summary>The Fluent message id, as in apps/windows/locales/en.ftl.</summary>
    public string Key { get; set; } = "";

    protected override object ProvideValue() => I18n.T(Key);
}
