# Additive build only. Original build.ps1 is invoked solely on a temporary baseline copy.
param([string]$VsPath='', [string]$SdkPath='', [string]$SdkVersion='')
$ErrorActionPreference='Stop'
if([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT){throw 'Native Windows MSVC required; no substitute ABI/compiler.'}
$root=$PSScriptRoot
& (Join-Path $root 'tools/check-additive-x86.ps1')
# --- Visual Studio -----------------------------------------------------------------------
if (-not $VsPath) {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
    if (-not (Test-Path $vswhere)) {
        throw "vswhere.exe not found. Install Visual Studio with the C++ workload, or pass -VsPath."
    }
    $VsPath = & $vswhere -latest -prerelease -products * `
        -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath |
        Select-Object -First 1
}

# vswhere does not see every install -- Insiders/preview builds in particular. Fall back to
# scanning the standard locations for the tools themselves.
if (-not $VsPath -or -not (Test-Path (Join-Path $VsPath 'VC\Tools\MSVC'))) {
    $VsPath = @($env:ProgramFiles, ${env:ProgramFiles(x86)}) |
        Where-Object { $_ } |
        ForEach-Object { Join-Path $_ 'Microsoft Visual Studio' } |
        Where-Object { Test-Path $_ } |
        ForEach-Object { Get-ChildItem $_ -Directory -Recurse -Depth 1 -ErrorAction SilentlyContinue } |
        Where-Object { Test-Path (Join-Path $_.FullName 'VC\Tools\MSVC') } |
        Sort-Object FullName -Descending |
        Select-Object -First 1 -ExpandProperty FullName
}
if (-not $VsPath) {
    throw "No Visual Studio install with the C++ tools was found. Pass -VsPath."
}

$msvcRoot = Join-Path $VsPath 'VC\Tools\MSVC'
if (-not (Test-Path $msvcRoot)) { throw "MSVC tools not found under $msvcRoot" }
$msvc = Get-ChildItem $msvcRoot -Directory | Sort-Object Name -Descending | Select-Object -First 1
$cl = Join-Path $msvc.FullName 'bin\Hostx64\x64\cl.exe'
if (-not (Test-Path $cl)) { throw "cl.exe not found at $cl" }

# --- Windows SDK -------------------------------------------------------------------------
if (-not $SdkPath) {
    foreach ($key in 'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Microsoft SDKs\Windows\v10.0',
                     'HKLM:\SOFTWARE\Microsoft\Microsoft SDKs\Windows\v10.0') {
        try { $SdkPath = (Get-ItemProperty $key -ErrorAction Stop).InstallationFolder } catch {}
        if ($SdkPath) { break }
    }
}
if (-not $SdkPath -or -not (Test-Path $SdkPath)) {
    throw "Windows 10/11 SDK not found in the registry. Pass -SdkPath."
}

if (-not $SdkVersion) {
    # Newest version that actually has the headers we need.
    $SdkVersion = Get-ChildItem (Join-Path $SdkPath 'Include') -Directory -ErrorAction SilentlyContinue |
        Where-Object { Test-Path (Join-Path $_.FullName 'um\windows.h') } |
        Sort-Object Name -Descending | Select-Object -First 1 -ExpandProperty Name
}
if (-not $SdkVersion) { throw "No usable SDK version found under $SdkPath\Include. Pass -SdkVersion." }

Write-Host "MSVC $($msvc.Name)"
Write-Host "SDK  $SdkVersion  ($SdkPath)"


$out=Join-Path $root 'build-x86bridge'
New-Item -ItemType Directory -Force -Path $out | Out-Null
$env:INCLUDE=@((Join-Path $msvc.FullName 'include'),(Join-Path $SdkPath "Include\$SdkVersion\ucrt"),(Join-Path $SdkPath "Include\$SdkVersion\um"),(Join-Path $SdkPath "Include\$SdkVersion\shared"),(Join-Path $SdkPath "Include\$SdkVersion\winrt"),(Join-Path $root 'external\reshade')) -join ';'
$flags=@('/nologo','/utf-8','/std:c++20','/EHsc','/O2','/MT','/W3','/DUNICODE','/D_UNICODE','/D_CRT_SECURE_NO_WARNINGS','/DNOMINMAX','/DWIN32_LEAN_AND_MEAN')
function PE([string]$p,[int]$machine){
    $b=[IO.File]::ReadAllBytes($p);$e=[BitConverter]::ToInt32($b,0x3c)
    if([BitConverter]::ToUInt32($b,$e) -ne 0x4550 -or [BitConverter]::ToUInt16($b,$e+4) -ne $machine){throw "Wrong PE machine: $p"}
}
Push-Location $out
try {
    foreach($arch in @('x86','x64')){
        $cl=Join-Path $msvc.FullName "bin\Hostx64\$arch\cl.exe"
        if(!(Test-Path -LiteralPath $cl)){throw "Missing native compiler: $cl"}
        $env:LIB=@((Join-Path $msvc.FullName "lib\$arch"),(Join-Path $SdkPath "Lib\$SdkVersion\ucrt\$arch"),(Join-Path $SdkPath "Lib\$SdkVersion\um\$arch")) -join ';'
        $proto=Join-Path $out "protocol-$arch.exe"
        & $cl @flags (Join-Path $root 'src/x86bridge/protocol_test.cpp') "/Fo$out\protocol-$arch.obj" /link "/OUT:$proto" 2>&1 | Tee-Object -FilePath (Join-Path $out "protocol-build-$arch.log")
        if($LASTEXITCODE -ne 0){throw "Protocol compile failed: $arch"}
        & $proto | Tee-Object -FilePath (Join-Path $out "protocol-test-$arch.log")
        if($LASTEXITCODE -ne 0){throw "Protocol test failed: $arch"}
        if($arch -eq 'x86'){
            $binary=Join-Path $out 'dlss5-neural.addon32'
            & $cl @flags /LD (Join-Path $root 'src/x86bridge/frontend32.cpp') "/Fo$out\frontend32.obj" /link /DLL "/OUT:$binary" user32.lib d3d11.lib dxgi.lib d3dcompiler.lib 2>&1 | Tee-Object -FilePath (Join-Path $out 'build-x86.log')
        }else{
            $binary=Join-Path $out 'dlss5-neural-host64.exe'
            & $cl @flags (Join-Path $root 'src/x86bridge/host64.cpp') "/Fo$out\host64.obj" /link "/OUT:$binary" user32.lib d3d11.lib d3d12.lib dxgi.lib d3dcompiler.lib bcrypt.lib 2>&1 | Tee-Object -FilePath (Join-Path $out 'build-x64.log')
        }
        if($LASTEXITCODE -ne 0){throw "Compile failed: $arch"}
        PE $binary $(if($arch -eq 'x86'){0x14c}else{0x8664})
        $dumpbin=Join-Path (Split-Path $cl -Parent) 'dumpbin.exe'
        $imports=& $dumpbin /imports $binary 2>&1
        if($LASTEXITCODE -ne 0){throw 'dumpbin failed'}
        $imports | Set-Content -LiteralPath (Join-Path $out "imports-$arch.txt")
        $text=$imports -join "`n"
        if($text -match '(?i)(VCRUNTIME|MSVCP|libgcc|libstdc\+\+)'){throw "Unexpected C++ DLL dependency: $arch"}
        if($arch -eq 'x86' -and $text -match '(?i)(d3d12\.dll|amdhip64_7\.dll|dlssnr_amd_pass1\.dll)'){throw 'Forbidden frontend import'}
    }
    # New installer stays outside the immutable upstream installer/ directory.
    $installer=Join-Path $out 'dlss5-installer-x86.exe'
    & $cl @flags (Join-Path $root 'installer-x86/main.cpp') "/Fo$out\installer-x86.obj" /link /SUBSYSTEM:WINDOWS "/OUT:$installer" user32.lib comdlg32.lib gdi32.lib bcrypt.lib 2>&1 | Tee-Object -FilePath (Join-Path $out 'installer-build.log')
    if($LASTEXITCODE -ne 0){throw 'x86-route installer compile failed'}
    PE $installer 0x8664
    $installerTest=Join-Path $out 'installer-tests.exe'
    & $cl @flags (Join-Path $root 'installer-x86/tests.cpp') "/Fo$out\installer-tests.obj" /link "/OUT:$installerTest" bcrypt.lib 2>&1 | Tee-Object -FilePath (Join-Path $out 'installer-tests-build.log')
    if($LASTEXITCODE -ne 0){throw 'Installer tests compile failed'}
    $release=Join-Path $root 'release'
    New-Item -ItemType Directory -Force -Path (Join-Path $release 'files') | Out-Null
    Copy-Item -LiteralPath $installer -Destination $release -Force
    foreach($name in @('dlss5-neural.addon32','dlss5-neural-host64.exe')){
        Copy-Item -LiteralPath (Join-Path $out $name) -Destination (Join-Path $release 'files') -Force
    }
    @('dlss5-neural.addon32','dlss5-neural-host64.exe') | ForEach-Object {"$((Get-FileHash -LiteralPath (Join-Path $release "files/$_") -Algorithm SHA256).Hash.ToLowerInvariant())  $_"} | Set-Content -Encoding ASCII -LiteralPath (Join-Path $release 'payload.sha256')
    $canTest=(Test-Path (Join-Path $release 'dgVoodoo2_87_4.zip')) -and (Test-Path (Join-Path $release 'files/dxgi.dll')) -and (Test-Path (Join-Path $release 'files/dlssnr_amd_pass1.dll')) -and (Test-Path (Join-Path $release 'files/dlssnr_on_amd_weights.bin'))
    if($canTest){
        & $installerTest $release | Tee-Object -FilePath (Join-Path $out 'installer-tests.log')
        if($LASTEXITCODE -ne 0){throw 'Installer tests failed'}
    }else{ 'UNVALIDATED installer payload tests: supply pinned private sidecars in release and rerun build or installer-tests.exe release.' | Set-Content (Join-Path $out 'installer-tests.log') }
    # Build untouched addon64 using only the original files and original script in TEMP.
    $baseline=Join-Path ([IO.Path]::GetTempPath()) ('dlss5-upstream-baseline-'+[guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $baseline | Out-Null
    foreach($line in Get-Content -LiteralPath (Join-Path $root 'docs/x86bridge-baseline.sha256')){
        $hash,$rel=$line -split '  ',2;$dst=Join-Path $baseline $rel
        New-Item -ItemType Directory -Force -Path (Split-Path $dst -Parent) | Out-Null
        Copy-Item -LiteralPath (Join-Path $root $rel) -Destination $dst
    }
    & (Join-Path $baseline 'build.ps1') -Target neural -VsPath $VsPath -SdkPath $SdkPath -SdkVersion $SdkVersion 2>&1 | Tee-Object -FilePath (Join-Path $out 'original-addon64-build.log')
    if(!$?){throw 'Original addon64 baseline build failed'}
    $original=Join-Path $baseline 'build/dlss5-neural.addon64';if(!(Test-Path -LiteralPath $original)){throw 'Original addon64 output missing'}
    PE $original 0x8664
    "Original build tested in: $baseline" | Set-Content -LiteralPath (Join-Path $out 'original-baseline-location.txt')
    & (Join-Path $root 'tools/check-additive-x86.ps1') -ReportPath (Join-Path $out 'preservation.json')
    Get-ChildItem -LiteralPath $out -File | Where-Object {$_.Name -ne 'SHA256SUMS.txt'} | Sort-Object Name | ForEach-Object {"$((Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash)  $($_.Name)"} | Set-Content -LiteralPath (Join-Path $out 'SHA256SUMS.txt')
    Write-Host 'PASS native x86/x64 builds, protocol tests, PE, imports, original addon64 temporary build, preservation. GPU/game tests still UNVALIDATED.'
    Write-Host "Outputs: $out"
} finally {Pop-Location}
