# Rejuvenator

Source: https://deadlock.wiki/Rejuvenator ; also https://deadlock.wiki/Midboss
Retrieved: 2026-10-25
Page last modified: 2026-10-04. Latest update-history entry: 2026-06-30.

## How it appears
- A single green crystal spawns above the Midboss pit only after the Midboss is killed (see midboss.md).
- Descends over 6 s (was 7 s before 2026-03-06) and cannot be claimed until fully landed.
- Any player (either team) can claim it by heavy melee. Each heavy melee hit = 1 Rejuvenator Credit for the hitter's whole team, up to 3 credits per Midboss kill.
- The first player to punch it is healed to 100% health.

## Rejuvenation Credit buff
- Team-wide buff; lasts 180 s (3 min) or until all credits are consumed, whichever first. (Reduced from 4 min on 2026-06-30.)
- Duration no longer refreshes when hitting a crystal later (2026-03-06).
- 30 s warning before expiry.
- On death with the buff: revive in place with 100% health after 3 s; that player loses the buff and team credits drop by 1 (one credit consumable per player).
- Killing a player with a credit shows no kill and gives no souls, but triggers "on hero kill" effects (Shiv Killing Blow reset, ult stacks of Grey Talon / Mo & Krill / Vindicta, Drifter Bloodscent).
- Reviving player is effectively dead during the 3 s countdown (no ally presence effects, does not contest Unstable Rift).
- Buff no longer grants +10% Max HP, +15% Fire Rate, +15% Spirit Damage Amp (2025-11-21).

## Timeline example (computed from the above, not stated on page)
- Midboss killed at T -> crystal claimable at T+6 s -> buff ends at (claim time)+180 s.
- If the enemy team claims it you have the Midboss souls/buffs but they get the revives: be ready to defend.

## UNVERIFIED
- Whether credits claimed later by the second team share the same 180 s window as first claim (page says duration no longer refreshes; per-team vs global timer not stated).

## Machine-friendly table
| name | first_spawn_secs | interval_secs | notes |
|---|---|---|---|
| rejuvenator_crystal | n/a (after midboss kill) | n/a | descends 6 s, claim by heavy melee |
| rejuv_credit_max | n/a | n/a | 3 credits per midboss kill |
| rejuv_buff_duration | n/a | 180 | team buff, expires early when all credits used |
| rejuv_revive_delay | n/a | 3 | revive in place at 100% HP |
