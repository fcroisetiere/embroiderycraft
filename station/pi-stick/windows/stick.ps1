# Design stick, from Windows: send designs to the Pi over the network.
# Uses the ssh and scp that come with Windows 10 and 11. The launchers (.bat) call it:
#   stick.ps1 load FILES | add FILES | status | clear | setup | sendto
param([string]$Action = 'status', [Parameter(ValueFromRemainingArguments = $true)][string[]]$Files)
$ErrorActionPreference = 'Stop'

# which stick: first line of stick.txt next to this script, e.g. shop@ecstick1.local
$conf = Join-Path $PSScriptRoot 'stick.txt'
$target = 'shop@ecstick1.local'
if (Test-Path $conf) {
    $line = (Get-Content $conf | Where-Object { $_ -match '\S' -and $_ -notmatch '^\s*#' } | Select-Object -First 1)
    if ($line) { $target = $line.Trim() }
}
$key = Join-Path $env:USERPROFILE '.ssh\id_ed25519'
$opts = @('-o', 'StrictHostKeyChecking=accept-new', '-o', 'ConnectTimeout=10', '-i', $key)

function Fail($msg) { Write-Host ''; Write-Host $msg -ForegroundColor Red; exit 1 }
# exit code 255 is ssh's own (no connection); anything else comes from the stick
function Ssh {
    & ssh.exe @opts $target @args
    if ($LASTEXITCODE -eq 255) { Fail "The stick did not answer ($target)." }
    if ($LASTEXITCODE -ne 0) { Fail 'The stick reported a problem (see above). Nothing was sent to the machine wrongly; try again or check the names.' }
}

if (-not (Get-Command ssh.exe -ErrorAction SilentlyContinue)) {
    Fail 'ssh is missing: Settings > System > Optional features > add "OpenSSH Client".'
}

# a key, so the password is asked only once, the first time
function Setup {
    $dir = Split-Path $key
    if (-not (Test-Path $dir)) { New-Item -ItemType Directory $dir | Out-Null }
    if (-not (Test-Path $key)) {
        Start-Process ssh-keygen.exe -ArgumentList "-q -t ed25519 -N `"`" -f `"$key`"" -NoNewWindow -Wait
    }
    $pub = (Get-Content "$key.pub" -Raw).Trim()
    Write-Host "Connecting to $target. Type the Pi's password (it does not show)."
    & ssh.exe -o StrictHostKeyChecking=accept-new -o ConnectTimeout=10 $target "umask 077; mkdir -p ~/.ssh; grep -qxF '$pub' ~/.ssh/authorized_keys 2>/dev/null || echo '$pub' >> ~/.ssh/authorized_keys"
    if ($LASTEXITCODE -ne 0) { Fail "Could not connect to $target (is the Pi on, and on this network?)" }
    Write-Host 'Done: no more password needed from this PC.' -ForegroundColor Green
}

function Ready {
    if (Test-Path $key) {
        & ssh.exe @opts -o BatchMode=yes $target true 2>$null
        if ($LASTEXITCODE -eq 0) { return }
    }
    Setup
}

switch ($Action) {
    'setup' { Setup }
    'status' { Ready; Ssh 'ecstick status' }
    'clear' { Ready; Ssh 'sudo ecstick clear' }
    'sendto' {
        # right-click a design > Send to > Design stick
        $link = Join-Path ([Environment]::GetFolderPath('SendTo')) 'Design stick.lnk'
        $s = (New-Object -ComObject WScript.Shell).CreateShortcut($link)
        $s.TargetPath = Join-Path $PSScriptRoot 'Send to stick.bat'
        $s.WorkingDirectory = $PSScriptRoot
        $s.Save()
        Write-Host 'Added: right-click designs > Send to > Design stick.' -ForegroundColor Green
    }
    { $_ -in 'load', 'add' } {
        if (-not $Files) { Fail 'No designs: drag design files onto the launcher.' }
        foreach ($f in $Files) { if (-not (Test-Path -LiteralPath $f -PathType Leaf)) { Fail "Not a file: $f" } }
        Ready
        # copy into a new folder on the Pi, then load from it in one go
        $dir = (& ssh.exe @opts $target 'mktemp -d').Trim()
        if ($LASTEXITCODE -ne 0 -or -not $dir) { Fail "The stick did not answer ($target)." }
        & scp.exe -q @opts $Files "${target}:$dir/"
        if ($LASTEXITCODE -ne 0) { Fail 'Copying to the stick failed.' }
        Ssh "sudo ecstick $Action $dir/*; s=`$?; rm -rf $dir; exit `$s"
        Write-Host ''
        Write-Host 'Done. On the machine, open the USB menu again to see the designs.' -ForegroundColor Green
    }
    default { Fail "Unknown action: $Action" }
}
