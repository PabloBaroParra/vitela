# Dot-sourced by apps/windows/test-*.ps1. Those UI Automation scripts find
# controls by AutomationId but also match English names and messages, so the
# app under test must speak English. The shell follows the Windows display
# language (Localization/Language.cs), so on a Spanish machine start it with
# VITELA_LANGUAGE=en; Start-VitelaInEnglish does.

# Starts the x64 Debug build with VITELA_LANGUAGE=en and returns its process
# once the main window is in the UI Automation tree. Only this process gets the
# override; the caller's environment is left as it was.
function Start-VitelaInEnglish {
    $exe = Get-ChildItem -Path (Join-Path $PSScriptRoot '..\Pdf.Windows\bin\x64\Debug\*\Pdf.Windows.exe') -File -ErrorAction SilentlyContinue |
        Select-Object -First 1
    if (-not $exe) {
        throw 'No x64 Debug build of Pdf.Windows to start. Build it, or start the app yourself with $env:VITELA_LANGUAGE = ''en''.'
    }
    $saved = $env:VITELA_LANGUAGE
    $env:VITELA_LANGUAGE = 'en'
    try { $process = Start-Process -FilePath $exe.FullName -WorkingDirectory $exe.DirectoryName -PassThru }
    finally { $env:VITELA_LANGUAGE = $saved }

    $deadline = (Get-Date).AddSeconds(30)
    while ($true) {
        $process.Refresh()
        if ($process.HasExited) { throw "Pdf.Windows exited during startup (exit code $($process.ExitCode))." }
        if ($process.MainWindowHandle -ne [IntPtr]::Zero) {
            $window = [System.Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)
            $ready = $window.FindFirst([System.Windows.Automation.TreeScope]::Descendants,
                [System.Windows.Automation.PropertyCondition]::new(
                    [System.Windows.Automation.AutomationElement]::AutomationIdProperty, 'HomeRailButton'))
            if ($null -ne $ready) { break }
        }
        if ((Get-Date) -gt $deadline) { throw 'Pdf.Windows did not show its main window within 30 s.' }
        Start-Sleep -Milliseconds 250
    }

    # Recent documents load asynchronously after the window appears, and until
    # then Home shows its empty state. Give the cards a few seconds to arrive;
    # a machine with no recent documents simply keeps the empty state.
    $buttons = [System.Windows.Automation.PropertyCondition]::new(
        [System.Windows.Automation.AutomationElement]::ControlTypeProperty, [System.Windows.Automation.ControlType]::Button)
    $settle = (Get-Date).AddSeconds(5)
    while ((Get-Date) -lt $settle) {
        foreach ($button in $window.FindAll([System.Windows.Automation.TreeScope]::Descendants, $buttons)) {
            if ($button.Current.Name -like 'Open *.pdf') { return $process }
        }
        Start-Sleep -Milliseconds 250
    }
    return $process
}

# Warns when an already running app is not speaking English, before the script
# fails on a name it cannot find. The Home rail button reads "Home" in English.
function Assert-VitelaSpeaksEnglish($window) {
    $homeButton = $window.FindFirst([System.Windows.Automation.TreeScope]::Descendants,
        [System.Windows.Automation.PropertyCondition]::new(
            [System.Windows.Automation.AutomationElement]::AutomationIdProperty, 'HomeRailButton'))
    if ($null -ne $homeButton -and $homeButton.Current.Name -ne 'Home') {
        Write-Warning ("Pdf.Windows is not speaking English (its Home button reads '$($homeButton.Current.Name)'). " +
            "This script matches English text; restart the app with `$env:VITELA_LANGUAGE = 'en'.")
    }
}
