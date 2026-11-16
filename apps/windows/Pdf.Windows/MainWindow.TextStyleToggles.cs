using Microsoft.UI.Text;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Input;
using Pdf.Windows.Facade;
using Pdf.Windows.Viewer;
using Windows.System;
using Windows.UI.Text;

namespace Pdf.Windows;

/// <summary>
/// Bold, italic and underline for existing text: three toggles on the Edit
/// page's Text card, and Ctrl+B / Ctrl+I / Ctrl+U. The Windows port of the GTK
/// shell's <c>content_edit::style_toggles</c>.
/// </summary>
/// <remarks>
/// They act on what content-edit mode is pointed at — the open editor's run or
/// block (only the runs its selection covers whole, when text is selected in
/// it), else the selected block, else every run of a multi-selection — and
/// show the first targeted run's state. What the restyle writes and how it
/// joins the queue are the core's; the toggle rule is the facade's
/// (<see cref="PdfDocumentFacade.ToggleTextStyleAsync"/>).
///
/// One difference from Linux, forced by the live editor: this editor writes as
/// the reader types, and every write names the runs it opened over. A restyle
/// changes those runs, so the editor is committed and opened again over the
/// restyled text — in its new weight and slant, the caret where it was —
/// rather than kept open on a stale snapshot. A restyle of only some of its
/// runs leaves the block selected instead, as on Linux.
/// </remarks>
public sealed partial class MainWindow
{
    private ToggleButton? _boldToggle;
    private ToggleButton? _italicToggle;
    private ToggleButton? _underlineToggle;

    /// <summary>Bumped per read, so a read that lands late never paints over a newer one.</summary>
    private uint _textStyleRequest;

    /// <summary>The target the buttons were last read for, so a caret move re-reads only when it changes it.</summary>
    private string? _textStyleShown;

    private static readonly TextStyleToggle[] TextStyles = [TextStyleToggle.Bold, TextStyleToggle.Italic, TextStyleToggle.Underline];

    private enum TextStyleScope
    {
        /// <summary>A selected block or a multi-selection; no editor is open.</summary>
        Canvas,
        /// <summary>Every run of the open editor.</summary>
        Editor,
        /// <summary>The runs the open editor's selection covers whole.</summary>
        EditorSelection,
        /// <summary>An editor selection a press refuses; the runs are the editor's, for the buttons.</summary>
        Refused,
    }

    private sealed record TextStyleTarget(IReadOnlyList<ContentTextRun> Runs, TextStyleScope Scope, string? Refusal = null)
    {
        public string Key => $"{Scope}:{string.Join(',', Runs.Select(run => $"{run.PageIndex}/{run.Id}"))}";
    }

    /// <summary>The three toggles, in a row of their own, wired to their keys.</summary>
    private UIElement BuildTextStyleToggles()
    {
        var row = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 4 };
        AutomationProperties.SetAutomationId(row, "EditTextStyleToggles");
        _boldToggle = StyleToggle(row, TextStyleToggle.Bold, "B", "Bold", text => text.FontWeight = FontWeights.Bold);
        _italicToggle = StyleToggle(row, TextStyleToggle.Italic, "I", "Italic", text => text.FontStyle = FontStyle.Italic);
        _underlineToggle = StyleToggle(row, TextStyleToggle.Underline, "U", "Underline", text => text.TextDecorations = TextDecorations.Underline);
        WireTextStyleKeys();
        return row;
    }

    private ToggleButton StyleToggle(Panel row, TextStyleToggle style, string letter, string name, Action<TextBlock> dress)
    {
        // Each shows its letter in its own style: no shared icon says "bold".
        var label = new TextBlock { Text = letter };
        dress(label);
        var toggle = new ToggleButton
        {
            Content = label,
            Style = NamedStyle("EditTileToggleStyle"),
            IsEnabled = false,
            // A click must not take the focus from an open editor, or the
            // press would act on nothing — like "Delete text".
            AllowFocusOnInteraction = false,
        };
        AutomationProperties.SetName(toggle, name);
        AutomationProperties.SetAutomationId(toggle, $"EditTextStyle{name}");
        ToolTipService.SetToolTip(toggle, StyleTooltip(style));
        toggle.Click += async (_, _) => await ToggleTextStyleAsync(style);
        row.Children.Add(toggle);
        return toggle;
    }

    private static string StyleTooltip(TextStyleToggle style) => style switch
    {
        TextStyleToggle.Bold => "Make the selected text, or the text being retyped, bold (Ctrl+B)",
        TextStyleToggle.Italic => "Make the selected text, or the text being retyped, italic (Ctrl+I)",
        _ => "Underline the selected text, or the text being retyped (Ctrl+U)",
    };

    private ToggleButton? ToggleFor(TextStyleToggle style) => style switch
    {
        TextStyleToggle.Bold => _boldToggle,
        TextStyleToggle.Italic => _italicToggle,
        _ => _underlineToggle,
    };

    /// <summary>
    /// Ctrl+B / Ctrl+I / Ctrl+U, hung on the page view for the reason Delete
    /// is (<see cref="WireTextBlockKeys"/>). They stand down for any text
    /// field but the open retype editor, and when there is nothing to restyle.
    /// </summary>
    private void WireTextStyleKeys()
    {
        foreach (var (style, key) in new[] { (TextStyleToggle.Bold, VirtualKey.B), (TextStyleToggle.Italic, VirtualKey.I), (TextStyleToggle.Underline, VirtualKey.U) })
        {
            var accelerator = new KeyboardAccelerator { Key = key, Modifiers = VirtualKeyModifiers.Control };
            accelerator.Invoked += async (_, args) =>
            {
                if ((TextInputHasFocus() && !RetypeEditorHasFocus()) || _isBusy || CurrentTextStyleTarget() is null)
                {
                    args.Handled = false;
                    return;
                }
                args.Handled = true;
                await ToggleTextStyleAsync(style);
            };
            PageScroller.KeyboardAccelerators.Add(accelerator);
        }
    }

    private bool RetypeEditorHasFocus() =>
        _xamlRoot is { } root && _pump.Box is { } editor && ReferenceEquals(FocusManager.GetFocusedElement(root), editor.Box);

    /// <summary>What a toggle acts on now, or <c>null</c> when content-edit mode may restyle nothing.</summary>
    private TextStyleTarget? CurrentTextStyleTarget()
    {
        if (_session is not { ContentEditingAllowed: true } || !_contentEditMode || _organizing) return null;
        if (_pump.Box is { } editor)
        {
            IReadOnlyList<ContentTextRun> runs = editor.Block?.Runs ?? [editor.Run];
            var selected = EditorRunSelection.Of(editor.Box.Text, editor.OpenedWith, [.. runs.Select(run => run.Text)],
                editor.Box.SelectionStart, editor.Box.SelectionLength);
            return selected.Kind switch
            {
                EditorRunSelectionKind.Whole => new(runs, TextStyleScope.Editor),
                EditorRunSelectionKind.Runs => new([.. selected.Runs.Select(index => runs[index])], TextStyleScope.EditorSelection),
                _ => new(runs, TextStyleScope.Refused, selected.Refusal),
            };
        }
        if (_selectedTextBlock is { } block) return new(block.Runs, TextStyleScope.Canvas);
        return _contentSelection is { } selection && selection.Blocks.Count > 0
            ? new([.. selection.Blocks.SelectMany(member => member.Runs)], TextStyleScope.Canvas)
            : null;
    }

    /// <summary>
    /// Shows the target's state on the buttons: the first run's look, a
    /// style that is its own font with no plain sibling pressed and disabled.
    /// With <paramref name="onlyIfTargetChanged"/>, as on a caret move, the
    /// core is asked again only when the move changed the target.
    /// </summary>
    private async void UpdateTextStyleToggles(bool onlyIfTargetChanged = false)
    {
        if (_boldToggle is null) return;
        var target = CurrentTextStyleTarget();
        if (onlyIfTargetChanged && target?.Key == _textStyleShown) return;
        _textStyleShown = target?.Key;
        var request = ++_textStyleRequest;
        TextStyleButtons? buttons = null;
        if (target is not null && _session is { } session && ContentEditButton.IsEnabled && !_isBusy)
        {
            var read = await _facade.TextStyleButtonsAsync(session.SessionId, target.Runs);
            if (request != _textStyleRequest) return;
            buttons = read.IsSuccess ? read.Value : null;
        }

        foreach (var style in TextStyles)
        {
            var toggle = ToggleFor(style)!;
            var isFixed = buttons?.IsFixed(style) ?? false;
            toggle.IsChecked = buttons?.Look.Has(style) ?? false;
            toggle.IsEnabled = buttons is not null && !isFixed;
            ToolTipService.SetToolTip(toggle, isFixed ? TextStyleButtons.FixedReason(style) : StyleTooltip(style));
        }
    }

    private void ContentEditor_SelectionChanged(object sender, RoutedEventArgs args) => UpdateTextStyleToggles(onlyIfTargetChanged: true);

    /// <summary>
    /// Presses <paramref name="style"/> for the current target and records the
    /// restyle, one undo step whatever the run count.
    /// </summary>
    private async Task ToggleTextStyleAsync(TextStyleToggle style)
    {
        if (_session is null || _isBusy || CurrentTextStyleTarget() is not { } target)
        {
            UpdateTextStyleToggles();
            return;
        }
        if (target.Refusal is { } refusal)
        {
            AnnotationStatus.Text = refusal;
            UpdateTextStyleToggles();
            return;
        }

        var sessionId = _session.SessionId;
        SetBusy(true);
        try
        {
            if (target.Scope == TextStyleScope.Canvas)
            {
                await RestyleOnCanvasAsync(sessionId, target, style);
            }
            else if (_pump.Box is { } editor)
            {
                await RestyleInEditorAsync(sessionId, editor, target, style);
            }
        }
        finally
        {
            SetBusy(false);
            UpdateEditTextSelection();
        }
    }

    private const string TextRestyledStatus = "Text restyled. Save to keep the change.";

    /// <summary>A selected block or a multi-selection: restyled, and still selected once the page is read again.</summary>
    private async Task RestyleOnCanvasAsync(string sessionId, TextStyleTarget target, TextStyleToggle style)
    {
        var selection = _contentSelection;
        var block = _selectedTextBlock;
        var result = await _facade.ToggleTextStyleAsync(sessionId, target.Runs, style);
        if (_session?.SessionId != sessionId) return;
        if (selection is not null)
        {
            await AfterContentSelectionEditAsync(selection, result, TextRestyledStatus, keep: true);
            return;
        }

        var page = target.Runs[0].PageIndex;
        await AfterTextBlockEditAsync(page, result, TextRestyledStatus);
        // Selected again from the new read, so a second toggle stacks on this
        // one without a reselect.
        if (result.IsSuccess && block is not null) ReselectRestyledBlock(page, block.Runs, block.Bounds);
    }

    /// <summary>
    /// The open editor: its typed text reaches the document first, the runs
    /// are read again as they stand now, and once restyled the editor is
    /// opened again over them — or, for some of its runs only, closed with
    /// the block left selected.
    /// </summary>
    private async Task RestyleInEditorAsync(string sessionId, ContentEditor editor, TextStyleTarget target, TextStyleToggle style)
    {
        var page = editor.PageIndex;
        var (caret, length) = (editor.Box.SelectionStart, editor.Box.SelectionLength);
        var blockRuns = editor.Block?.Runs ?? [editor.Run];
        var blockBounds = BoundsOf(editor.Run);
        await _pump.DrainAsync();
        if (_session?.SessionId != sessionId || !ReferenceEquals(_pump.Box, editor)) return;

        // A block retyped as the reader typed is one run now; the others are
        // gone, and the one left reads its new text.
        await RereadPageContentAsync(page);
        var ids = target.Runs.Select(run => run.Id).ToHashSet();
        ContentTextRun[] runs = _pageContent.TryGetValue(page, out var state) && state.Content is { } content
            ? [.. content.TextRuns.Where(run => ids.Contains(run.Id))]
            : [];
        if (runs.Length == 0)
        {
            AnnotationStatus.Text = "This text is no longer on the page.";
            return;
        }

        var result = await _facade.ToggleTextStyleAsync(sessionId, runs, style);
        if (_session?.SessionId != sessionId) return;
        if (!result.IsSuccess)
        {
            // Nothing changed, so the editor stays open as it was.
            AnnotationStatus.Text = result.Error!.Message;
            return;
        }

        // Every write was drained above, so this only takes the box off.
        await _pump.CommitAsync();
        await AfterTextBlockEditAsync(page, result, TextRestyledStatus);
        ReselectRestyledBlock(page, blockRuns, blockBounds);
        if (target.Scope != TextStyleScope.Editor || _selectedTextBlock is not { } restyled
            || restyled.First.RequiresFontSubstitution)
        {
            return;
        }

        // Opened again over the restyled block, in its new face, the caret or
        // selection where the reader left it.
        await RetypeSelectedTextBlockAsync();
        if (_pump.Box is { } reopened && reopened.PageIndex == page)
        {
            var start = Math.Clamp(caret, 0, reopened.Box.Text.Length);
            reopened.Box.Select(start, Math.Clamp(length, 0, reopened.Box.Text.Length - start));
            AnnotationStatus.Text = TextRestyledStatus;
        }
    }

    /// <summary>
    /// Selects the block that now holds one of <paramref name="runs"/>, or the
    /// one under the middle of <paramref name="bounds"/> when the read gave
    /// them new identities.
    /// </summary>
    private void ReselectRestyledBlock(uint page, IReadOnlyList<ContentTextRun> runs, AnnotationRect bounds)
    {
        if (!_pageContent.TryGetValue(page, out var state) || state.Content is not { } content) return;
        var ids = runs.Select(run => run.Id).ToHashSet();
        var (x, y) = (bounds.X + bounds.Width / 2, bounds.Y + bounds.Height / 2);
        var block = content.TextBlocks.FirstOrDefault(candidate => candidate.Runs.Any(run => ids.Contains(run.Id)))
            ?? (ContentHitTest.TextRunAt(content.TextRuns, BoundsOf, x, y) is { } hit ? content.BlockOf(hit) : null);
        if (block is null) return;
        _selectedTextBlock = block;
        DrawContentOutlines(page);
        UpdateEditTextSelection();
    }

    /// <summary>
    /// Dresses an editor opened over existing text in the weight and slant the
    /// core reads for it — a synthesized bold or a sibling face included —
    /// rather than only what its font's name suggests.
    /// </summary>
    private async void DressEditorFromLook(ContentEditor editor)
    {
        if (_session is null) return;
        IReadOnlyList<ContentTextRun> runs = editor.Block?.Runs ?? [editor.Run];
        if (editor.Run.RequiresFontSubstitution) return;
        var read = await _facade.TextStyleButtonsAsync(_session.SessionId, [runs[0]]);
        if (!ReferenceEquals(_pump.Box, editor) || read.Value is not { } buttons) return;
        editor.Box.FontWeight = buttons.Look.Bold ? FontWeights.Bold : FontWeights.Normal;
        editor.Box.FontStyle = buttons.Look.Italic ? FontStyle.Italic : FontStyle.Normal;
        if (editor.PageIndex < _slots.Count) PlaceEditor(_slots[(int)editor.PageIndex], editor.PageIndex, editor);
    }
}
