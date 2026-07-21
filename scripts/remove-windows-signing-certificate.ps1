$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

function Remove-CertificateByThumbprint {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Store,
        [ValidateSet("CurrentUser", "LocalMachine")]
        [string]$Location = "CurrentUser",
        [string]$Thumbprint
    )

    if ([string]::IsNullOrWhiteSpace($Thumbprint)) {
        return
    }
    $normalized = ($Thumbprint -replace '\s', '').ToUpperInvariant()
    if ($normalized -notmatch '^[A-F0-9]{40}$') {
        throw "Refusing to remove a certificate with an invalid thumbprint"
    }
    $path = "Cert:\$Location\$Store\$normalized"
    if (Test-Path $path) {
        Remove-Item -Path $path -Force
        Write-Output "Removed the ephemeral certificate from Cert:\$Location\$Store."
    }
}

Remove-CertificateByThumbprint `
    -Store "My" `
    -Thumbprint ([Environment]::GetEnvironmentVariable("WINDOWS_CERTIFICATE_THUMBPRINT"))
Remove-CertificateByThumbprint `
    -Store "Root" `
    -Location "LocalMachine" `
    -Thumbprint ([Environment]::GetEnvironmentVariable("WINDOWS_TEST_ROOT_THUMBPRINT"))
