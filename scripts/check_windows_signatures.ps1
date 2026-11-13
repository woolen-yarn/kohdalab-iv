[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$Directory,
    [string]$ReportPath,
    [switch]$RequireSigned
)

# Read-only release audit. It never changes trust settings, certificates, or
# package files. Run this against a staged Windows package before packaging it.
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

function Get-FullPath([string]$Path) {
    return [IO.Path]::GetFullPath($Path)
}

function Assert-NoReparsePoints([string]$Root) {
    $rootItem = Get-Item -LiteralPath $Root -Force -ErrorAction Stop
    if (($rootItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
        throw "Refusing to inspect a reparse-point directory: $Root"
    }
    foreach ($item in @(Get-ChildItem -LiteralPath $Root -Force -Recurse -ErrorAction Stop)) {
        if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw "Refusing to inspect a tree containing a reparse point: $($item.FullName)"
        }
    }
}

function Get-CertificateRecord($Certificate) {
    if ($null -eq $Certificate) { return $null }
    return [ordered]@{
        subject = [string]$Certificate.Subject
        issuer = [string]$Certificate.Issuer
        thumbprint = [string]$Certificate.Thumbprint
        not_before = $Certificate.NotBefore.ToUniversalTime().ToString('o')
        not_after = $Certificate.NotAfter.ToUniversalTime().ToString('o')
    }
}

$root = Get-FullPath $Directory
if (-not (Test-Path -LiteralPath $root -PathType Container)) {
    throw "Directory does not exist: $root"
}
Assert-NoReparsePoints $root

$files = @(Get-ChildItem -LiteralPath $root -File -Force -Recurse -ErrorAction Stop |
    Where-Object { $_.Extension -in @('.exe', '.dll') } |
    Sort-Object FullName)

$results = foreach ($file in $files) {
    $signature = Get-AuthenticodeSignature -LiteralPath $file.FullName
    $relative = $file.FullName.Substring($root.TrimEnd('\').Length + 1).Replace('\', '/')
    $timestampCertificate = Get-CertificateRecord $signature.TimeStamperCertificate
    [ordered]@{
        path = $relative
        status = [string]$signature.Status
        status_message = [string]$signature.StatusMessage
        signature_type = [string]$signature.SignatureType
        signer = Get-CertificateRecord $signature.SignerCertificate
        timestamp_present = ($null -ne $signature.TimeStamperCertificate)
        timestamp_certificate = $timestampCertificate
    }
}

$invalid = @($results | Where-Object { $_.status -ne 'Valid' })
$report = [ordered]@{
    generated_utc = [DateTime]::UtcNow.ToString('o')
    directory = $root
    file_count = $results.Count
    valid_count = @($results | Where-Object { $_.status -eq 'Valid' }).Count
    invalid_count = $invalid.Count
    require_signed = [bool]$RequireSigned
    files = @($results)
}
$json = $report | ConvertTo-Json -Depth 8
if (-not [String]::IsNullOrWhiteSpace($ReportPath)) {
    $reportFullPath = Get-FullPath $ReportPath
    $parent = Split-Path -Parent $reportFullPath
    if (-not (Test-Path -LiteralPath $parent -PathType Container)) {
        New-Item -ItemType Directory -Path $parent -Force | Out-Null
    }
    Set-Content -LiteralPath $reportFullPath -Value $json -Encoding UTF8
}
Write-Output $json

if ($RequireSigned -and ($files.Count -eq 0 -or $invalid.Count -gt 0)) {
    throw "Signature audit failed: $($invalid.Count) of $($results.Count) executable or DLL files are not Valid."
}
