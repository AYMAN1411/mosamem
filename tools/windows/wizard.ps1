# Drives the Mosamem installer or uninstaller in the desktop session (start it with a scheduled
# task, e.g. research's inrun.sh), through UI Automation: no keyboard focus or mouse needed.
#   wizard.ps1 dump              lists the wizard windows' elements into C:\Mosamem\wizard.txt
#   wizard.ps1 pick <index>      selects an item of the language box (dump lists them)
#   wizard.ps1 click <name>…     presses the first enabled button with one of these names
#   wizard.ps1 next | back | cancel | yes   presses Next/Install/Finish (id 1), Back (3), Cancel (2), or Yes (6) in a question
param([string]$action, [string[]]$names)
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
Add-Type @"
using System; using System.Runtime.InteropServices;
public static class W {
  [DllImport("user32.dll")] public static extern IntPtr PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern IntPtr SendMessage(IntPtr h, uint m, IntPtr w, string l);
  [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern IntPtr SendMessage(IntPtr h, uint m, IntPtr w, System.Text.StringBuilder l);
}
"@
$log = "C:\Mosamem\wizard.txt"
$ids = @(Get-Process | Where-Object { $_.ProcessName -like "Mosamem*setup*" -or $_.ProcessName -like "Mosamem-Setup*" -or $_.ProcessName -eq "uninstall" -or $_.ProcessName -eq "Un" -or $_.ProcessName -like "Un_*" -or $_.ProcessName -like "Au_*" } | ForEach-Object { $_.Id })
$root = [Windows.Automation.AutomationElement]::RootElement
$all = [Windows.Automation.Condition]::TrueCondition
$out = @("pids: $ids")
$id = @{ next = 1; cancel = 2; back = 3; yes = 6 }[$action]
foreach ($w in $root.FindAll([Windows.Automation.TreeScope]::Children, $all)) {
  if ($ids -notcontains $w.Current.ProcessId) { continue }
  $out += "window [$($w.Current.Name)] at $($w.Current.BoundingRectangle) offscreen=$($w.Current.IsOffscreen)"
  if ($id) {
    # WM_COMMAND with the button's id, to the wizard window
    [W]::PostMessage([IntPtr]$w.Current.NativeWindowHandle, 0x0111, [IntPtr]$id, [IntPtr]::Zero) | Out-Null
    $out += "pressed $action"; $out | Set-Content $log -Encoding UTF8; exit 0
  }
  foreach ($e in $w.FindAll([Windows.Automation.TreeScope]::Descendants, $all)) {
    $c = $e.Current
    $n = ($c.Name -replace "&", "").Trim()
    if ($action -eq "dump") {
      $out += "  $($c.ControlType.ProgrammaticName) [$n] class=$($c.ClassName) enabled=$($c.IsEnabled)"
      if ($c.ClassName -eq "ComboBox") {
        # CB_GETCOUNT, CB_GETLBTEXT
        $h = [IntPtr]$c.NativeWindowHandle
        $count = [int][W]::SendMessage($h, 0x0146, [IntPtr]::Zero, [IntPtr]::Zero)
        for ($k = 0; $k -lt $count; $k++) { $sb = New-Object System.Text.StringBuilder 256; [W]::SendMessage($h, 0x0148, [IntPtr]$k, $sb) | Out-Null; $out += "    item $k [$sb]" }
      }
    } elseif ($action -eq "pick" -and $c.ClassName -eq "ComboBox") {
      # CB_SETCURSEL: the dialog reads the selection when OK is pressed
      [W]::SendMessage([IntPtr]$c.NativeWindowHandle, 0x014E, [IntPtr][int]$names[0], [IntPtr]::Zero) | Out-Null
      $out += "picked item $($names[0])"; $out | Set-Content $log -Encoding UTF8; exit 0
    } elseif ($action -eq "click" -and ($c.ClassName -eq "Button" -or $c.ControlType -eq [Windows.Automation.ControlType]::Button) -and $names -contains $n -and $c.IsEnabled) {
      [W]::PostMessage([IntPtr]$c.NativeWindowHandle, 0xF5, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
      $out += "clicked $n"; $out | Set-Content $log -Encoding UTF8; exit 0
    }
  }
}
if ($action -ne "dump") { $out += "not found: $names" }
$out | Set-Content $log -Encoding UTF8
