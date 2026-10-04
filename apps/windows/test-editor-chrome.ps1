param()

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
$processes = @(Get-Process Pdf.Windows -ErrorAction SilentlyContinue)
if ($processes.Count -ne 1) { throw 'Start exactly one Pdf.Windows instance with the built-in sample open.' }
$window = [System.Windows.Automation.AutomationElement]::FromHandle($processes[0].MainWindowHandle)

function Find-Element([string]$Id) {
    return $window.FindFirst([System.Windows.Automation.TreeScope]::Descendants,
        [System.Windows.Automation.PropertyCondition]::new(
            [System.Windows.Automation.AutomationElement]::AutomationIdProperty, $Id))
}

$returnButton = Find-Element 'ReturnToDocumentButton'
if ($null -ne $returnButton -and -not $returnButton.Current.IsOffscreen) {
    $returnButton.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
    Start-Sleep -Milliseconds 500
}
$title = Find-Element 'DocumentTitle'
if ($null -eq $title -or $title.Current.IsOffscreen -or $title.Current.Name -ne 'Vitela sample.pdf') {
    throw 'This smoke test requires the built-in sample, not a user document.'
}

foreach ($entry in @(
    @('OpenButton', 'Open PDF'), @('OpenSampleButton', 'Open sample'),
    @('SaveButton', 'Save as'), @('PrintButton', 'Print'), @('ExportImagesButton', 'Export images'),
    @('UndoButton', 'Undo'), @('RedoButton', 'Redo'),
    @('ZoomOutButton', 'Zoom out'), @('ZoomInButton', 'Zoom in'),
    @('FitWidthButton', 'Fit width'), @('FitPageButton', 'Fit page'),
    @('PagesPanelButton', 'Pages'), @('ToolsPanelButton', 'Tools'),
    @('FindDocumentButton', 'Find in document'))) {
    $element = Find-Element $entry[0]
    if ($null -eq $element -or $element.Current.IsOffscreen -or $element.Current.Name -ne $entry[1]) {
        throw "Missing or inaccessible toolbar command: $($entry[0])"
    }
}
if ((Find-Element 'UndoButton').Current.IsEnabled -or (Find-Element 'RedoButton').Current.IsEnabled) {
    throw 'History commands must be disabled for an unchanged sample.'
}
'PASS Compact toolbar accessible names, visibility and empty-history gates'

function Read-Text([string]$Id) {
    (Find-Element $Id).GetCurrentPattern([System.Windows.Automation.TextPattern]::Pattern).DocumentRange.GetText(-1)
}
# Linux's editor_toolbar.rs readouts: "current / total" and a bare percentage.
if ((Read-Text 'PageCounter') -notmatch '^\d+ / \d+$') { throw "Page indicator must read 'N / M': $(Read-Text 'PageCounter')" }
if ((Read-Text 'ZoomLevel') -notmatch '^\d+%$') { throw "Zoom indicator must read 'N%': $(Read-Text 'ZoomLevel')" }
'PASS Page and zoom indicators use the Linux readout'

$keys = @('Annotate', 'Edit', 'Comments', 'Sign')
foreach ($key in $keys) {
    $tab = Find-Element "ToolsTab_$key"
    if ($null -eq $tab -or $tab.Current.IsOffscreen) { throw "Missing tools tab: $key" }
    # Repeat the active-tab click: it must never leave all tabs unchecked.
    for ($click = 0; $click -lt 2; $click++) {
        $tab.GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern).Toggle()
        Start-Sleep -Milliseconds 150
        foreach ($other in $keys) {
            $pattern = (Find-Element "ToolsTab_$other").GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern)
            $expected = if ($other -eq $key) { [System.Windows.Automation.ToggleState]::On }
                else { [System.Windows.Automation.ToggleState]::Off }
            if ($pattern.Current.ToggleState -ne $expected) { throw "Tools tab selection disagrees at $key / $other" }
        }
        $comments = Find-Element 'CommentsPlaceholder'
        if ($key -eq 'Comments') {
            if ($null -eq $comments -or $comments.Current.IsOffscreen -or $comments.Current.Name -ne "Comments aren't available in this shell yet.") {
                throw 'Comments tab must explicitly explain that comments are unavailable.'
            }
        } elseif ($null -ne $comments -and -not $comments.Current.IsOffscreen) {
            throw 'Comments placeholder leaked into another tools page.'
        }
    }
}
(Find-Element 'ToolsTab_Annotate').GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern).Toggle()
'PASS All four tools tabs and active-tab reassertion'

(Find-Element 'ToolsTab_Edit').GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern).Toggle()
foreach ($entry in @(@('EditPanelHeading', 'Edit PDF'), @('EditTextCardHeading', 'Text'), @('EditImagesCardHeading', 'Images'),
    @('ContentEditButton', 'Edit content'), @('InsertTextButton', 'Insert text'), @('DeleteTextButton', 'Delete text'),
    @('InsertImageButton', 'Insert image'), @('ReplaceImageButton', 'Replace image'), @('DeleteImageButton', 'Delete image'))) {
    $element = Find-Element $entry[0]
    if ($null -eq $element -or $element.Current.IsOffscreen -or $element.Current.Name -ne $entry[1]) {
        throw "Missing Edit page heading/card/command: $($entry[0])"
    }
}
$notice = Find-Element 'EditAvailability'
if ($null -ne $notice -and -not $notice.Current.IsOffscreen) { throw 'Editable sample must not show an availability notice.' }
$mode = Find-Element 'ContentEditButton'
if ((Find-Element 'DeleteTextButton').Current.IsEnabled) { throw 'Delete text must require an open inline target.' }
if ((Find-Element 'EditTextCardHint').Current.Name -ne 'Click a text run to retype it in place.') { throw 'Missing text selection guidance.' }
if (-not $mode.Current.IsEnabled) { throw 'Content editing must be available for the sample.' }
$mode.GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern).Toggle()
Start-Sleep -Milliseconds 150
if ($mode.GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern).Current.ToggleState -ne [System.Windows.Automation.ToggleState]::On) {
    throw 'Edit content did not arm.'
}
if ((Find-Element 'DeleteTextButton').Current.IsEnabled) { throw 'Arming Edit content alone must not enable Delete text.' }
$mode.GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern).Toggle()
Start-Sleep -Milliseconds 150
'PASS Edit PDF heading, Text/Images cards, accessible commands, availability and mode toggle'

function Get-Toggle([string]$id) {
    (Find-Element $id).GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern).Current.ToggleState
}
$on = [System.Windows.Automation.ToggleState]::On
$off = [System.Windows.Automation.ToggleState]::Off
if ((Find-Element 'DeleteImageButton').Current.IsEnabled -or (Find-Element 'ReplaceImageButton').Current.IsEnabled) {
    throw 'Delete and Replace image must require an image selected on the page.'
}
if ((Find-Element 'EditImagesCardHint').Current.Name -ne 'Click an image on the page to select it.') { throw 'Missing image selection guidance.' }
(Find-Element 'InsertTextButton').GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern).Toggle()
Start-Sleep -Milliseconds 150
if ((Get-Toggle 'InsertTextButton') -ne $on -or (Get-Toggle 'ContentEditButton') -ne $on) { throw 'Insert text must arm itself and Edit content.' }
(Find-Element 'InsertImageButton').GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern).Toggle()
Start-Sleep -Milliseconds 150
if ((Get-Toggle 'InsertImageButton') -ne $on -or (Get-Toggle 'InsertTextButton') -ne $off) { throw 'Insert text and Insert image must be mutually exclusive.' }
$mode.GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern).Toggle()
Start-Sleep -Milliseconds 150
if ((Get-Toggle 'ContentEditButton') -ne $off -or (Get-Toggle 'InsertImageButton') -ne $off) { throw 'Leaving Edit content must disarm the insert kind.' }
(Find-Element 'ToolsTab_Annotate').GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern).Toggle()
'PASS Insert text/image arm by click, imply Edit content, exclude each other; image commands follow the selection'

foreach ($id in @('PreviousAnnotationButton', 'NudgeButton', 'GrowButton', 'AnnotationColorButton', 'DeleteAnnotationButton')) {
    $command = Find-Element $id
    if ($null -eq $command -or $command.Current.IsOffscreen -or $command.Current.IsEnabled) {
        throw "Annotation selection action is missing or enabled without a selection: $id"
    }
}
if ((Find-Element 'AnnotationColorButton').Current.Name -ne 'Restyle') { throw 'Annotation color must be labelled Restyle.' }
$annotationTools = @('HighlightButton', 'UnderlineButton', 'StrikeoutButton', 'InkButton', 'NoteButton', 'ShapeButton', 'StampButton')
foreach ($armed in @('HighlightButton', 'UnderlineButton')) {
    (Find-Element $armed).GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern).Toggle()
    Start-Sleep -Milliseconds 150
    foreach ($id in $annotationTools) {
        $tool = Find-Element $id
        if ($null -eq $tool -or $tool.Current.IsOffscreen -or -not $tool.Current.IsEnabled) { throw "Annotation tool unavailable: $id" }
        $expected = if ($id -eq $armed) { [System.Windows.Automation.ToggleState]::On }
            else { [System.Windows.Automation.ToggleState]::Off }
        if ($tool.GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern).Current.ToggleState -ne $expected) {
            throw "More than one annotation tool is armed: $armed / $id"
        }
    }
}
(Find-Element 'UnderlineButton').GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern).Toggle()
'PASS Annotation selection gates, Restyle label and mutually exclusive tools'

(Find-Element 'ToolsTab_Sign').GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern).Toggle()
$certificate = Find-Element 'ChooseSigningCertificate'
if ($null -eq $certificate -or $certificate.Current.IsOffscreen -or -not $certificate.Current.IsEnabled -or
    $certificate.Current.Name -ne ('Choose signing certificate (.pfx)' + [char]0x2026)) { throw 'PFX signing command is missing or disabled for the sample.' }
foreach ($id in @('ChooseCardCertificate', 'ChooseComputerCertificate')) {
    $source = Find-Element $id
    if ($null -eq $source -or $source.Current.IsOffscreen -or $source.Current.IsEnabled) { throw "Unsupported signing source is missing or enabled: $id" }
}
$indicator = Find-Element 'SignedIndicator'
if ($null -ne $indicator -and -not $indicator.Current.IsOffscreen) { throw 'Unsigned sample must not claim a signature.' }
$certificate.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
$picker = $null
for ($attempt = 0; $attempt -lt 20 -and $null -eq $picker; $attempt++) {
    Start-Sleep -Milliseconds 250
    $picker = $window.FindFirst([System.Windows.Automation.TreeScope]::Descendants,
        [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::ControlTypeProperty,
            [System.Windows.Automation.ControlType]::Window))
}
if ($null -eq $picker) { throw 'Signing certificate command did not open a native picker.' }
$picker.GetCurrentPattern([System.Windows.Automation.WindowPattern]::Pattern).Close()
for ($attempt = 0; $attempt -lt 40 -and -not $certificate.Current.IsEnabled; $attempt++) { Start-Sleep -Milliseconds 100 }
if (-not $certificate.Current.IsEnabled -or (Find-Element 'DocumentTitle').Current.Name -ne 'Vitela sample.pdf' -or
    (Find-Element 'UndoButton').Current.IsEnabled) { throw 'Signing picker cancellation did not preserve the sample and command state.' }
(Find-Element 'ToolsTab_Annotate').GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern).Toggle()
'PASS Signing panel, unsupported-source gates, unsigned indicator, native picker and cancellation'

$find = Find-Element 'FindDocumentButton'
$find.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
Start-Sleep -Milliseconds 500
$query = Find-Element 'SearchBox'
if ($null -eq $query -or $query.Current.IsOffscreen -or -not $query.Current.HasKeyboardFocus) {
    throw 'Find flyout did not open and focus Search document.'
}
foreach ($id in @('PreviousMatchButton', 'NextMatchButton')) {
    $match = Find-Element $id
    if ($null -eq $match -or $match.Current.IsOffscreen -or $match.Current.IsEnabled) {
        throw "No-result navigation is missing or incorrectly enabled: $id"
    }
}
'PASS Native Find flyout, focused query and no-result navigation gates'
