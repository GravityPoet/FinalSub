$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$archiveName = "ffmpeg-n7.1.5-12-g1fdbca85aa-win64-gpl-7.1.zip"
$archiveSha256 = "6d55c3fdb589f75d69d75fb59b90b3ca620e5e7a5c534dc78434310c8288cc6d"
$archiveUrl = "https://github.com/BtbN/FFmpeg-Builds/releases/download/autobuild-2026-08-14-13-16/$archiveName"
$repoRoot = Split-Path -Parent $PSScriptRoot
$binDir = Join-Path $repoRoot "src-tauri/binaries"
$destination = Join-Path $binDir "ffmpeg-x86_64-pc-windows-msvc.exe"
$workDir = Join-Path ([System.IO.Path]::GetTempPath()) ("finalsub-ffmpeg-windows-" + [guid]::NewGuid())
$backup = Join-Path $workDir "ffmpeg.backup.exe"
$installStarted = $false

New-Item -ItemType Directory -Force -Path $workDir, $binDir | Out-Null
try {
    $archive = Join-Path $workDir $archiveName
    Invoke-WebRequest -Uri $archiveUrl -OutFile $archive -MaximumRetryCount 5 -RetryIntervalSec 2
    $actual = (Get-FileHash -Algorithm SHA256 $archive).Hash.ToLowerInvariant()
    if ($actual -ne $archiveSha256) {
        throw "FFmpeg archive checksum mismatch: $actual"
    }
    Expand-Archive -Path $archive -DestinationPath $workDir -Force
    $sourceBinary = Get-ChildItem -Path $workDir -Recurse -Filter ffmpeg.exe |
        Where-Object { $_.FullName -match '[\\/]bin[\\/]ffmpeg\.exe$' } |
        Select-Object -First 1
    if (-not $sourceBinary) { throw "FFmpeg archive did not contain bin/ffmpeg.exe" }

    $buildconf = (& $sourceBinary.FullName -hide_banner -buildconf 2>&1 | Out-String)
    if ($buildconf.Contains("--enable-nonfree")) { throw "Refusing to bundle a nonfree FFmpeg build" }
    $filters = (& $sourceBinary.FullName -hide_banner -filters 2>&1 | Out-String)
    if ($filters -notmatch '\ssubtitles\s') { throw "FFmpeg build lacks the subtitles filter" }
    $encoders = (& $sourceBinary.FullName -hide_banner -encoders 2>&1 | Out-String)
    if ($encoders -notmatch '\slibx264\s') { throw "FFmpeg build lacks the libx264 encoder" }

    if (Test-Path $destination) { Copy-Item $destination $backup }
    $installStarted = $true
    Copy-Item $sourceBinary.FullName "$destination.new" -Force
    Move-Item "$destination.new" $destination -Force
    & $destination -version | Select-Object -First 1
    (Get-FileHash -Algorithm SHA256 $destination).Hash.ToLowerInvariant()
}
catch {
    if ($installStarted) {
        if (Test-Path $backup) { Copy-Item $backup $destination -Force }
        elseif (Test-Path $destination) { Remove-Item $destination -Force }
    }
    throw
}
finally {
    Remove-Item $workDir -Recurse -Force -ErrorAction SilentlyContinue
}
