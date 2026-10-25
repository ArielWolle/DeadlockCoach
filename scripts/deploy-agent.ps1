# Deploys the Agent to the gaming PC over SSH and (re)starts it in the user's
# interactive desktop session via a scheduled task (SSH sessions can't capture the screen).
param(
  [string]$Target = "ariel@192.168.2.17",
  [Parameter(Mandatory)][string]$Token
)
$ErrorActionPreference = "Continue"
$root = Split-Path $PSScriptRoot -Parent
Push-Location $root; cargo build --release -p coach-agent; Pop-Location
$exe = Join-Path $root "target\release\coach-agent.exe"

ssh $Target "if not exist %USERPROFILE%\DeadlockCoach mkdir %USERPROFILE%\DeadlockCoach & taskkill /f /im coach-agent.exe 2>nul & exit 0"
scp $exe "${Target}:DeadlockCoach/coach-agent.exe"

$remote = @"
`$exe = Join-Path `$env:USERPROFILE 'DeadlockCoach\coach-agent.exe'
`$cmd = "```$env:COACH_TOKEN='$Token'; & '`$exe'"
`$a = New-ScheduledTaskAction -Execute 'powershell.exe' -Argument "-WindowStyle Hidden -NoProfile -Command `$cmd"
`$t = New-ScheduledTaskTrigger -AtLogOn -User `$env:USERNAME
`$p = New-ScheduledTaskPrincipal -UserId `$env:USERNAME -LogonType Interactive
Register-ScheduledTask -TaskName DeadlockCoachAgent -Action `$a -Trigger `$t -Principal `$p -Force | Out-Null
Start-ScheduledTask -TaskName DeadlockCoachAgent
try { New-NetFirewallRule -DisplayName DeadlockCoachAgent -Direction Inbound -Protocol TCP -LocalPort 7878 -Action Allow -Profile Private -ErrorAction Stop | Out-Null } catch { Write-Warning "Firewall rule not added (needs admin): `$_" }
Get-ScheduledTask DeadlockCoachAgent | Select TaskName,State
"@
$tmp = New-TemporaryFile
Set-Content $tmp $remote
scp $tmp "${Target}:dc-install.ps1"
ssh $Target "powershell -NoProfile -ExecutionPolicy Bypass -File dc-install.ps1"


