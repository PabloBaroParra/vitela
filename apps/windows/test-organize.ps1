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
function Wait-State([scriptblock]$Condition, [string]$Message) {
    for ($attempt = 0; $attempt -lt 100; $attempt++) {
        if (& $Condition) { return }
        Start-Sleep -Milliseconds 100
    }
    throw $Message
}
function Invoke-Button([string]$Id) {
    (Find-Element $Id).GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
}
function Page-Count {
    $grid = Find-Element 'OrganizeGrid'
    return $grid.FindAll([System.Windows.Automation.TreeScope]::Children,
        [System.Windows.Automation.PropertyCondition]::new(
            [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
            [System.Windows.Automation.ControlType]::ListItem)).Count
}

$return = Find-Element 'ReturnToDocumentButton'
if ($null -ne $return -and -not $return.Current.IsOffscreen) {
    Invoke-Button 'ReturnToDocumentButton'
    Wait-State { $title = Find-Element 'DocumentTitle'; $null -ne $title -and -not $title.Current.IsOffscreen } 'Return from Home did not show the sample editor.'
}
$title = Find-Element 'DocumentTitle'
if ($null -eq $title -or $title.Current.IsOffscreen -or $title.Current.Name -ne 'Vitela sample.pdf') {
    throw 'This smoke only edits the built-in sample, never a user document.'
}
if ((Find-Element 'UndoButton').Current.IsEnabled -or (Find-Element 'RedoButton').Current.IsEnabled) {
    throw 'Start with an unchanged sample and empty history.'
}
(Find-Element 'OrganizeButton').GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern).Toggle()
Wait-State { $heading = Find-Element 'OrganizeHeading'; $null -ne $heading -and -not $heading.Current.IsOffscreen } 'Organize screen did not open.'
$previousAction = $null
foreach ($entry in @(@('OrganizeHeading', 'Organize pages'), @('OrganizeUndoButton', 'Undo'),
    @('OrganizeRedoButton', 'Redo'), @('OrganizeAddPdfsButton', 'Add PDFs'),
    @('OrganizeExtractButton', 'Extract'), @('OrganizeSplitButton', 'Split'), @('OrganizeSaveButton', 'Save'))) {
    $element = Find-Element $entry[0]
    if ($null -eq $element -or $element.Current.IsOffscreen -or $element.Current.Name -ne $entry[1]) {
        throw "Missing Organize heading/action: $($entry[0])"
    }
    if ($entry[0] -ne 'OrganizeHeading') {
        $bounds = $element.Current.BoundingRectangle
        if ($null -ne $previousAction -and ($bounds.Top -lt $previousAction.Top -or
            ($bounds.Top -eq $previousAction.Top -and $bounds.Left -le $previousAction.Left))) {
            throw "Organize header action order disagrees at $($entry[0])"
        }
        $previousAction = $bounds
    }
}
foreach ($id in @('EditorToolbar', 'DocumentSidebar', 'PagesSidebar', 'PageScroller')) {
    $element = Find-Element $id
    if ($null -ne $element -and -not $element.Current.IsOffscreen) { throw "Editor chrome leaked into dedicated Organize screen: $id" }
}
foreach ($id in @('OrganizeUndoButton', 'OrganizeRedoButton', 'OrganizeAddPdfsButton')) {
    if ((Find-Element $id).Current.IsEnabled) { throw "Empty history/unsupported import must be disabled: $id" }
}
if (-not (Find-Element 'OrganizeExtractButton').Current.IsEnabled -or
    -not (Find-Element 'OrganizeSaveButton').Current.IsEnabled) { throw 'Extract/Save must be available for the sample.' }
Wait-State { $card = Find-Element 'OrganizeDocument_0'; $null -ne $card -and -not $card.Current.IsOffscreen } 'Documents default did not show the sample block card.'
if ((Find-Element 'OrganizeDocumentsToggle').GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern).Current.ToggleState -ne
    [System.Windows.Automation.ToggleState]::On -or
    (Find-Element 'OrganizePagesToggle').GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern).Current.ToggleState -ne
    [System.Windows.Automation.ToggleState]::Off) { throw 'Organize must open on Documents with an exclusive selector.' }
if ((Find-Element 'OrganizeDocument_0').Current.Name -notlike 'Vitela sample.pdf,*document 1 of 1') {
    throw 'Document card lost its source name or accessible position.'
}
foreach ($id in @('OrganizeDocument_0_Moveup', 'OrganizeDocument_0_Movedown')) {
    if ((Find-Element $id).Current.IsEnabled) { throw "Single-block move must be disabled: $id" }
}
foreach ($id in @('OrganizeDocument_0_Rotateleft', 'OrganizeDocument_0_Rotateright', 'OrganizeDocument_0_Delete')) {
    if (-not (Find-Element $id).Current.IsEnabled) { throw "Block action must be enabled for the sample: $id" }
}
'PASS Documents default, exclusive selector, source/position label and keyboard action gates'
(Find-Element 'OrganizePagesToggle').GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern).Toggle()
Wait-State { (Page-Count) -gt 0 } 'Pages selector did not show sample page cards.'
$initialCount = Page-Count
if ($initialCount -lt 1) { throw 'Expected visible sample page cards.' }
if ((Find-Element 'OrganizeSplitButton').Current.IsEnabled -ne ($initialCount -gt 1)) { throw 'Split gate disagrees with page count.' }
'PASS Dedicated Organize heading, ordered actions, isolated screen, history/output/import gates'

(Find-Element 'HomeRailButton').GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern).Toggle()
Wait-State { $return = Find-Element 'ReturnToDocumentButton'; $null -ne $return -and -not $return.Current.IsOffscreen } 'Home did not retain an organize session.'
$heading = Find-Element 'OrganizeHeading'
if ($null -ne $heading -and -not $heading.Current.IsOffscreen) { throw 'Organize screen leaked into Home.' }
Invoke-Button 'ReturnToDocumentButton'
Wait-State { $heading = Find-Element 'OrganizeHeading'; $null -ne $heading -and -not $heading.Current.IsOffscreen } 'Return from Home did not restore Organize.'
if ((Page-Count) -ne $initialCount) { throw 'Home changed the organize page set.' }
'PASS Home hides and restores the same dedicated Organize session'

Invoke-Button 'InsertBlankPageButton'
Wait-State { (Page-Count) -eq ($initialCount + 1) -and (Find-Element 'OrganizeUndoButton').Current.IsEnabled } 'Blank insertion did not refresh cards/history.'
Invoke-Button 'OrganizeUndoButton'
Wait-State { (Page-Count) -eq $initialCount -and (Find-Element 'OrganizeRedoButton').Current.IsEnabled } 'Organize Undo did not restore the page count.'
Invoke-Button 'OrganizeRedoButton'
Wait-State { (Page-Count) -eq ($initialCount + 1) -and (Find-Element 'OrganizeUndoButton').Current.IsEnabled } 'Organize Redo did not restore the added page.'
Invoke-Button 'OrganizeUndoButton'
Wait-State { (Page-Count) -eq $initialCount -and (Find-Element 'OrganizeRedoButton').Current.IsEnabled } 'Final undo did not restore sample pages.'
Invoke-Button 'OrganizeReturnButton'
Wait-State { $title = Find-Element 'DocumentTitle'; $null -ne $title -and -not $title.Current.IsOffscreen } 'Return to document did not restore editor.'
if ((Find-Element 'DocumentTitle').Current.Name -ne 'Vitela sample.pdf' -or
    -not (Find-Element 'RedoButton').Current.IsEnabled) { throw 'Return lost the session or shared history.' }
(Find-Element 'OrganizeButton').GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern).Toggle()
Wait-State { $heading = Find-Element 'OrganizeHeading'; $null -ne $heading -and -not $heading.Current.IsOffscreen } 'Reenter did not restore dedicated screen.'
(Find-Element 'OrganizePagesToggle').GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern).Toggle()
Wait-State { (Page-Count) -eq $initialCount -and (Find-Element 'OrganizeRedoButton').Current.IsEnabled } "Reentry lost cards or history (expected $initialCount pages and enabled Redo)."
Invoke-Button 'OrganizeReturnButton'
'PASS Real-core blank insertion, shared Undo/Redo, restored pages, return and reentry'
