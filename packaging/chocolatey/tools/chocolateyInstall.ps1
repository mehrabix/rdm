$ErrorActionPreference = 'Stop'

# Update $url/$checksum on each release (the release job's .zip and .zip.sha256).
$url = 'https://github.com/mehrabix/rdm/releases/download/v0.1.0/rdm-windows-x86_64.zip'
$checksum = 'REPLACE_WITH_THE_RELEASE_SHA256'

$toolsDir = Split-Path -Parent $MyInvocation.MyCommand.Definition

Install-ChocolateyZipPackage `
  -PackageName 'rdm' `
  -Url $url `
  -UnzipLocation $toolsDir `
  -Checksum $checksum `
  -ChecksumType 'sha256'

# Shim the binary so `rdm` works from any shell.
$exe = Get-ChildItem -Path $toolsDir -Recurse -Filter 'rdm.exe' |
  Select-Object -First 1 -ExpandProperty FullName
if ($exe) {
  Install-BinFile -Name 'rdm' -Path $exe
}
