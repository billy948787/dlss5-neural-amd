# Package only freshly built binaries; third-party files are sidecars, never embedded.
# One archive serves both architectures: the 64-bit add-on is inside the installer, and the
# unpacked folder is what field 1 takes for either route.
param([switch]$Private,[string]$OutputPath='')
$ErrorActionPreference='Stop'
$root=Split-Path $PSScriptRoot -Parent
$release=Join-Path $root 'release'
# The bridge payloads come from build-x86bridge.ps1; the installer is built separately, in
# installer/. Name the right script for whichever is missing rather than sending someone to run
# the one that was never going to produce it.
foreach($name in @('files/dlss5-neural.addon32','files/dlss5-neural-host64.exe','payload.sha256')){
    if(!(Test-Path -LiteralPath (Join-Path $release $name))){throw "Run build-x86bridge.ps1 first; missing $name"}
}
if(!(Test-Path -LiteralPath (Join-Path $release 'dlss5-installer.exe'))){
    throw 'Run installer/build.ps1 and copy its dlss5-installer.exe into release/ first.'
}
$stage=Join-Path ([IO.Path]::GetTempPath()) ('dlss5-x86-release-'+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path (Join-Path $stage 'files') -Force | Out-Null
try {
    foreach($name in @('dlss5-installer.exe','payload.sha256','files/dlss5-neural.addon32','files/dlss5-neural-host64.exe')){Copy-Item -LiteralPath (Join-Path $release $name) -Destination (Join-Path $stage $name)}
    Copy-Item -LiteralPath (Join-Path $root 'docs/install.md') -Destination (Join-Path $stage 'README.md')
    # Shipped loose as well so a 64-bit install can be done by hand without the installer.
    $addon64=Join-Path $root 'build/dlss5-neural.addon64'
    if(!(Test-Path -LiteralPath $addon64)){throw 'Run ./build.ps1 -Target neural first; missing build/dlss5-neural.addon64'}
    Copy-Item -LiteralPath $addon64 -Destination (Join-Path $stage 'dlss5-neural.addon64')
    if($Private){
        foreach($name in @('files/dxgi.dll','files/dlssnr_amd_pass1.dll','files/dlssnr_on_amd_weights.bin')){
            if(!(Test-Path -LiteralPath (Join-Path $release $name))){throw "Missing private sidecar: $name"}
            Copy-Item -LiteralPath (Join-Path $release $name) -Destination (Join-Path $stage $name)
        }
        $d3d8to9=Join-Path $release 'files/d3d8to9.dll'
        if(Test-Path -LiteralPath $d3d8to9){
            Copy-Item -LiteralPath $d3d8to9 -Destination (Join-Path $stage 'files/d3d8to9.dll')
            New-Item -ItemType Directory -Path (Join-Path $stage 'third-party') -Force | Out-Null
            Copy-Item -LiteralPath (Join-Path $root 'docs/third-party/d3d8to9-LICENSE.md') -Destination (Join-Path $stage 'third-party/d3d8to9-LICENSE.md')
        }
        'PRIVATE LOCAL TEST PACKAGE - do not publish third-party payloads as public release.' | Set-Content (Join-Path $stage 'PRIVATE_TEST_ONLY.txt')
    }
    Get-ChildItem $stage -File -Recurse | Sort-Object FullName | ForEach-Object {"$((Get-FileHash $_.FullName -Algorithm SHA256).Hash)  $($_.FullName.Substring($stage.Length+1))"} | Set-Content (Join-Path $stage 'SHA256SUMS.txt')
    if(!$OutputPath){$OutputPath=Join-Path $root $(if($Private){'dlss5-neural-amd-private-test.zip'}else{'dlss5-neural-amd-release.zip'})}
    if(Test-Path -LiteralPath $OutputPath){throw 'Output already exists; choose a new output path.'}
    Compress-Archive -Path (Join-Path $stage '*') -DestinationPath $OutputPath
    Get-FileHash -LiteralPath $OutputPath -Algorithm SHA256
}finally{Remove-Item -LiteralPath $stage -Recurse -Force}
