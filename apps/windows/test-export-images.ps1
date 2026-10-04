param()
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
$processes = @(Get-Process Pdf.Windows -ErrorAction SilentlyContinue)
if ($processes.Count -ne 1) { throw 'Start exactly one normal Pdf.Windows with the unchanged built-in sample open.' }
$process = $processes[0]
$window = [System.Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)
function Find-Element([string]$Id) {
    $window.FindFirst([System.Windows.Automation.TreeScope]::Descendants,
        [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::AutomationIdProperty, $Id))
}
function Wait-For([scriptblock]$Condition, [string]$Message) {
    $deadline = [DateTime]::UtcNow.AddSeconds(15)
    do { if (& $Condition) { return }; Start-Sleep -Milliseconds 100 } while ([DateTime]::UtcNow -lt $deadline)
    throw $Message
}
function Invoke-Button([string]$Id) {
    (Find-Element $Id).GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
}
$return = Find-Element 'ReturnToDocumentButton'
if ($null -ne $return -and -not $return.Current.IsOffscreen) { Invoke-Button 'ReturnToDocumentButton' }
Wait-For { $null -ne (Find-Element 'DocumentTitle') } 'Document toolbar missing.'
if ((Find-Element 'DocumentTitle').Current.Name -ne 'Vitela sample.pdf' -or
    (Find-Element 'UndoButton').Current.IsEnabled -or (Find-Element 'RedoButton').Current.IsEnabled) {
    throw 'Never change a user document: this test requires the unchanged built-in sample and empty history.'
}
Invoke-Button 'ExportImagesButton'
Wait-For { $null -ne (Find-Element 'CloseButton') } 'Export dialog missing.'
Invoke-Button 'CloseButton'
Wait-For { (Find-Element 'ExportImagesButton').Current.IsEnabled } 'Dialog cancellation failed to restore output.'
if ((Find-Element 'AnnotationStatus').Current.Name -ne 'Export cancelled.') { throw 'Export cancellation status differs.' }
Invoke-Button 'ExportImagesButton'
Wait-For { $null -ne (Find-Element 'PrimaryButton') } 'Second Export dialog missing.'
Invoke-Button 'PrimaryButton'
$script:pickerPattern = $null
Wait-For {
    $candidates = $window.FindAll([System.Windows.Automation.TreeScope]::Descendants,
        [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::ControlTypeProperty,
            [System.Windows.Automation.ControlType]::Window))
    foreach ($candidate in $candidates) {
        # A departing ContentDialog can still expose WindowPattern; only close the native picker HWND.
        if ($candidate.Current.ClassName -ne '#32770' -or $candidate.Current.NativeWindowHandle -eq 0) { continue }
        $pattern = $null
        if ($candidate.TryGetCurrentPattern([System.Windows.Automation.WindowPattern]::Pattern, [ref]$pattern)) {
            $script:pickerPattern = $pattern
            break
        }
    }
    $null -ne $script:pickerPattern
} 'Valid export did not reach native folder picker.'
$script:pickerPattern.Close()
Wait-For { (Find-Element 'ExportImagesButton').Current.IsEnabled } 'Folder cancellation did not restore controls.'
if ((Find-Element 'AnnotationStatus').Current.Name -ne 'Export cancelled. No file was written.' -or
    (Find-Element 'UndoButton').Current.IsEnabled -or (Find-Element 'RedoButton').Current.IsEnabled) {
    throw 'Folder cancellation changed history or failed to report no output.'
}
'PASS native Export dialog cancellation, folder picker cancellation, control recovery and unchanged history'
