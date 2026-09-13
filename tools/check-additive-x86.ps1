param([string]$Root=(Split-Path $PSScriptRoot -Parent),[string]$ReportPath='')
$ErrorActionPreference='Stop'
$Root=(Resolve-Path -LiteralPath $Root).Path
$baseline=Join-Path $Root 'docs/x86bridge-baseline.sha256'
$modified=@();$deleted=@();$before=@{};$after=@{}
foreach($line in Get-Content -LiteralPath $baseline){
    $hash,$rel=$line -split '  ',2
    if(!$rel -or $hash -notmatch '^[a-fA-F0-9]{64}$' -or $rel.Contains('..')){throw 'Invalid baseline entry'}
    $before[$rel]=$hash
    $p=Join-Path $Root $rel
    if(!(Test-Path -LiteralPath $p -PathType Leaf)){$deleted+=$rel;continue}
    $after[$rel]=(Get-FileHash -LiteralPath $p -Algorithm SHA256).Hash.ToLowerInvariant()
    if($after[$rel] -ne $hash){$modified+=$rel}
}
$new=@(Get-ChildItem -LiteralPath $Root -File -Recurse -Force | ForEach-Object {$_.FullName.Substring($Root.TrimEnd('\','/').Length+1).Replace('\','/')} | Where-Object {!$before.ContainsKey($_)} | Sort-Object)
$result=[ordered]@{original_files=$before.Count;modified_existing_files=$modified.Count;deleted_existing_files=$deleted.Count;modified=$modified;deleted=$deleted;new_files=$new;original_before_sha256=$before;original_after_sha256=$after}
if($ReportPath){$result|ConvertTo-Json -Depth 5|Set-Content -LiteralPath $ReportPath -Encoding UTF8}
if($modified.Count -or $deleted.Count){throw "Preservation FAILED: modified=$($modified.Count) deleted=$($deleted.Count)"}
Write-Host "PASS original_files=$($before.Count) modified_existing_files=0 deleted_existing_files=0 new_files=$($new.Count)"
