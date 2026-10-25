# DeadlockCoach knowledge base

Searchable markdown for the coach. Each file lists its sources and retrieval date.
Anything marked UNVERIFIED must be confirmed before it becomes a spoken cue.

| Folder | Contents | Origin |
|---|---|---|
| [timings/](timings/README.md) | Midboss, Urn, Rejuvenator, structures, waves, power curve, match timeline | deadlock.wiki (researched) |
| [jungle/](jungle/README.md) | Camps, routes, map layout, buffs, jungle tactics | deadlock.wiki + Deadlock API |
| [strategy/](strategy/README.md) | Laning, macro, teamfights, economy, mistakes, cue library, MOBA fundamentals | wiki + community + general MOBA theory |
| [heroes/](heroes/README.md) | Per-hero win rates and best/worst matchups (generated) | Deadlock API analytics |
| [items/](items/README.md) | Items by slot with cost, tier, win rate, pick count (generated) | Deadlock API |

Regenerate API-derived docs after patches:
```
scripts\sync-assets.ps1
scripts\gen-docs.ps1
```

Known conflicts between sources (wiki pages disagree on some timers and bounties) are listed in
`strategy/README.md` and the individual timing files.
