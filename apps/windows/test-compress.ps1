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
function Find-Name([string]$Name) {
    $window.FindFirst([System.Windows.Automation.TreeScope]::Descendants,
        [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty, $Name))
}
function Wait-For([scriptblock]$Condition, [string]$Message) {
    $deadline = [DateTime]::UtcNow.AddSeconds(20)
    do { if (& $Condition) { return }; Start-Sleep -Milliseconds 100 } while ([DateTime]::UtcNow -lt $deadline)
    throw $Message
}
function Invoke-Button([string]$Id) {
    (Find-Element $Id).GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
}
$return = Find-Element 'ReturnToDocumentButton'
if ($null -ne $return -and -not $return.Current.IsOffscreen) { Invoke-Button 'ReturnToDocumentButton' }
Wait-For { $null -ne (Find-Element 'DocumentTitle') } 'Document toolbar did not appear.'
if ((Find-Element 'DocumentTitle').Current.Name -ne 'Vitela sample.pdf' -or
    (Find-Element 'UndoButton').Current.IsEnabled -or (Find-Element 'RedoButton').Current.IsEnabled) {
    throw 'Never change a user document: this test requires the unchanged built-in sample and empty history.'
}
Invoke-Button 'CompressButton'
Wait-For { $null -ne (Find-Name 'Compress PDF') } 'Compress dialog did not appear.'
$texts = $window.FindAll([System.Windows.Automation.TreeScope]::Descendants,
    [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::ControlTypeProperty,
        [System.Windows.Automation.ControlType]::Text))
$size = @($texts | Where-Object { $_.Current.Name -match '^This file is .+ on disk\.$' })
if ($size.Count -ne 1) { throw 'Dialog must show exactly one measured source-size sentence.' }
foreach ($name in @('Lossless', 'Balanced', 'Small',
    'Repack the file only. Every page renders exactly as it does now.',
    'Also bring images down to 150 DPI. Visibly unchanged on screen, materially smaller on disk.',
    'Also bring images down to 96 DPI. For when the file has to fit through something and the pixels matter less than that.')) {
    if ($null -eq (Find-Name $name)) { throw "Missing preset label/description: $name" }
}
$balanced = Find-Name 'Balanced'
$selection = $balanced.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern)
if (-not $selection.Current.IsSelected) { throw 'Balanced must be the default preset.' }
Invoke-Button 'CloseButton'
Wait-For { (Find-Element 'CompressButton').Current.IsEnabled } 'Cancellation did not restore Compress.'
if ((Find-Element 'AnnotationStatus').Current.Name -ne 'Compression cancelled.') { throw 'Cancellation status differs.' }
'PASS Compress source-size, three full presets, Balanced default and cancellation'
Invoke-Button 'CompressButton'
Wait-For { $null -ne (Find-Element 'PrimaryButton') } 'Second Compress dialog did not appear.'
Invoke-Button 'PrimaryButton'
$script:pickerPattern = $null
Wait-For {
    $candidates = $window.FindAll([System.Windows.Automation.TreeScope]::Descendants,
        [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::ControlTypeProperty,
            [System.Windows.Automation.ControlType]::Window))
    foreach ($candidate in $candidates) {
        $pattern = $null
        if ($candidate.TryGetCurrentPattern([System.Windows.Automation.WindowPattern]::Pattern, [ref]$pattern)) {
            $script:pickerPattern = $pattern
            break
        }
    }
    ($null -ne $script:pickerPattern) -or ((Find-Element 'AnnotationStatus').Current.Name -like 'Nothing to gain:*')
} 'Compression neither reported no gain nor opened a native destination.'
if ($null -ne $script:pickerPattern) {
    $status = (Find-Element 'AnnotationStatus').Current.Name
    if ($status -notmatch 'uncompressed.+compressed \(\d+% smaller\)\. Choose where to write it\.') {
        throw "Picker opened without measured before/after status: $status"
    }
    $script:pickerPattern.Close()
    'PASS real-core measured reduction before native picker and picker cancellation'
} else {
    if ((Find-Element 'AnnotationStatus').Current.Name -notlike '*so no file was written.*') { throw 'No-gain result must explain that nothing was written.' }
    'PASS real-core no-gain result without a destination picker'
}
Wait-For { (Find-Element 'CompressButton').Current.IsEnabled } 'Output flow did not restore controls.'
if ((Find-Element 'DocumentTitle').Current.Name -ne 'Vitela sample.pdf' -or
    (Find-Element 'UndoButton').Current.IsEnabled -or (Find-Element 'RedoButton').Current.IsEnabled) {
    throw 'Compression changed the live document or history.'
}
'PASS live session and empty history preserved'
