param()

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -TypeDefinition 'using System; using System.Runtime.InteropServices; public static class LifecycleClose { [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l); }'
$processes = @(Get-Process Pdf.Windows -ErrorAction SilentlyContinue)
if ($processes.Count -ne 1) { throw 'Start exactly one normal Pdf.Windows with the unchanged built-in sample open.' }
$process = $processes[0]
$window = [System.Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)
function Find-Element([string]$Id) {
    return $window.FindFirst([System.Windows.Automation.TreeScope]::Descendants,
        [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::AutomationIdProperty, $Id))
}
function Wait-For([scriptblock]$Condition, [string]$Message) {
    $deadline = [DateTime]::UtcNow.AddSeconds(10)
    do { if (& $Condition) { return }; Start-Sleep -Milliseconds 100 } while ([DateTime]::UtcNow -lt $deadline)
    throw $Message
}
function Invoke-Button([string]$Id) {
    (Find-Element $Id).GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
}
function Cancel-Picker {
    $script:picker = $null
    $script:pickerPattern = $null
    Wait-For {
        $candidates = $window.FindAll([System.Windows.Automation.TreeScope]::Descendants,
            [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::ControlTypeProperty,
                [System.Windows.Automation.ControlType]::Window))
        foreach ($candidate in $candidates) {
            $pattern = $null
            if ($candidate.TryGetCurrentPattern([System.Windows.Automation.WindowPattern]::Pattern, [ref]$pattern)) {
                $script:picker = $candidate
                $script:pickerPattern = $pattern
                break
            }
        }
        $null -ne $script:picker
    } 'Save did not show a native picker.'
    $script:pickerPattern.Close()
    Wait-For { (Find-Element 'SaveButton').Current.IsEnabled } 'Save picker cancellation did not restore controls.'
}
$return = Find-Element 'ReturnToDocumentButton'
if ($null -ne $return -and -not $return.Current.IsOffscreen) { Invoke-Button 'ReturnToDocumentButton' }
Wait-For { $null -ne (Find-Element 'DocumentTitle') } 'Document toolbar did not appear.'
if ((Find-Element 'DocumentTitle').Current.Name -ne 'Vitela sample.pdf' -or (Find-Element 'UndoButton').Current.IsEnabled) {
    throw 'Never change a user document: this test requires the unchanged built-in sample.'
}
Invoke-Button 'SaveButton'
Cancel-Picker
'PASS native Save picker cancellation and control recovery'
(Find-Element 'ToolsTab_Annotate').GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern).Toggle()
$title = Find-Element 'MetadataTitle'
$original = $title.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).Current.Value
$title.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).SetValue('Lifecycle picker smoke')
Wait-For { (Find-Element 'UndoButton').Current.IsEnabled } 'Fixture edit did not enter history.'
try {
    if (-not [LifecycleClose]::PostMessage($process.MainWindowHandle, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)) { throw 'Native close request failed.' }
    Wait-For { $null -ne (Find-Element 'PrimaryButton') } 'Dirty native close did not show a confirmation.'
    Invoke-Button 'PrimaryButton'
    Cancel-Picker
    $process.Refresh()
    if ($process.HasExited -or -not (Find-Element 'UndoButton').Current.IsEnabled -or
        (Find-Element 'MetadataTitle').GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).Current.Value -ne 'Lifecycle picker smoke') {
        throw 'Close/Save picker cancellation lost the window or pending work.'
    }
    'PASS native Close -> Save -> cancelled picker keeps the window and pending work'
}
finally {
    if ((Find-Element 'UndoButton').Current.IsEnabled) { Invoke-Button 'UndoButton' }
}
Wait-For { (Find-Element 'MetadataTitle').GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).Current.Value -ceq $original } 'Sample metadata was not restored.'
'PASS sample restored through Undo'
