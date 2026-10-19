[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$Directory,
    [Parameter(Mandatory = $true)][string]$Version,
    [string]$ReportPath
)
# Read-only metadata gate; this does not imply a valid Authenticode signature.
$ErrorActionPreference = 'Stop'
$root = [IO.Path]::GetFullPath($Directory)
$paths = @('kohdalab-iv.exe', 'iv-cli.exe',
    'support/kohdalab-gpib-helper.exe', 'support/kohdalab-usbtmc-helper.exe',
    'support/kohdalab-visa-helper.exe', 'support/USB-Setup.exe')
$files = foreach ($relative in $paths) {
    $file = Join-Path $root $relative
    if (-not (Test-Path -LiteralPath $file -PathType Leaf)) { throw "Missing PE: $relative" }
    $info = [Diagnostics.FileVersionInfo]::GetVersionInfo($file)
    if ($info.ProductName -ne 'KohdaLab IV' -or $info.ProductVersion -ne $Version -or
        $info.CompanyName -ne 'Kohdalab' -or $info.FileVersion -ne $Version -or
        $info.OriginalFilename -ne [IO.Path]::GetFileName($file)) {
        throw "Unexpected product/version metadata: $relative"
    }
    [ordered]@{path=$relative;product=$info.ProductName;version=$info.ProductVersion;
        file_version=$info.FileVersion;company=$info.CompanyName;original_filename=$info.OriginalFilename}
}
$report = [ordered]@{version=$Version;files=@($files);signature_claim=$false;
    upstream_excluded=@('support/libwdi.dll');gpib_signing_eligibility='requires SignPath review'}
$json = $report | ConvertTo-Json -Depth 5
if ($ReportPath) { Set-Content -LiteralPath $ReportPath -Value $json -Encoding UTF8 }
Write-Output $json
