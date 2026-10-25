# MOBA fundamentals applied to Deadlock

Type: general MOBA theory (Dota 2 / LoL / HoN lineage), NOT sourced from Deadlock pages.
Treat every mapping to Deadlock as a hypothesis to confirm against the Deadlock-specific docs
(`docs/timings`, `docs/strategy`) before turning it into a spoken cue. Written 2026-10-25.

## Core principles

| Principle | Meaning | Deadlock mapping (hypothesis) | Detectable from screen |
|---|---|---|---|
| Economy is power | Gold/souls lead converts to item lead then fights then objectives | Souls/net worth, shop tiers | souls, net worth, items |
| Tempo | Act before the opponent: first to the objective/spawn | Arrive before Midboss, Urn, wave clashes | game clock, minimap |
| Map control and vision | Know where the enemy is; deny safe farming | Minimap pings, enemy positions | minimap |
| Trading | Spend resources (HP/mana/cooldowns) efficiently vs opponent | Lane trades, ability cooldown advantage | health bars, cooldowns |
| Wave management | Push, freeze, or hold waves to control lane position and farm | Trooper waves toward guardians | minimap, lane state |
| Numbers advantage | Fight when you have more bodies/cooldowns | 6v5 after a kill, ult cooldowns up | scoreboard, ult icon |
| Win condition | Pick the fastest realistic route to the base | Guardian > Walker > Patron | structure state |
| Snowball vs scale | Know whether your team is ahead and should force, or behind and should stall | Net-worth delta | team net worth |
| Cooldown tracking | Fight right after the enemy spends key abilities | Enemy ult cooldowns | enemy bars |
| Risk management | The bigger the lead, the less variance you need | Avoid solo deaths when ahead | deaths, position |

## Decision rules a coach can apply

### When ahead (net worth lead, more towers/structures)
1. Convert the lead into objectives, not kills. Take Guardian/Walker/Midboss while enemies respawn.
2. Group; avoid being caught solo. Do not facecheck dark areas.
3. Deny enemy comeback resources (Urn, Midboss, jungle camps).
4. Spend souls immediately; unspent souls are wasted power.

### When behind
1. Do not fight in the open without numbers or cooldown advantage.
2. Farm safe camps and defend structures; wait for a mistake or a stronger item timing.
3. Avoid trading deaths for nothing; each death costs both time off the map and souls.
4. Pick fights near your structures/teammates; look for picks on isolated enemies.

### Fight evaluation checklist (before engaging)
- Are key ultimates/cooldowns available on my side and used on theirs?
- Is anyone on either team missing (dead/away)?
- Who has the item or level advantage on each side?
- Do I have an escape or an objective to take after winning?

### Objective timing pattern
1. Know the spawn time. Warn about 30-60s ahead (cue), prepare ~15-20s ahead.
2. Group and position before the timer, not after.
3. Check enemy positions. If enemies are missing from lanes, they may be taking the objective.
4. After taking an objective, push immediately while the enemy is respawning.

### Itemization
1. Core items early for your role, situational items after seeing the enemy build.
2. Counter-buy against what is actually hurting you: tanks need armor-ignoring damage, burst heroes need survivability, healers need anti-heal. (Confirm the Deadlock item names in `docs/items`.)
3. Do not hoard souls through fights; buy during respawn or at the base.

### Laning
1. Last-hit (and deny) to maximize income; do not chase kills at the expense of farm.
2. Trade when the enemy spends cooldowns or is out of position; retreat when low.
3. Keep the wave near your side when outnumbered; push when your jungler/teammate can rotate.
4. Track the enemy laners' summoner/ult cooldowns for kill windows.

## Common mistakes (all MOBAs)
- Fighting while the enemy has ultimates and you do not.
- Chasing kills into fog or toward enemy structures.
- Splitting into 2v2 skirmishes while the enemy holds a 5-man.
- Ignoring the minimap for more than a few seconds.
- Dying with unspent resources.
- Contesting an objective after the enemy already has the numbers there.
- Throwing a won game by farming instead of closing.

## Mapping to cue categories
| Category | Example cue | Needs |
|---|---|---|
| Objective timer | "Midboss in 30 seconds" | game clock |
| Economy | "You have unspent souls, go shop" | souls, shop state |
| Itemization | "Enemy stacks bullet damage, consider armor" | enemy items |
| Safety | "Low health, fall back" | HP bar |
| Tempo | "Enemy team is missing, watch for rotation" | minimap |
| Closing | "You're ahead, push the Walker" | net worth, structures |

All thresholds above are to be tuned against recorded sessions.
