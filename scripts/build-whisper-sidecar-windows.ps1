$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$upstreamCommit = "f049fff95a089aa9969deb009cdd4892b3e74916"
$upstreamArchiveSha256 = "279af4ce60dbf397362868f3bacc75b56a4332ac2541cae155070093f6aaf0e3"
$archiveUrl = "https://codeload.github.com/ggml-org/whisper.cpp/tar.gz/$upstreamCommit"
$repoRoot = Split-Path -Parent $PSScriptRoot
$binDir = Join-Path $repoRoot "src-tauri/binaries"
$destination = Join-Path $binDir "whisper-cli-x86_64-pc-windows-msvc.exe"
$workDir = Join-Path ([System.IO.Path]::GetTempPath()) ("finalsub-whisper-windows-" + [guid]::NewGuid())
$sourceDir = Join-Path $workDir "source"
$buildDir = Join-Path $workDir "build"
$backup = Join-Path $workDir "whisper.backup.exe"
$installStarted = $false

New-Item -ItemType Directory -Force -Path $workDir, $sourceDir, $binDir | Out-Null
try {
    $archive = Join-Path $workDir "whisper.cpp.tar.gz"
    Invoke-WebRequest -Uri $archiveUrl -OutFile $archive -MaximumRetryCount 5 -RetryIntervalSec 2
    $actual = (Get-FileHash -Algorithm SHA256 $archive).Hash.ToLowerInvariant()
    if ($actual -ne $upstreamArchiveSha256) {
        throw "whisper.cpp archive checksum mismatch: $actual"
    }
    & tar.exe -xzf $archive --strip-components=1 -C $sourceDir
    if ($LASTEXITCODE -ne 0) { throw "Failed to extract whisper.cpp" }

    & cmake -S $sourceDir -B $buildDir -A x64 `
        -DCMAKE_BUILD_TYPE=Release `
        -DCMAKE_MSVC_RUNTIME_LIBRARY=MultiThreaded `
        -DBUILD_SHARED_LIBS=OFF `
        -DGGML_STATIC=ON `
        -DGGML_NATIVE=OFF `
        -DGGML_OPENMP=OFF `
        -DGGML_BLAS=OFF `
        -DGGML_METAL=OFF `
        -DGGML_CUDA=OFF `
        -DGGML_VULKAN=OFF `
        -DWHISPER_COREML=OFF `
        -DWHISPER_CURL=OFF `
        -DWHISPER_COMMON_FFMPEG=OFF `
        -DWHISPER_BUILD_TESTS=OFF `
        -DWHISPER_BUILD_SERVER=OFF `
        -DWHISPER_BUILD_EXAMPLES=ON
    if ($LASTEXITCODE -ne 0) { throw "CMake configuration failed" }
    & cmake --build $buildDir --config Release --parallel --target whisper-cli
    if ($LASTEXITCODE -ne 0) { throw "whisper-cli build failed" }

    $sourceBinary = Get-ChildItem -Path $buildDir -Recurse -Filter whisper-cli.exe |
        Where-Object { $_.FullName -match '[\\/]bin[\\/]' } |
        Select-Object -First 1
    if (-not $sourceBinary) { throw "whisper-cli build output is missing" }
    if (Test-Path $destination) { Copy-Item $destination $backup }
    $installStarted = $true
    Copy-Item $sourceBinary.FullName "$destination.new" -Force
    Move-Item "$destination.new" $destination -Force
    $help = (& $destination --help 2>&1 | Out-String)
    if ($help -notmatch '(?m)^usage:') { throw "whisper-cli --help smoke test failed" }
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
