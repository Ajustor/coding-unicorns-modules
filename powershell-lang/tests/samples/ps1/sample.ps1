# Script
param(
    [string]$Path = ".",
    [switch]$Recurse
)

function Get-Size {
    [CmdletBinding()]
    param([Parameter(Mandatory)][string]$Folder)
    $files = Get-ChildItem -Path $Folder -File -Recurse:$Recurse
    return ($files | Measure-Object -Property Length -Sum).Sum
}

$total = Get-Size -Folder $Path
if ($total -gt 1MB) {
    Write-Host "Big: $($total / 1MB) MB" -ForegroundColor Yellow
} else {
    Write-Output 'Small'
}
<# block
   comment #>
foreach ($i in 1..3) { $env:COUNT = $i }
