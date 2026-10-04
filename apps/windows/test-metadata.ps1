param()

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
$processes = @(Get-Process Pdf.Windows -ErrorAction SilentlyContinue)
if ($processes.Count -ne 1) { throw 'Start exactly one Pdf.Windows instance with an unchanged built-in sample open.' }
$window = [System.Windows.Automation.AutomationElement]::FromHandle($processes[0].MainWindowHandle)

function Find-Element([string]$Id) {
    return $window.FindFirst([System.Windows.Automation.TreeScope]::Descendants,
        [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::AutomationIdProperty, $Id))
}
function Value([string]$Id) {
    return (Find-Element $Id).GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).Current.Value
}
function Set-Value([string]$Id, [string]$Text) {
    (Find-Element $Id).GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).SetValue($Text)
}
function Wait-Value([string]$Id, [string]$Expected) {
    $deadline = [DateTime]::UtcNow.AddSeconds(5)
    do {
        if ((Value $Id) -ceq $Expected) { return }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "Expected $Id to be '$Expected'; got '$(Value $Id)'."
}
function Invoke-Command([string]$Id) {
    (Find-Element $Id).GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
    Start-Sleep -Milliseconds 200
}

$return = Find-Element 'ReturnToDocumentButton'
if ($null -ne $return -and -not $return.Current.IsOffscreen) { Invoke-Command 'ReturnToDocumentButton' }
if ((Find-Element 'DocumentTitle').Current.Name -ne 'Vitela sample.pdf') { throw 'Never edit a user document in this smoke test.' }
(Find-Element 'ToolsTab_Annotate').GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern).Toggle()
if ((Find-Element 'UndoButton').Current.IsEnabled) { throw 'The sample must be unchanged.' }
foreach ($id in @('MetadataTitle', 'MetadataAuthor', 'MetadataSubject', 'MetadataKeywords', 'MetadataCreator', 'MetadataProducer', 'MetadataCreated', 'MetadataModified')) {
    if ($null -eq (Find-Element $id) -or -not (Find-Element $id).Current.IsEnabled) { throw "Missing/enabled-state mismatch: $id" }
}
if ($null -ne (Find-Element 'ApplyMetadataButton')) { throw 'Properties must not require Apply.' }
if ((Find-Element 'MetadataPages').Current.Name -notmatch '^Pages: [1-9][0-9]*$') { throw 'Missing page count.' }
'PASS Properties fields, Pages and immediate-edit presentation'

$originalTitle = Value 'MetadataTitle'
$originalCreated = Value 'MetadataCreated'
Set-Value 'MetadataTitle' 'Metadata parity smoke'
$deadline = [DateTime]::UtcNow.AddSeconds(5)
while (-not (Find-Element 'UndoButton').Current.IsEnabled -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 100 }
if (-not (Find-Element 'UndoButton').Current.IsEnabled) { throw 'A text change must immediately enter history.' }
Invoke-Command 'UndoButton'
Wait-Value 'MetadataTitle' $originalTitle
Invoke-Command 'RedoButton'
Wait-Value 'MetadataTitle' 'Metadata parity smoke'
Invoke-Command 'UndoButton'
Wait-Value 'MetadataTitle' $originalTitle
'PASS Immediate text edit, Undo and Redo'

# Focusing the native field then another control exercises its LostFocus commit.
$created = Find-Element 'MetadataCreated'
$scrollItem = $null
if ($created.TryGetCurrentPattern([System.Windows.Automation.ScrollItemPattern]::Pattern, [ref]$scrollItem)) { $scrollItem.ScrollIntoView() }
$created.SetFocus()
Set-Value 'MetadataCreated' '2026-02-30 08:30'
(Find-Element 'MetadataModified').SetFocus()
Wait-Value 'MetadataCreated' '2026-02-30 08:30:00'
Invoke-Command 'UndoButton'
Wait-Value 'MetadataCreated' $originalCreated
Invoke-Command 'RedoButton'
Wait-Value 'MetadataCreated' '2026-02-30 08:30:00'
Invoke-Command 'UndoButton'
Wait-Value 'MetadataCreated' $originalCreated
$created.SetFocus()
Set-Value 'MetadataCreated' '2026-13-01'
(Find-Element 'MetadataModified').SetFocus()
Wait-Value 'MetadataCreated' $originalCreated
if ((Find-Element 'UndoButton').Current.IsEnabled) { throw 'Invalid dates must not create history.' }
'PASS Focus-loss date normalization, component-valid February 30, Undo/Redo and invalid-date reversion'
