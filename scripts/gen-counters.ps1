# Builds data/counters.json: for each enemy hero, items whose win rate against that hero
# beats the item's overall win rate the most (min sample sizes). Source: Deadlock API item-stats.
$ErrorActionPreference = "Continue"
$root = Split-Path $PSScriptRoot -Parent
$api = "https://api.deadlock-api.com/v1/analytics/item-stats"
$items = Get-Content "$root\data\items.json" -Raw | ConvertFrom-Json
$heroes = Get-Content "$root\data\heroes.json" -Raw | ConvertFrom-Json
$byId = @{}; foreach ($i in $items) { $byId[[long]$i.id] = $i }
$base = @{}; foreach ($r in (Invoke-RestMethod "$api`?bucket=no_bucket&min_matches=2000")) { if ($r.matches -gt 0) { $base[[long]$r.item_id] = $r.wins / $r.matches } }
$out = @()
foreach ($h in $heroes) {
  try { $rows = Invoke-RestMethod "$api`?bucket=no_bucket&enemy_hero_ids=$($h.id)&min_matches=1500" } catch { Write-Warning "skip $($h.name)"; continue }
  $c = foreach ($r in $rows) {
    $id = [long]$r.item_id; if (-not $byId.ContainsKey($id) -or -not $base.ContainsKey($id) -or $r.matches -le 0) { continue }
    if ($byId[$id].tier -lt 2) { continue }
    [pscustomobject]@{ item=$byId[$id]; lift=(100.0 * ($r.wins / $r.matches - $base[$id])) } }
  foreach ($x in ($c | Sort lift -Descending | Select -First 3)) {
    if ($x.lift -le 0.5) { continue }
    $out += [pscustomobject]@{ when_enemy_has=$h.class_name; suggest=$x.item.name; suggest_class=$x.item.class_name; label=$h.name
      reason = ("Wins {0:N1} points more against {1}." -f $x.lift, $h.name) }
  }
}
$out | ConvertTo-Json -Depth 3 | Set-Content "$root\data\counters.json" -Encoding utf8
"counters: $($out.Count)"
