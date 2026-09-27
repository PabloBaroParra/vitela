namespace Pdf.Windows.Viewer;

/// <summary>
/// Translates line breaks between a WinUI <c>TextBox</c> and a text field's
/// value. The TextBox stores every break as <c>"\r"</c>; <c>pdf-form</c>
/// wraps a multiline field's paragraphs on <c>"\n"</c> alone, so a break sent
/// through unconverted would reach the appearance stream as a stray control
/// character instead of a new line.
/// </summary>
internal static class FormFieldText
{
    public static string FromTextBox(string text) => text.Replace("\r\n", "\n").Replace('\r', '\n');

    public static string ToTextBox(string value) => value.Replace("\r\n", "\n").Replace('\n', '\r');
}
