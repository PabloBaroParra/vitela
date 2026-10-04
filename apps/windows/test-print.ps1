param()

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
$processes = @(Get-Process Pdf.Windows -ErrorAction SilentlyContinue)
if ($processes.Count -ne 1) { throw 'Start exactly one normal Pdf.Windows with the unchanged built-in sample open.' }
$window = [System.Windows.Automation.AutomationElement]::FromHandle($processes[0].MainWindowHandle)
$desktop = [System.Windows.Automation.AutomationElement]::RootElement
function Find-Element($Root, [string]$Id) {
    $Root.FindFirst([System.Windows.Automation.TreeScope]::Descendants,
        [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::AutomationIdProperty, $Id))
}
function Wait-For([scriptblock]$Condition, [string]$Message) {
    $deadline = [DateTime]::UtcNow.AddSeconds(20)
    do { if (& $Condition) { return }; Start-Sleep -Milliseconds 100 } while ([DateTime]::UtcNow -lt $deadline)
    throw $Message
}
if ((Find-Element $window 'DocumentTitle').Current.Name -ne 'Vitela sample.pdf' -or
    (Find-Element $window 'UndoButton').Current.IsEnabled -or (Find-Element $window 'RedoButton').Current.IsEnabled) {
    throw 'Never print a user document: this test requires the unchanged built-in sample and empty history.'
}
$counter = (Find-Element $window 'PageCounter').GetCurrentPattern(
    [System.Windows.Automation.TextPattern]::Pattern).DocumentRange.GetText(-1)
if ($counter -notmatch '(\d+)\D*$') { throw "Cannot read sample page count: $counter" }
$total = [int]$Matches[1]
for ($run = 0; $run -lt 2; $run++) {
    $button = Find-Element $window 'PrintButton'
    if (-not $button.Current.IsEnabled) { throw 'Print must be enabled before starting.' }
    $button.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
    $script:printWindow = $null
    Wait-For {
        $candidates = $desktop.FindAll([System.Windows.Automation.TreeScope]::Children,
            [System.Windows.Automation.Condition]::TrueCondition)
        foreach ($candidate in $candidates) {
            # Windows hosts the task outside the app, under the shell's frame.
            # Match the known sample title; do not depend on localized "Print".
            if ($candidate.Current.ClassName -eq 'ApplicationFrameWindow' -and
                $candidate.Current.Name -like 'Vitela sample.pdf - *') {
                $script:printWindow = $candidate
                break
            }
        }
        $null -ne $script:printWindow
    } 'Native sample Print UI did not appear.'
    Wait-For { $null -ne (Find-Element $script:printWindow 'TotalPageNumber') } 'Native page count did not appear.'
    if ([int](Find-Element $script:printWindow 'TotalPageNumber').Current.Name -ne $total) {
        throw 'Native print page count must include every sample page.'
    }
    $preview = Find-Element $script:printWindow 'PreviewImage'
    if ($null -eq $preview -or $preview.Current.IsOffscreen) { throw 'Native print preview is missing.' }
    foreach ($id in @('PrintButton', 'OpenButton', 'SaveButton')) {
        if ((Find-Element $window $id).Current.IsEnabled) { throw "Print task did not guard $id." }
    }
    $cancel = Find-Element $script:printWindow 'CloseButton'
    if ($null -eq $cancel) { throw 'Native Print Cancel button is missing.' }
    $cancel.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
    Wait-For { (Find-Element $window 'PrintButton').Current.IsEnabled -and
        (Find-Element $window 'OpenButton').Current.IsEnabled -and
        (Find-Element $window 'SaveButton').Current.IsEnabled } 'Native cancellation did not release print ownership.'
    if ((Find-Element $window 'DocumentTitle').Current.Name -ne 'Vitela sample.pdf' -or
        (Find-Element $window 'UndoButton').Current.IsEnabled -or (Find-Element $window 'RedoButton').Current.IsEnabled) {
        throw 'Native Print cancellation changed the document/history.'
    }
}
'PASS native Print UI, complete page count/preview, operation gates, Cancel/completion/retry and unchanged session/history (two runs, no job submitted)'
