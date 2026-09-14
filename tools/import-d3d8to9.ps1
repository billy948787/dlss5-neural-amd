param(
    [Parameter(Mandatory=$true)]
    [string]$SourcePath
)
$ErrorActionPreference='Stop'
$expectedSha='AB6BF7A9A9F4B3E66A75CA038D8D10289C88ACBFE8D52C3B5A8A9A259CB26CD5'
$source=(Resolve-Path -LiteralPath $SourcePath).Path
if((Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash -ne $expectedSha){
    throw 'The file is not the pinned official d3d8to9 v1.15.1 release asset.'
}
$bytes=[IO.File]::ReadAllBytes($source)
if($bytes.Length -lt 256 -or $bytes[0] -ne 0x4d -or $bytes[1] -ne 0x5a){throw 'd3d8to9 sidecar is not a PE image.'}
$peOffset=[BitConverter]::ToInt32($bytes,0x3c)
if($peOffset -lt 0 -or $peOffset + 6 -gt $bytes.Length -or
   $bytes[$peOffset] -ne 0x50 -or $bytes[$peOffset+1] -ne 0x45 -or
   [BitConverter]::ToUInt16($bytes,$peOffset+4) -ne 0x14c){
    throw 'd3d8to9 sidecar must be the x86/PE32 build.'
}
$root=Split-Path $PSScriptRoot -Parent
$destination=Join-Path $root 'release\files\d3d8to9.dll'
New-Item -ItemType Directory -Path (Split-Path $destination -Parent) -Force | Out-Null
Copy-Item -LiteralPath $source -Destination $destination -Force
Write-Host "Imported pinned d3d8to9 v1.15.1 sidecar: $destination"
