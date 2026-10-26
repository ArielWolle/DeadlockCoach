# Quieter, explained item suggestions

Retrieved: 2026-10-26

The opponent/item associations in `data/counters.json` come from matchup win-rate samples.
They do not prove a causal hero-counter interaction. The coach now explains **one item** per
call instead of reading a three-item list. A shared three-minute category gap prevents six
enemy heroes producing six back-to-back lists. The same item has a twenty-minute gap across
opponents. Browser delivery also retains these limits across option changes.

Gameplay suggestions wait until three minutes, need an observed shopping budget, skip owned
items and known unaffordable items, and are suppressed during low/falling own health or a
visible nearby enemy. They remain low priority behind survival/waves/objectives. A suggestion
is an option for the next shopping opportunity, not an instruction to leave the current wave.

## Verified item functions

These summaries explain item use; the associated matchup ranking remains statistical.

| Item | Concise use explanation | Source |
| --- | --- | --- |
| Healing Nova | Heal yourself and nearby allies to recover health between exchanges. | [Healing Nova](https://deadlock.wiki/Healing_Nova) |
| Guardian Ward | A barrier absorbs damage; temporary movement speed helps you or an ally disengage. | [Guardian Ward](https://deadlock.wiki/Guardian_Ward) |
| Arcane Surge | Dash-jump before the next ability to gain range, duration and Spirit Power. | [Arcane Surge](https://deadlock.wiki/Arcane_Surge) |

The exact source excerpts and check date are stored in `data/coaching/item-rationales.json`.
For suggestions without a verified function summary, the coach gives the recorded matchup
win-rate reason rather than inventing a mechanical counter. No claim is made that these three
items cleanse, prevent, or directly counter Rem's abilities.

Example: **Statistical option against Rem: Healing Nova. It heals you and nearby allies,
helping recover health between exchanges.**
