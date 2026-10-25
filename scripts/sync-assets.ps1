# Pulls game data from the Deadlock API (api.deadlock-api.com/v1/assets) into data/.
# Produces slim items.json / heroes.json for the knowledge base and icon PNGs for
# template matching. Re-run after game patches.
$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
$d = Join-Path $root "data"; $icons = Join-Path $d "icons"
New-Item -ItemType Directory -Force "$icons\items","$icons\heroes" | Out-Null
$api = "https://api.deadlock-api.com/v1/assets"

$items = Invoke-RestMethod "$api/items"
$slim = $items | ? { $_.type -eq "upgrade" -and $_.shopable } | % {
  [pscustomobject]@{ id=$_.id; class_name=$_.class_name; name=$_.name; tier=$_.item_tier
    slot=$_.item_slot_type; cost=$_.cost; active=$_.is_active_item; image=$_.image } }
$slim | ConvertTo-Json -Depth 3 | Set-Content "$d\items.json" -Encoding utf8

$heroes = Invoke-RestMethod "$api/heroes"
$hs = $heroes | ? { $_.player_selectable } | % {
  [pscustomobject]@{ id=$_.id; class_name=$_.class_name; name=$_.name
    portrait=$_.images.icon_hero_card } }
$hs | ConvertTo-Json -Depth 3 | Set-Content "$d\heroes.json" -Encoding utf8

function Get-Icons($rows, $dir, $prop) {
  foreach ($r in $rows) {
    $url = $r.$prop; if (-not $url) { continue }
    $out = Join-Path $dir "$($r.class_name)$([IO.Path]::GetExtension(($url -split '\?')[0]))"
    if (-not (Test-Path $out)) { try { Invoke-WebRequest $url -OutFile $out -UseBasicParsing } catch { Write-Warning "skip $url" } }
  }
}
Get-Icons $slim "$icons\items" "image"
Get-Icons $hs "$icons\heroes" "portrait"
"items: $($slim.Count)  heroes: $($hs.Count)"
