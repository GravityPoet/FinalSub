param(
    [Parameter(Position = 0)]
    [string]$TargetRoot,
    [switch]$RequireAuthenticode
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$repoRoot = Split-Path -Parent $PSScriptRoot
$targetRoot = if (-not [string]::IsNullOrWhiteSpace($TargetRoot)) {
    $TargetRoot
} else {
    Join-Path $repoRoot "src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis"
}
$installDir = Join-Path $env:RUNNER_TEMP ("FinalSub-package-verify-" + [guid]::NewGuid())
$uninstaller = $null
$expectedThumbprint = ([string][Environment]::GetEnvironmentVariable("WINDOWS_CERTIFICATE_THUMBPRINT") -replace '\s', '').ToUpperInvariant()

if ($RequireAuthenticode -and $expectedThumbprint -notmatch '^[A-F0-9]{40}$') {
    throw "WINDOWS_CERTIFICATE_THUMBPRINT is required for signed package verification"
}

function Assert-FinalSubAuthenticode {
    param(
        [Parameter(Mandatory = $true)]
        [System.IO.FileInfo]$File,
        [Parameter(Mandatory = $true)]
        [string]$Label
    )

    $signature = Get-AuthenticodeSignature -FilePath $File.FullName
    if ($signature.Status -ne "Valid") {
        throw "$Label Authenticode status is $($signature.Status), expected Valid"
    }
    if ($null -eq $signature.SignerCertificate) {
        throw "$Label is missing its signer certificate"
    }
    $actualThumbprint = ($signature.SignerCertificate.Thumbprint -replace '\s', '').ToUpperInvariant()
    if ($actualThumbprint -ne $expectedThumbprint) {
        throw "$Label was not signed by the imported release certificate"
    }
    if ($null -eq $signature.TimeStamperCertificate) {
        throw "$Label is signed but is missing an Authenticode timestamp"
    }
}

try {
    $installers = @(Get-ChildItem -Path $targetRoot -File -Filter "*.exe")
    if ($installers.Count -ne 1) {
        throw "Expected exactly one NSIS installer below $targetRoot, found $($installers.Count)"
    }
    $installer = $installers[0]
    $signature = Get-AuthenticodeSignature -FilePath $installer.FullName
    Add-Content -Path $env:GITHUB_STEP_SUMMARY -Value "Windows installer Authenticode status: ``$($signature.Status)``"
    if ($RequireAuthenticode) {
        Assert-FinalSubAuthenticode -File $installer -Label "Windows installer"
    }

    $install = Start-Process -FilePath $installer.FullName `
        -ArgumentList @("/S", "/D=$installDir") `
        -Wait `
        -PassThru
    if ($install.ExitCode -ne 0) {
        throw "NSIS silent install failed with exit code $($install.ExitCode)"
    }

    $appCandidates = @(Get-ChildItem -Path $installDir -Recurse -File -Filter "*.exe" |
        Where-Object { $_.Name -notmatch '(?i)uninstall|ffmpeg|whisper' })
    if ($appCandidates.Count -ne 1) {
        throw "Expected one installed FinalSub executable, found $($appCandidates.Count)"
    }
    $app = $appCandidates[0]
    if ($RequireAuthenticode) {
        $ffmpegCandidates = @(Get-ChildItem -Path $installDir -Recurse -File -Filter "ffmpeg.exe")
        $whisperCandidates = @(Get-ChildItem -Path $installDir -Recurse -File -Filter "whisper-cli.exe")
        if ($ffmpegCandidates.Count -ne 1) {
            throw "Expected one installed FFmpeg sidecar, found $($ffmpegCandidates.Count)"
        }
        if ($whisperCandidates.Count -ne 1) {
            throw "Expected one installed Whisper sidecar, found $($whisperCandidates.Count)"
        }
        Assert-FinalSubAuthenticode -File $app -Label "FinalSub executable"
        Assert-FinalSubAuthenticode -File $ffmpegCandidates[0] -Label "FFmpeg sidecar"
        Assert-FinalSubAuthenticode -File $whisperCandidates[0] -Label "Whisper sidecar"
    }
    $process = Start-Process -FilePath $app.FullName -PassThru
    Start-Sleep -Seconds 10
    if ($process.HasExited) {
        throw "Installed FinalSub exited during the 10 second startup smoke (code $($process.ExitCode))"
    }
    Stop-Process -Id $process.Id -Force
    Wait-Process -Id $process.Id -ErrorAction SilentlyContinue

    $uninstallers = @(Get-ChildItem -Path $installDir -Recurse -File -Filter "*.exe" |
        Where-Object { $_.Name -match '(?i)uninstall|unins' })
    if ($uninstallers.Count -ne 1) {
        throw "Expected one NSIS uninstaller, found $($uninstallers.Count)"
    }
    $uninstaller = $uninstallers[0]
    $uninstall = Start-Process -FilePath $uninstaller.FullName -ArgumentList "/S" -Wait -PassThru
    if ($uninstall.ExitCode -ne 0) {
        throw "NSIS silent uninstall failed with exit code $($uninstall.ExitCode)"
    }
    Start-Sleep -Seconds 2
    if (Test-Path $app.FullName) {
        throw "FinalSub executable remains after NSIS uninstall"
    }
    $uninstaller = $null

    $hash = (Get-FileHash -Algorithm SHA256 -Path $installer.FullName).Hash.ToLowerInvariant()
    "$hash  $($installer.Name)" | Set-Content -NoNewline -Path "$($installer.FullName).sha256"
    if ($RequireAuthenticode) {
        Add-Content -Path $env:GITHUB_STEP_SUMMARY -Value "Windows installer, app, FFmpeg, and Whisper signatures: ``Valid`` with one timestamped certificate."
    }
    Write-Output "Verified Windows NSIS install, 10 second startup, uninstall, and SHA-256 file."
}
finally {
    if ($null -ne $uninstaller -and (Test-Path $uninstaller.FullName)) {
        Start-Process -FilePath $uninstaller.FullName -ArgumentList "/S" -Wait -ErrorAction SilentlyContinue
    }
    if (Test-Path $installDir) {
        Remove-Item -Path $installDir -Recurse -Force -ErrorAction SilentlyContinue
    }
}
