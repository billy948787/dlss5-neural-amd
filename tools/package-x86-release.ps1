# Package only freshly built binaries; third-party files are sidecars, never embedded.
param([switch]$Private,[string]$OutputPath='')
$ErrorActionPreference='Stop'
$root=Split-Path $PSScriptRoot -Parent
$release=Join-Path $root 'release'
foreach($name in @('dlss5-installer-x86.exe','files/dlss5-neural.addon32','files/dlss5-neural-host64.exe','payload.sha256')){
    if(!(Test-Path -LiteralPath (Join-Path $release $name))){throw "Run build-x86bridge.ps1 first; missing $name"}
}
$stage=Join-Path ([IO.Path]::GetTempPath()) ('dlss5-x86-release-'+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path (Join-Path $stage 'files') -Force | Out-Null
try {
    foreach($name in @('dlss5-installer-x86.exe','payload.sha256','files/dlss5-neural.addon32','files/dlss5-neural-host64.exe')){Copy-Item -LiteralPath (Join-Path $release $name) -Destination (Join-Path $stage $name)}
    Copy-Item -LiteralPath (Join-Path $root 'docs/x86bridge-install.md') -Destination (Join-Path $stage 'README.md')
    if($Private){
        foreach($name in @('dgVoodoo2_87_4.zip','files/dxgi.dll','files/dlssnr_amd_pass1.dll','files/dlssnr_on_amd_weights.bin')){
            if(!(Test-Path -LiteralPath (Join-Path $release $name))){throw "Missing private sidecar: $name"}
            Copy-Item -LiteralPath (Join-Path $release $name) -Destination (Join-Path $stage $name)
        }
        'PRIVATE LOCAL TEST PACKAGE - do not publish third-party payloads as public release.' | Set-Content (Join-Path $stage 'PRIVATE_TEST_ONLY.txt')
    }
    Get-ChildItem $stage -File -Recurse | Sort-Object FullName | ForEach-Object {"$((Get-FileHash $_.FullName -Algorithm SHA256).Hash)  $($_.FullName.Substring($stage.Length+1))"} | Set-Content (Join-Path $stage 'SHA256SUMS.txt')
    if(!$OutputPath){$OutputPath=Join-Path $root $(if($Private){'dlss5-x86bridge-private-test.zip'}else{'dlss5-x86bridge-public-release.zip'})}
    if(Test-Path -LiteralPath $OutputPath){throw 'Output already exists; choose a new output path.'}
    Compress-Archive -Path (Join-Path $stage '*') -DestinationPath $OutputPath
    Get-FileHash -LiteralPath $OutputPath -Algorithm SHA256
}finally{Remove-Item -LiteralPath $stage -Recurse -Force}
