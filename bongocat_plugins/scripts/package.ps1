param(
  [string]$Version = ""
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
if ([string]::IsNullOrWhiteSpace($Version)) {
  $Version = "0.1.0-dev." + (Get-Date -Format "yyyyMMddHHmm")
}

Write-Host "Building plugin frontends..."
pnpm --dir "$Root/plugins/todo/ui" install
pnpm --dir "$Root/plugins/todo/ui" build
pnpm --dir "$Root/plugins/mail/ui" install
pnpm --dir "$Root/plugins/mail/ui" build
pnpm --dir "$Root/plugins/cyberwork/ui" install
pnpm --dir "$Root/plugins/cyberwork/ui" build

Write-Host "Building plugin executables..."
Push-Location $Root
try {
  cargo build --release -p bongocat-todo -p bongocat-mail -p bongocat-cyberwork --features custom-protocol
} finally {
  Pop-Location
}

$Output = Join-Path $Root "dist"
New-Item -ItemType Directory -Force -Path $Output | Out-Null

function Package-Plugin {
  param(
    [string]$Id,
    [string]$Name,
    [string]$Entry,
    [string]$Directory
  )
  $Staging = Join-Path $Root "target/plugin-staging/$Id-$Version"
  if (Test-Path $Staging) { Remove-Item -Recurse -Force $Staging }
  New-Item -ItemType Directory -Force -Path $Staging | Out-Null

  Copy-Item (Join-Path $Root "target/release/$Entry.exe") (Join-Path $Staging "$Entry.exe")
  Copy-Item (Join-Path $Directory "resources/icon.png") (Join-Path $Staging "icon.png")
  Copy-Item (Join-Path $Root "LICENSE-MIT.txt") (Join-Path $Staging "LICENSE-MIT.txt")
  Copy-Item (Join-Path $Root "OFL.txt") (Join-Path $Staging "OFL.txt")

  $ManifestText = [System.IO.File]::ReadAllText((Join-Path $Directory "plugin.json"), [System.Text.Encoding]::UTF8)
  $Manifest = $ManifestText | ConvertFrom-Json
  $Manifest.metadata.version = $Version
  $Manifest.metadata.icon.value = "icon.png"
  $Json = $Manifest | ConvertTo-Json -Depth 12
  $Utf8NoBom = New-Object System.Text.UTF8Encoding($false)
  [System.IO.File]::WriteAllText((Join-Path $Staging "plugin.json"), $Json, $Utf8NoBom)

  $Zip = Join-Path $Output "$Id-$Version-windows-x86_64.zip"
  if (Test-Path $Zip) { Remove-Item -Force $Zip }
  Compress-Archive -Path (Join-Path $Staging "*") -DestinationPath $Zip
  Write-Host "Wrote $Zip"
}

Package-Plugin "com.bongocat.todo" "Todo" "bongocat-todo" (Join-Path $Root "plugins/todo")
Package-Plugin "com.bongocat.mail" "Mail" "bongocat-mail" (Join-Path $Root "plugins/mail")
Package-Plugin "com.bongocat.cyberwork" "Cyber Work" "bongocat-cyberwork" (Join-Path $Root "plugins/cyberwork")
