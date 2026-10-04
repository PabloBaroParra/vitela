param([switch]$TestPicker, [switch]$TestSample)

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
$processes = @(Get-Process Pdf.Windows -ErrorAction SilentlyContinue)
if ($processes.Count -ne 1) { throw 'Start exactly one Pdf.Windows instance before running the Home smoke test.' }
$window = [System.Windows.Automation.AutomationElement]::FromHandle($processes[0].MainWindowHandle)

function Find-Element([string]$Id) {
    $condition = [System.Windows.Automation.PropertyCondition]::new(
        [System.Windows.Automation.AutomationElement]::AutomationIdProperty, $Id)
    return $window.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condition)
}

$search = Find-Element 'HomeSearchBox'
if ($null -eq $search -or $search.Current.IsOffscreen) { throw 'The initial Home screen is not visible.' }
$homeButton = Find-Element 'HomeRailButton'
if ($null -eq $homeButton -or -not $homeButton.Current.IsEnabled) { throw 'Home navigation is not enabled.' }
$value = $search.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern)
$originalQuery = $value.Current.Value
try {
    $value.SetValue('')
    Start-Sleep -Milliseconds 500
    $elements = $window.FindAll([System.Windows.Automation.TreeScope]::Descendants,
        [System.Windows.Automation.Condition]::TrueCondition)
    $cardCount = 0
    foreach ($element in $elements) {
        if ($element.Current.ControlType -eq [System.Windows.Automation.ControlType]::Button -and
            $element.Current.Name -like 'Open *.pdf') { $cardCount++ }
    }
    if ($cardCount -gt 8) { throw 'Recent must show at most eight PDF cards.' }
    $value.SetValue('vitela-no-match-8ec3e8d4')
    $expected = if ($cardCount -gt 0) { 'No recent document matches that search.' }
        else { 'No recent documents yet. Open a PDF and it will show up here.' }
    # UI Automation can observe the preceding layout while the async recent
    # refresh is still settling. Wait for the exact state; never skip its assertion.
    $empty = $null
    for ($attempt = 0; $attempt -lt 20; $attempt++) {
        Start-Sleep -Milliseconds 100
        $empty = Find-Element 'RecentEmptyMessage'
        if ($null -ne $empty -and $empty.Current.Name -eq $expected) { break }
    }
    if ($null -eq $empty -or $empty.Current.Name -ne $expected) {
        $actual = if ($null -eq $empty) { '<missing>' } else { $empty.Current.Name }
        throw "Recent empty/filter state does not match: $cardCount cards, expected '$expected', actual '$actual'."
    }
    $tools = $window.FindFirst([System.Windows.Automation.TreeScope]::Descendants,
        [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty, 'Tools'))
    if ($null -ne $tools -and -not $tools.Current.IsOffscreen) { throw 'The empty Tools card must be hidden.' }
    'PASS Home initial screen, eight-card cap and no-match filtering'

    if ($TestPicker) {
        $select = Find-Element 'HomeSelectFileButton'
        if ($null -eq $select) { throw 'Select file is not a real accessible button.' }
        $select.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
        $dialog = $null
        $pickerPattern = $null
        for ($attempt = 0; $attempt -lt 80 -and $null -eq $pickerPattern; $attempt++) {
            Start-Sleep -Milliseconds 250
            $candidates = $window.FindAll([System.Windows.Automation.TreeScope]::Descendants,
                [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::ControlTypeProperty,
                    [System.Windows.Automation.ControlType]::Window))
            foreach ($candidate in $candidates) {
                if ($candidate.Current.ClassName -ne '#32770' -or $candidate.Current.NativeWindowHandle -eq 0) { continue }
                $pattern = $null
                if ($candidate.TryGetCurrentPattern([System.Windows.Automation.WindowPattern]::Pattern, [ref]$pattern)) {
                    $dialog = $candidate
                    $pickerPattern = $pattern
                    break
                }
            }
        }
        if ($null -eq $pickerPattern) { throw 'Select file did not open a native picker.' }
        $pickerPattern.Close()
        for ($attempt = 0; $attempt -lt 20 -and -not $homeButton.Current.IsEnabled; $attempt++) {
            Start-Sleep -Milliseconds 250
        }
        if (-not $homeButton.Current.IsEnabled) { throw 'Cancelling the picker did not restore navigation.' }
        'PASS Select file picker and cancellation recovery'
    }
}
finally {
    # Do not mask an assertion failure with cleanup against a modal-disabled Home.
    # A failed picker assertion still fails the script; restoration is best effort.
    if ($search.Current.IsEnabled) { $value.SetValue($originalQuery) }
}

if ($TestSample) {
    # Only enable this on a freshly launched instance: it replaces the document.
    $sample = $window.FindFirst([System.Windows.Automation.TreeScope]::Descendants,
        [System.Windows.Automation.AndCondition]::new(
            [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty, 'Open the sample'),
            [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::ControlTypeProperty,
                [System.Windows.Automation.ControlType]::Button)))
    if ($null -eq $sample) { throw 'The sample quick action is missing.' }
    $sample.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
    $title = $null
    for ($attempt = 0; $attempt -lt 40; $attempt++) {
        Start-Sleep -Milliseconds 250
        $title = Find-Element 'DocumentTitle'
        if ($null -ne $title -and $title.Current.Name -eq 'Vitela sample.pdf' -and -not $title.Current.IsOffscreen) { break }
    }
    if ($null -eq $title -or $title.Current.Name -ne 'Vitela sample.pdf' -or $title.Current.IsOffscreen) {
        throw 'Opening the sample did not switch to the editor.'
    }
    $homeButton.GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern).Toggle()
    Start-Sleep -Milliseconds 500
    $returnButton = Find-Element 'ReturnToDocumentButton'
    if ($null -eq $returnButton -or $returnButton.Current.IsOffscreen) { throw 'Home lost the open-document return action.' }
    $returnButton.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
    Start-Sleep -Milliseconds 500
    $title = Find-Element 'DocumentTitle'
    if ($null -eq $title -or $title.Current.Name -ne 'Vitela sample.pdf' -or $title.Current.IsOffscreen) {
        throw 'Returning from Home did not preserve the sample session.'
    }
    $homeButton.GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern).Toggle()
    'PASS Sample quick action and Home/editor session preservation'
}
