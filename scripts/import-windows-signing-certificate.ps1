$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

foreach ($name in @("WINDOWS_CERTIFICATE", "WINDOWS_CERTIFICATE_PASSWORD", "WINDOWS_TIMESTAMP_URL", "GITHUB_ENV", "RUNNER_TEMP")) {
    $value = [Environment]::GetEnvironmentVariable($name)
    if ([string]::IsNullOrWhiteSpace($value)) {
        throw "$name is required for Windows release signing"
    }
}

$timestampUrl = $null
if (-not [Uri]::TryCreate($env:WINDOWS_TIMESTAMP_URL.Trim(), [UriKind]::Absolute, [ref]$timestampUrl)) {
    throw "WINDOWS_TIMESTAMP_URL must be an absolute URL"
}
if ($timestampUrl.Scheme -notin @("http", "https")) {
    throw "WINDOWS_TIMESTAMP_URL must use HTTP or HTTPS"
}
if (-not [string]::IsNullOrEmpty($timestampUrl.UserInfo) -or -not [string]::IsNullOrEmpty($timestampUrl.Fragment)) {
    throw "WINDOWS_TIMESTAMP_URL must not contain credentials or a fragment"
}

$workDir = Join-Path $env:RUNNER_TEMP ("finalsub-windows-signing-" + [guid]::NewGuid())
$encodedPath = Join-Path $workDir "certificate.txt"
$pfxPath = Join-Path $workDir "certificate.pfx"
$importedCertificates = @()

try {
    New-Item -ItemType Directory -Path $workDir | Out-Null
    Set-Content -Path $encodedPath -Value $env:WINDOWS_CERTIFICATE -Encoding Ascii -NoNewline
    & certutil.exe -f -decode $encodedPath $pfxPath | Out-Null
    if ($LASTEXITCODE -ne 0 -or -not (Test-Path $pfxPath)) {
        throw "Failed to decode WINDOWS_CERTIFICATE as a PFX file"
    }

    $password = ConvertTo-SecureString -String $env:WINDOWS_CERTIFICATE_PASSWORD -Force -AsPlainText
    $importedCertificates = @(Import-PfxCertificate `
        -FilePath $pfxPath `
        -CertStoreLocation "Cert:\CurrentUser\My" `
        -Password $password)
    $codeSigningOid = "1.3.6.1.5.5.7.3.3"
    $signingCertificates = @($importedCertificates | Where-Object {
        $_.HasPrivateKey -and $codeSigningOid -in @($_.EnhancedKeyUsageList.ObjectId.Value)
    })
    if ($signingCertificates.Count -ne 1) {
        throw "The PFX must contain exactly one private code-signing certificate"
    }

    $certificate = $signingCertificates[0]
    if ($certificate.NotAfter.ToUniversalTime() -le (Get-Date).ToUniversalTime().AddDays(30)) {
        throw "The Windows code-signing certificate expires within 30 days"
    }
    $thumbprint = ($certificate.Thumbprint -replace '\s', '').ToUpperInvariant()
    if ($thumbprint -notmatch '^[A-F0-9]{40}$') {
        throw "The imported certificate did not produce a 40-character SHA-1 thumbprint"
    }

    Add-Content -Path $env:GITHUB_ENV -Value "WINDOWS_CERTIFICATE_THUMBPRINT=$thumbprint"
    Add-Content -Path $env:GITHUB_ENV -Value "WINDOWS_TIMESTAMP_URL=$($timestampUrl.AbsoluteUri)"
    Write-Output "Imported one Windows code-signing certificate into the ephemeral runner store."
}
catch {
    foreach ($certificate in $importedCertificates) {
        if ($null -ne $certificate.Thumbprint) {
            $path = "Cert:\CurrentUser\My\$($certificate.Thumbprint)"
            if (Test-Path $path) {
                Remove-Item -Path $path -Force
            }
        }
    }
    throw
}
finally {
    if (Test-Path $workDir) {
        Remove-Item -Path $workDir -Recurse -Force
    }
    Add-Content -Path $env:GITHUB_ENV -Value "WINDOWS_CERTIFICATE="
    Add-Content -Path $env:GITHUB_ENV -Value "WINDOWS_CERTIFICATE_PASSWORD="
}
