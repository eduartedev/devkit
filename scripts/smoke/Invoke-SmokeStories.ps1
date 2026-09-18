<#
.SYNOPSIS
  Smoke de las 3 historias de uso de DevKit (CLI).

.PARAMETER Story
  Start | Laravel | Stop | All

.PARAMETER DevKitExe
  Ruta a devkit.exe (si se omite, busca release/debug o compila).

.PARAMETER KeepSite
  Con -Story Laravel/All, no elimina el proyecto creado en www/.

.PARAMETER SiteName
  Nombre del sitio Laravel (solo a-z0-9-). Default: smoke-laravel-<ticks>
#>
[CmdletBinding()]
param(
    [ValidateSet("Start", "Laravel", "Stop", "All")]
    [string] $Story = "All",

    [string] $DevKitExe = "",

    [switch] $KeepSite,

    [string] $SiteName = ""
)

$ErrorActionPreference = "Stop"
$Root = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path

function Write-Step([string] $msg) {
    Write-Host ""
    Write-Host "==> $msg" -ForegroundColor Cyan
}

function Write-Ok([string] $msg) {
    Write-Host "OK  $msg" -ForegroundColor Green
}

function Write-Fail([string] $msg) {
    Write-Host "FAIL $msg" -ForegroundColor Red
}

function Resolve-DevKitExe {
    param([string] $Hint)
    if ($Hint -and (Test-Path $Hint)) {
        return (Resolve-Path $Hint).Path
    }
    $candidates = @(
        (Join-Path $Root "target\release\devkit.exe"),
        (Join-Path $Root "target\debug\devkit.exe"),
        (Join-Path $Root "devkit.exe")
    )
    foreach ($c in $candidates) {
        if (Test-Path $c) { return (Resolve-Path $c).Path }
    }
    Write-Step "No hay devkit.exe; compilando CLI (release)..."
    Push-Location $Root
    try {
        $env:CARGO_TARGET_DIR = Join-Path $Root "target"
        cargo build -p devkit-cli --release | Out-Host
    } finally {
        Pop-Location
    }
    $built = Join-Path $Root "target\release\devkit.exe"
    if (-not (Test-Path $built)) {
        throw "No se pudo obtener target\release\devkit.exe"
    }
    return $built
}

function Invoke-DevKit {
    param(
        [Parameter(Mandatory = $true)][string] $Exe,
        [Parameter(Mandatory = $true)][string[]] $DevArgs
    )
    Write-Host ("  > devkit " + ($DevArgs -join " ")) -ForegroundColor DarkGray
    # stderr de avisos (puertos, mysqladmin) no debe abortar el script
    $prev = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    $out = & $Exe @DevArgs 2>&1
    $code = $LASTEXITCODE
    $ErrorActionPreference = $prev
    foreach ($line in $out) {
        Write-Host $line
    }
    if ($code -ne 0) {
        throw "devkit $($DevArgs -join ' ') salio con codigo $code"
    }
}

function Clear-OrphanNginx {
    $httpPort = Get-HttpPortFromConfig
    Get-Process nginx -ErrorAction SilentlyContinue | ForEach-Object {
        try {
            $path = $_.Path
            if ($path -and $path.StartsWith($Root, [System.StringComparison]::OrdinalIgnoreCase)) {
                Write-Host "  Matando nginx huerfano PID $($_.Id) ($path)" -ForegroundColor DarkYellow
                Stop-Process -Id $_.Id -Force -ErrorAction SilentlyContinue
            }
        } catch { }
    }
    Start-Sleep -Seconds 1
    Remove-Item (Join-Path $Root "logs\nginx.pid") -Force -ErrorAction SilentlyContinue
    Remove-Item (Join-Path $Root "logs\nginx-master.pid") -Force -ErrorAction SilentlyContinue
    $null = $httpPort
}

function Get-StatusText([string] $Exe) {
    $out = & $Exe status 2>&1 | Out-String
    return $out
}

function Assert-ServiceRunning([string] $StatusText, [string] $IdHint) {
    $lines = $StatusText -split "`r?`n"
    $hit = $lines | Where-Object { $_.Contains("[running]") -and $_.Contains($IdHint) }
    if (-not $hit) {
        throw "Servicio no running (buscaba '$IdHint'). Status:`n$StatusText"
    }
}

function Assert-ServiceStopped([string] $StatusText, [string] $IdHint) {
    $lines = $StatusText -split "`r?`n"
    $running = $lines | Where-Object { $_.Contains("[running]") -and $_.Contains($IdHint) }
    if ($running) {
        throw "Servicio aun running (buscaba parado '$IdHint'). Status:`n$StatusText"
    }
}

function Get-HttpPortFromConfig {
    $cfg = Join-Path $Root "etc\devkit\config.toml"
    $port = 8080
    if (Test-Path $cfg) {
        $m = Select-String -Path $cfg -Pattern '^\s*http_port\s*=\s*(\d+)' | Select-Object -First 1
        if ($m) { $port = [int]$m.Matches[0].Groups[1].Value }
    }
    return $port
}

function Get-HttpsPortFromConfig {
    $cfg = Join-Path $Root "etc\devkit\config.toml"
    $port = 8443
    if (Test-Path $cfg) {
        $m = Select-String -Path $cfg -Pattern '^\s*https_port\s*=\s*(\d+)' | Select-Object -First 1
        if ($m) { $port = [int]$m.Matches[0].Groups[1].Value }
    }
    return $port
}

function Get-SslEnabled {
    $cfg = Join-Path $Root "etc\devkit\config.toml"
    if (Test-Path $cfg) {
        $m = Select-String -Path $cfg -Pattern '^\s*ssl\s*=\s*true' | Select-Object -First 1
        return [bool]$m
    }
    return $false
}

function Test-ValidSiteName([string] $Name) {
    if ([string]::IsNullOrWhiteSpace($Name)) { return $false }
    if ($Name.Length -gt 63) { return $false }
    if ($Name.StartsWith("-") -or $Name.EndsWith("-")) { return $false }
    foreach ($ch in $Name.ToCharArray()) {
        $ok = ($ch -ge "a"[0] -and $ch -le "z"[0]) -or
              ($ch -ge "0"[0] -and $ch -le "9"[0]) -or
              ($ch -eq "-"[0])
        if (-not $ok) { return $false }
    }
    return $true
}

function Test-HttpCodeOk([string] $Code) {
    return @("200", "301", "302") -contains $Code
}

function Test-HttpOk([string] $Url, [int] $TimeoutSec = 45) {
    $deadline = (Get-Date).AddSeconds($TimeoutSec)
    $last = ""
    while ((Get-Date) -lt $deadline) {
        try {
            $code = & curl.exe -s -k -o NUL -w "%{http_code}" --max-time 5 $Url
            $last = $code
            if (Test-HttpCodeOk $code) {
                return $code
            }
        } catch {
            $last = $_.Exception.Message
        }
        Start-Sleep -Seconds 2
    }
    throw "URL no respondio OK a tiempo: $Url (ultimo: $last)"
}

function Invoke-StoryStart([string] $Exe) {
    Write-Step "Historia 1 - Start All"
    try { Invoke-DevKit -Exe $Exe -DevArgs @("stop") } catch { Write-Host "  (stop previo omitido)" -ForegroundColor DarkGray }
    Clear-OrphanNginx
    Start-Sleep -Seconds 1
    Invoke-DevKit -Exe $Exe -DevArgs @("start")
    Start-Sleep -Seconds 2
    $st = Get-StatusText $Exe
    Write-Host $st
    Assert-ServiceRunning $st "Nginx"
    Assert-ServiceRunning $st "PHP"
    Assert-ServiceRunning $st "MariaDB"
    $port = Get-HttpPortFromConfig
    $code = Test-HttpOk "http://127.0.0.1:$port/"
    Write-Ok "Historia 1: stack up (localhost:$port -> HTTP $code)"
}

function Invoke-StoryLaravel([string] $Exe) {
    Write-Step "Historia 2 - Crear Laravel + abrir URL"
    $name = $SiteName
    if (-not $name) {
        $name = "smoke-laravel-" + [DateTimeOffset]::UtcNow.ToUnixTimeSeconds()
    }
    if (-not (Test-ValidSiteName $name)) {
        throw "SiteName invalido: $name (solo a-z, 0-9 y guion)"
    }
    $www = Join-Path $Root "www\$name"
    if (Test-Path $www) {
        throw "Ya existe $www - elige otro -SiteName o borralo"
    }

    $st0 = Get-StatusText $Exe
    if (-not $st0.Contains("[running]") -or -not $st0.Contains("Nginx")) {
        Write-Host "  Stack no arriba; ejecutando start..." -ForegroundColor DarkGray
        Invoke-DevKit -Exe $Exe -DevArgs @("start")
        Start-Sleep -Seconds 2
    }

    Write-Step "devkit create laravel $name (Composer; puede tardar varios minutos)"
    Invoke-DevKit -Exe $Exe -DevArgs @("create", "laravel", $name)

    if (-not (Test-Path (Join-Path $www "artisan"))) {
        throw "No se creo artisan en $www"
    }
    if (-not (Test-Path (Join-Path $www "public\index.php"))) {
        throw "Falta public\index.php en $www"
    }
    Write-Ok "Proyecto Laravel en $www"

    Invoke-DevKit -Exe $Exe -DevArgs @("reload")
    try { Invoke-DevKit -Exe $Exe -DevArgs @("start") } catch { }

    $port = Get-HttpPortFromConfig
    $httpsPort = Get-HttpsPortFromConfig
    $ssl = Get-SslEnabled
    $urlHttp = "http://${name}.test:${port}/"
    $urlHttps = "https://${name}.test:${httpsPort}/"
    $url = if ($ssl) { $urlHttps } else { $urlHttp }

    $probe = if ($ssl) {
        "https://127.0.0.1:${httpsPort}/"
    } else {
        "http://127.0.0.1:$port/"
    }
    Write-Step "Probe HTTP Host: ${name}.test"
    $code = $null
    $deadline = (Get-Date).AddSeconds(60)
    while ((Get-Date) -lt $deadline) {
        $curlArgs = @("-s", "-k", "-o", "NUL", "-w", "%{http_code}", "--max-time", "8", "-H", "Host: ${name}.test")
        $code = & curl.exe @curlArgs $probe
        if (Test-HttpCodeOk $code) { break }
        Start-Sleep -Seconds 2
    }
    if (-not (Test-HttpCodeOk $code)) {
        throw "Laravel no respondio OK (HTTP $code) Host=${name}.test -> $probe"
    }
    Write-Ok "HTTP $code para ${name}.test"

    Write-Step "Abriendo navegador: $url"
    Start-Process $url

    if (-not $KeepSite) {
        Write-Step "Limpiando sitio $name (-KeepSite para conservar)"
        Remove-Item -LiteralPath $www -Recurse -Force -ErrorAction Stop
        Invoke-DevKit -Exe $Exe -DevArgs @("reload")
        Write-Ok "Sitio eliminado"
    } else {
        Write-Ok "Sitio conservado: $www ($url)"
    }

    Write-Ok "Historia 2 completada"
}

function Invoke-StoryStop([string] $Exe) {
    Write-Step "Historia 3 - Stop All"
    Invoke-DevKit -Exe $Exe -DevArgs @("stop")
    Start-Sleep -Seconds 2
    $st = Get-StatusText $Exe
    Write-Host $st
    Assert-ServiceStopped $st "Nginx"
    Assert-ServiceStopped $st "PHP"
    Assert-ServiceStopped $st "MariaDB"
    $port = Get-HttpPortFromConfig
    try {
        $code = & curl.exe -s -o NUL -w "%{http_code}" --max-time 3 "http://127.0.0.1:$port/"
        if ($code -eq "200") {
            throw "Tras stop, :$port aun responde 200 (Nginx huerfano?)"
        }
    } catch {
        if ($_.Exception.Message -like "*huerfano*") { throw }
    }
    Write-Ok "Historia 3: servicios detenidos"
}

$exe = Resolve-DevKitExe -Hint $DevKitExe
Write-Host "Usando: $exe" -ForegroundColor DarkGray
Write-Host "Root:   $Root" -ForegroundColor DarkGray

try {
    switch ($Story) {
        "Start" { Invoke-StoryStart $exe }
        "Laravel" { Invoke-StoryLaravel $exe }
        "Stop" { Invoke-StoryStop $exe }
        "All" {
            Invoke-StoryStart $exe
            Invoke-StoryLaravel $exe
            Invoke-StoryStop $exe
        }
    }
    Write-Host ""
    Write-Host "SMOKE OK ($Story)" -ForegroundColor Green
    exit 0
} catch {
    Write-Fail $_.Exception.Message
    Write-Host ""
    Write-Host "SMOKE FAIL ($Story)" -ForegroundColor Red
    exit 1
}
