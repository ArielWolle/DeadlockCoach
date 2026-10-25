# Generates docs/heroes/*.md, docs/items/*.md from the Deadlock API (last 30 days,
# ranked+unranked). Run scripts/sync-assets.ps1 first. Re-run after patches.
$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
$api = "https://api.deadlock-api.com/v1"
$stamp = (Get-Date).ToString("yyyy-MM-dd")
$heroes = Get-Content "$root\data\heroes.json" -Raw | ConvertFrom-Json
$items  = Get-Content "$root\data\items.json" -Raw | ConvertFrom-Json
$hname = @{}; foreach ($h in $heroes) { $hname[[int]$h.id] = $h.name }
function HN($id) { if ($hname.ContainsKey([int]$id)) { $hname[[int]$id] } else { "hero#$id" } }
function Pct($a,$b) { if ($b -gt 0) { [math]::Round(100.0*$a/$b,1) } else { 0 } }

$hd = "$root\docs\heroes"; $idir = "$root\docs\items"
New-Item -ItemType Directory -Force $hd,$idir | Out-Null

# ---- Heroes + matchups ----
$stats = Invoke-RestMethod "$api/analytics/hero-stats?bucket=no_bucket"
$ctr = Invoke-RestMethod "$api/analytics/hero-counter-stats?min_matches=300"
$src = "Source: $api/analytics/hero-stats, $api/analytics/hero-counter-stats (min 300 matches). Generated $stamp. Window: last 30 days, ranked+unranked."
$rows = foreach ($h in $heroes) {
  $s = $stats | ? { $_.hero_id -eq $h.id } | Select -First 1
  $m = $ctr | ? { $_.hero_id -eq $h.id } | % {
    [pscustomobject]@{ enemy=(HN $_.enemy_hero_id); wr=(Pct $_.wins $_.matches_played); n=$_.matches_played } }
  $best  = $m | Sort wr -Descending | Select -First 8
  $worst = $m | Sort wr | Select -First 8
  $wr = if ($s) { Pct $s.wins $s.matches } else { 0 }
  $kda = if ($s -and $s.total_deaths -gt 0) { [math]::Round(($s.total_kills+$s.total_assists)/$s.total_deaths,2) } else { 0 }
  $nm = if ($s) { [int]($s.total_net_worth / [math]::Max($s.matches,1)) } else { 0 }
  $md = @("# $($h.name)","",$src,"","- class_name: ``$($h.class_name)``","- id: $($h.id)","- win rate: $wr% over $($s.matches) matches","- KDA (K+A)/D: $kda","- avg final net worth: $nm souls","",
    "## Strong against (this hero's win rate vs enemy hero)","","| enemy | win% | matches |","|---|---|---|")
  $md += $best  | % { "| $($_.enemy) | $($_.wr) | $($_.n) |" }
  $md += @("","## Weak against","","| enemy | win% | matches |","|---|---|---|")
  $md += $worst | % { "| $($_.enemy) | $($_.wr) | $($_.n) |" }
  $md += @("","Note: win rates are correlational (hero-vs-hero on opposing teams, not necessarily same lane).")
  $slug = ($h.name -replace '[^A-Za-z0-9]+','-').Trim('-').ToLower()
  Set-Content "$hd\$slug.md" ($md -join "`n") -Encoding utf8
  [pscustomobject]@{ name=$h.name; slug=$slug; wr=$wr; n=$s.matches }
}
$idx = @("# Heroes","",$src,"","| hero | win% | matches |","|---|---|---|") + ($rows | Sort wr -Descending | % { "| [$($_.name)]($($_.slug).md) | $($_.wr) | $($_.n) |" })
Set-Content "$hd\README.md" ($idx -join "`n") -Encoding utf8

# ---- Items ----
$is = Invoke-RestMethod "$api/analytics/item-stats?bucket=no_bucket&min_matches=500"
$byId = @{}; foreach ($r in $is) { $byId[[long]$r.item_id] = $r }
$isrc = "Source: $api/assets/items, $api/analytics/item-stats (min 500 matches). Generated $stamp."
foreach ($slot in ($items | Group slot)) {
  $md = @("# $($slot.Name) items","",$isrc,"","| tier | item | cost | active | win% | picks |","|---|---|---|---|---|---|")
  foreach ($i in ($slot.Group | Sort tier, cost)) {
    $st = $byId[[long]$i.id]
    $w = if ($st) { Pct $st.wins $st.matches } else { "n/a" }
    $n = if ($st) { $st.matches } else { "n/a" }
    $md += "| $($i.tier) | $($i.name) | $($i.cost) | $($i.active) | $w | $n |"
  }
  Set-Content "$idir\$($slot.Name).md" ($md -join "`n") -Encoding utf8
}
$all = @("# Items (all)","",$isrc,"","| item | class_name | slot | tier | cost |","|---|---|---|---|---|") + ($items | Sort name | % { "| $($_.name) | $($_.class_name) | $($_.slot) | $($_.tier) | $($_.cost) |" })
Set-Content "$idir\README.md" ($all -join "`n") -Encoding utf8
"heroes: $($rows.Count)  item stat rows: $($is.Count)"

