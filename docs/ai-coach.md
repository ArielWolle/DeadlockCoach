# Optional AI coach

Deadlock Coach works entirely on your PC by default: screen reading, rules and speech all run
in the browser. The **AI coach** is an optional extra. When you turn it on, the app sends a short
**text** summary of the match to an AI gateway that you choose. It adds model-written advice on
top of the rule-based calls and does not change them.

- **Off by default.** Nothing is sent until you turn it on in **Settings → AI coach (optional)**.
- **Text only.** The app never sends screenshots, video or the minimap image. It sends only the
  summary, an optional player-profile summary and short excerpts from the field guide (see below).
- **Your gateway and your key.** The key is stored only in this browser's `localStorage`
  (`deadlock-coach-settings-v1`). The app never writes it to the cue log, recordings, exports,
  diagnostics or error messages (keys that a gateway echoes back are redacted). The field never
  shows a saved key. Use **Clear key** to remove it. **Restore defaults** also removes it.

## Setup

| Gateway | Provider | Base URL | Model example | Key |
|---|---|---|---|---|
| OpenAI | OpenAI-compatible | *(blank)* = `https://api.openai.com/v1` | `gpt-4o-mini` | `sk-…` |
| OpenRouter | OpenAI-compatible | `https://openrouter.ai/api/v1` | `openai/gpt-4o-mini`, `anthropic/claude-3.5-haiku` | OpenRouter key |
| LiteLLM / company proxy | OpenAI-compatible | your proxy's `/v1` URL | the proxy's model alias | proxy key |
| Anthropic | Anthropic Messages | *(blank)* = `https://api.anthropic.com` | `claude-3-5-haiku-latest` | `sk-ant-…` |
| Ollama (local) | OpenAI-compatible | `http://localhost:11434/v1` | `llama3.1:8b`, `qwen2.5:7b` | *(none)* |
| LM Studio (local) | OpenAI-compatible | `http://localhost:1234/v1` | the loaded model id | *(none)* |

1. Open **Settings**, tick **Enable the AI coach**, then choose the provider, base URL and model.
2. Paste a key if your gateway needs one. Then click **Save settings**.
3. Click **Test connection**. You see either `✓ Connected · model · latency`, or the exact
   failure: a configuration problem, `Network/CORS` (the browser could not finish the request),
   `Gateway error: HTTP <status>: <body>`, or a timeout.
4. On the Coach page, use **Ask the coach now** at any time.

Other settings:
- **Advice interval when quiet:** default 90 s (minimum 15 s). Meaningful changes ask sooner (see
  "When it speaks"). A saved 40 s (the old, chatty default) is moved to 90 s.
- **Max reply tokens:** default 1,024 (range 512-8,192): the budget for the answer itself. The
  reasoning headroom below is added on top. 200 (an old default) cut Claude Haiku replies off
  mid-JSON on OpenRouter; saved values below 512 are raised to 1,024. A reply cut off by the limit
  (`finish_reason: "length"`, including a reply whose reasoning used the whole budget) is retried
  once with double the reply budget, then reported as "Reply cut off".
- **Model reasoning:** *Low* (default), *Off* or *Model default*. rec11 ran Claude Haiku 5.5 and
  `openrouter/free` (routed to a Nemotron reasoning model) through OpenRouter with no reasoning
  setting: replies spent 400-1,240 reasoning tokens, took 3-6.6 s, and 5 of 60 ran out of tokens
  before answering. How the setting is sent (`reasoningParams()`):

  | Gateway | Off | Low | Model default | Extra max_tokens |
  |---|---|---|---|---|
  | OpenRouter (base URL on `openrouter.ai`) | `reasoning: {enabled: false}` | `reasoning: {effort: "low"}` | nothing | Off 0 · Low +1,024 · Default +2,048 |
  | OpenAI `o1`/`o3`/`o4-mini`/`gpt-5*` (not via OpenRouter) | `reasoning_effort: "minimal"` (gpt-5) / `"low"` (o-series) | `reasoning_effort: "low"` | nothing | gpt-5 Off +256, else +1,024 / +2,048 |
  | Anthropic Messages | nothing (no extended thinking unless asked) | nothing | nothing | 0 |
  | Other OpenAI-compatible (Ollama, LiteLLM, …) | nothing | nothing | nothing | 0 |

  OpenRouter turns `effort: "low"` into a 1,024-token thinking budget for Claude models
  (`max(max_tokens × 0.2, 1024)`) and requires `max_tokens` to stay above it, hence +1,024
  (docs: openrouter.ai/docs/use-cases/reasoning-tokens). OpenAI reasoning models get
  `max_completion_tokens` and no `temperature`. If a gateway answers HTTP 400 naming the
  reasoning field (a model whose reasoning is mandatory), the request is sent once more without
  it. Reasoning tokens (`usage.completion_tokens_details.reasoning_tokens`) are kept in the AI
  exchange log (`usage`, `max_tokens`, `reasoning`), shown in the AI panel meta line, in the raw
  exchange header, in **Test connection**, and averaged under **Runtime diagnostics**.
- **Speak AI advice:** respects **Mute voice**.
- **Extra request headers:** a JSON object, for example `{"HTTP-Referer": "https://example.com"}`
  for OpenRouter, or `{"api-key": "…"}` for an Azure-style proxy. These header values are
  treated as secrets in error messages too.

Request shapes:
- **OpenAI-compatible:** `POST {base}/chat/completions` with `Authorization: Bearer <key>`
  (left out when no key is set) and `{model, messages:[system,user], max_tokens, temperature: 0.3}`
  plus the reasoning field above (OpenAI reasoning models: `max_completion_tokens`, no temperature).
- **Anthropic:** `POST {base}/v1/messages` with `x-api-key`, `anthropic-version: 2023-06-01`,
  `anthropic-dangerous-direct-browser-access: true` and `{model, max_tokens, temperature, system, messages}`.

### CORS and http://

The browser calls the gateway directly. There is no app server. So the gateway must allow
**CORS** from the page origin, for example `https://arielwolle.github.io` or
`http://127.0.0.1:4173`. That includes the preflight for `Authorization` / `x-api-key` /
`Content-Type`.
- OpenAI, OpenRouter and Anthropic (with the header above) allow browser calls.
- **Ollama:** start it with `OLLAMA_ORIGINS=https://arielwolle.github.io` (or `*`).
- **LM Studio:** turn on *Enable CORS* in the server settings.
- **LiteLLM / proxies:** add the page origin to their allowed origins.

The site is served over HTTPS, so browsers block `http://` gateways unless they run on
`localhost` / `127.0.0.1`. The settings check reports this before it sends anything. A
"Network/CORS failure" result almost always means a missing CORS header, a wrong URL, or a
gateway that is not running.

## When it speaks

- **Periodic, on meaningful changes.** Only while coaching live, alive and on the Gameplay screen,
  never within 10 s of a High-priority rule call, at least 45 s (`EVENT_GAP_SECS`) after the last
  periodic request **and** the last spoken AI advice, and only for an event from the last 40 s:
  a new phase (laning → midgame at 8:00, → lategame at 20:00), a respawn, an enemy-dead window
  (≥ 2 enemies dead), a major objective within 60 s (Powerups at 5:00, Sinner's at 8:00, every
  Soul Urn, first Rift, Guardians at 12:00, Walkers at 18:00), souls reaching the usual buy, or a
  big swing in the team souls difference (≥ 4,000 or 5% of the average team souls) since the last
  request, or a good play (see "Reinforcing good plays"). The prompt starts with
  `WHY NOW: <what changed>`.
- **Periodic, quiet.** Otherwise once per *interval* (default 90 s) with `WHY NOW: routine check,
  nothing new since …: PASS unless you see a NEW, specific insight`.
- **Death:** once per death, after the rule coach's death review **of this death** has been
  spoken, or 4 s after the death panel has been read (8 s at most); with a respawn timer of 25 s or
  more (or none read after 15:00) it waits up to 15 s for that review, with 40 s or more (or none
  read after 30:00) up to 25 s (rec12's reviews came 7-25 s after the death, rec13's 41:05 one 25 s).
  **Without a damage panel** (no `death_damage` / `death_cause` / killing blow read: the shop hides
  it) it waits for the panel up to `DEATH_PANEL_MAX_WAIT_MS` (40 s), or 4 s after the rule death
  review, but never into the last 10 s of the respawn timer (`DEATH_RESPAWN_MARGIN_SECS`): rec8's
  20:40 death sat in the shop for 30 s and the old call went out at 21:01 with "cause unknown";
  now it goes out at 21:10 with "cause Tankbuster · damage taken: Tankbuster 479, Barrage 459, …".
  The death prompt also carries `Before this death: teammates dead at the death (top bar): 3 of 5 ·
  you left our base at 41:44 with 3 teammates dead, 21 s before the death · you were in our half`
  (`beforeDeathLine()`; "your position was not read (unknown)" without a marker), and a throw in
  progress (THROW RISK) outranks the other context lines.
  The answer (≤ 2 sentences, 30 words) is spoken only when it adds a **different concrete lesson**
  to the rule review (`deathCovered()`: not ≥ 45% of its words, and its next-life sentence not only
  the review's "Next time" concepts): one coach voice on the death screen. No cheap retry for a
  death, and never two AI death calls within 20 s of game time. It is built from the damage panel,
  the killing blow, the status effects of the last 10 s alive, the health before death, who was
  near you and the rule death review (which names the killer hero). Opening the shop or scoreboard
  from the death screen is the same death, and so is a gameplay frame **without a health read**
  while the respawn timer still runs (or within 15 s of the death without a timer): rec13's 64 s
  death at 41:05 read Shop at 41:05, Gameplay without health at 41:17, 41:44 and 41:49, and got 3
  AI death reviews; now it is one death and one request.
- **Fights belong to the rule coach.** While a fight is on (2+ heroes died on the top bar in the
  last 10 s, or your health falls fast: `health_delta_ratio` ≤ -25% or 30 points within 6 s) no
  periodic request is made, and a periodic answer that arrives during one is dropped
  (diagnostics: "N fights left to the rule coach", "N dropped for a fight").
- **Not on a theme the rule coach just covered.** A rule call's key gives its themes
  (`RULE_THEMES`: `plan:` objective/team, `threat:` positioning/map, `lead:` team/objective,
  `reset:` healing, `fight` team/positioning, `timer:` / `map:urn` / `pos:enemies_down`
  objective, `map:wave` / `wave:` / `lane:last_hit` wave, `lane:back_off` / `lane:critical`
  healing, `pos:gank` / `pos:missing` map, `unspent` economy, `counter:` itemization, ...). A
  cadence event whose theme a rule call covered in the last 30 s does not ask (respawn → team,
  enemy-dead window / objective → objective, souls → economy, team souls swing → team), and a
  periodic answer on such a theme is not spoken ("same theme (objective) as the rule call
  plan:walker:Yellow at 30:10"). RECENT COACH CALLS show each call's themes.
- **Latency-aware windows.** The gateway latency is measured (smoothed, default 6 s; rec13's median
  was 5.8 s) and the summary says "heard about 6 s after this state". An enemy-dead window is
  stated as the seconds at least 2 enemies stay dead after the call is heard (second-longest
  respawn timer minus the latency); at 10 s or less it is "too short for a structure, PASS or call
  the reset", the event does not ask, and an answer sending us to a structure is not spoken.
- **On demand:** **Ask the coach now**. In Replay lab and when you are not live, the answer is
  shown but not spoken. Before any match state has been read it does not send a request (rec11
  spent one on "Manual trigger with no clock").

Safeguards:
- Only one request is in flight at a time.
- Live-trigger answers older than 15 s are dropped.
- Pause, Stop and starting a replay cancel the request (`AbortController`).
- After an error, automatic requests back off exponentially: 5 s, 10 s, 20 s, … up to 5 min.
  The backoff is shown in the panel and under **Runtime diagnostics → AI coach**.
- Spoken AI advice uses the existing speaker with cue keys `ai:<trigger>:<theme>` and a 30 s
  cooldown for periodic advice.
- AI priority is capped at *Normal* in the speech queue, so it never interrupts urgent rule
  calls.
- Advice is listed in **Recent calls** with an **AI** marker, and in the **AI coach** panel
  with its reason, latency, model and estimated input tokens.
- Every automatic answer passes the repeat and souls gate below; a rejected answer is shown as
  "Skipped a repeat: …" and counted under **Runtime diagnostics** ("N repeats skipped").

## Repetition and souls gating

In a reviewed match (rec8) the previous version spoke 7 AI calls and 6 of them were "spend your
souls" variants, several of them "buy a 1,600 item" at 34–40 minutes, right after the rule coach
had said the same. The coach now follows the rule engine's time-scaled souls rule and refuses
repeats, in the prompt and in code (`checkAdvice()` in `web/src/ai-coach.ts`):

- **Souls in tier context.** `spendTarget(clock)` / `deathSpend(clock)` (`web/src/player-profile.ts`,
  mirroring `crates/engine/src/economy.rs`): the usual buy is 1,600 before 10:00, 3,200 until
  20:00 and 6,400 after; a death is worth a souls remark from half that (at least 1,200). The
  summary says `Souls 2,834 (saving: the usual buy now is a 6,400 item; 3,566 short)` or
  `Unspent souls 7,443 · can afford the usual 6,400 buy for 40:26`, and always ends with
  `Souls advice: allowed` or `Souls advice: not allowed (<reason>)`.
- **Souls gate.** Alive: open only when the usual buy is affordable **and** the rule coach has
  not given a souls reminder (`unspent*` cues) in the last 3 minutes. Dead: open only when the
  souls at death reach the death-spend threshold. When the gate is closed, sentences about souls
  or spending are dropped from the answer; if nothing is left, nothing is spoken. A sentence that
  names a tier cheaper than the usual buy (half of it on the death screen) is dropped even when
  the gate is open.
- **Theme memory (periodic answers).** Answers are normalized to a theme (`topicOf()`:
  positioning, healing, team, objective, economy, map, itemization, wave, mechanics; the
  model's `focus` tag first, else the text; an answer whose every sentence is about souls is
  economy). Each delivered call also records the **weakness concepts** it coaches (`CONCEPTS`:
  range against divers, not walking out alone, healing before fights, waiting for the team,
  missing enemies, Midboss, crowd control) and the episode counters of the live triggers at
  that moment. Within 5 minutes (`THEME_WINDOW_SECS`):
  - a sentence restating a concept already coached is dropped ("Silver and Apollo are down, so
    regroup … Stay at max range behind your front line." keeps only the first sentence); if
    nothing self-contained is left (no action, or a pronoun pointing at the dropped sentence),
    nothing is spoken;
  - an answer on a theme already coached is not spoken;
  - **unless a NEW concrete trigger** for it happened since: a live trigger that became active
    again after the earlier call (`liveTriggers()`: `diver-near`/`enemy-near` for positioning,
    `hp-low` for healing, `respawn`/`allies-down`/`isolated`/`enemy-window` for team,
    `enemy-window`/`objective-soon` for objective, `missing` for map, `souls-tier` for economy,
    `debuff` for mechanics). High priority alone is not new (rec11 marked half its repeats High).
  - The rule coach's "Profile focus: … Stay at max range behind your front line." counts as
    coached too.
  Death reviews and **Ask the coach now** are not theme-gated (they still pass the souls gate
  and the near-duplicate check), but their themes are remembered. The prompt carries
  `THEMES COACHED` (last 10 minutes: each theme/weakness with its clocks and `blocked`,
  `NEW trigger now: …` or `free again`).
- **No near-duplicates.** An answer whose content words overlap ≥ 60% (of the shorter text) with
  an AI call from the last 5 minutes or a rule call from the last 90 s is not spoken.
- **RECENT AI ADVICE.** The last 4 delivered AI calls (10 minutes) are in the prompt with their
  clock and theme, so the model can vary on its own.
- **Cheap retry only.** If an automatic answer is rejected and came back within 2.5 s, the coach
  asks once more with "YOUR PREVIOUS ANSWER WAS NOT USED (<reason>) … give advice on a different
  focus, or PASS". Otherwise it simply does not speak.
- **On demand is not gated.** **Ask the coach now** shows the model's answer as is (the prompt
  rules still apply).

## Match boundaries and this match's evidence

**rec12 bug.** Two of the three live death explanations in rec12 (Grey Talon) named heroes of the
previous match ("Baba's Feed The Birds" at 9:48, "Baba and Graves" at 14:50) and the third blamed
"trooper melee" at 22:10, when Drifter and Silver did the damage. Root cause, from the recorded
prompts:

1. **The AI history was never reset between the two matches.** `AiHistory` started a new match only
   when the clock of a frame was 30 s lower than the clock of the *previous frame*. Between the
   Violet match (last clock 37:22) and the Grey Talon match (1:07) the app read about 11 minutes of
   menus without a clock (Shop, Countdown, loading), so the comparison never ran; the HUD "match id"
   was 127139499 in both matches, so nothing else noticed either. The rec12 death prompts said
   "Deaths observed this session: 10 (at 27:50, 33:52, 37:16, 9:48)".
2. **The rule death review was matched by `clock >= death clock` only.** The Violet match's last
   review ("Baba did 71% of the damage … Baba and Graves", spoken at 37:22) passed that test for
   every rec12 death (9:48, 14:50), and the AI asked 4 s after the panel, before rec12's own review
   (7-25 s later) arrived. So the model quoted Baba and Graves.
3. **No hero names for the damage panel.** "Rend" (Drifter) and "Boot Kick" (Silver) were bare
   ability names and the panel's cause read "Trooper", so the model said "trooper melee".
4. The player profile (earlier matches) also names killers ("Jar x3"); nothing told the model that
   those are other matches.

**Fixes** (`web/src/ai-coach.ts`):
- **Boundaries.** A new match starts (history, deaths, spoken cues, advice, roster, good plays and
  the controller's cadence state are forgotten) when the clock restarts against the last clock
  *read* (across clock-less frames), after a post-game screen, when the hero changes after a 3-minute
  gap (or, confirmed by 3 reads, with a clock that is not continuous), or when the clock jumps more
  than the wall time allows after a long gap. A game pause (clock still) and an early portrait
  misread with a continuous clock are not boundaries. Diagnostics show "new match detected N× (last:
  the clock restarted (37:16 → 1:07))".
- **This death only.** `reviewsFor(death)`: a rule review counts only when spoken after this death
  (wall time), before its respawn, and within its clock window (`death:review:<clock>` within 40 s).
  The death review waits up to 15 s for it when the respawn timer is long. Earlier deaths of the
  match are labelled "Earlier Death at …", the current one "(THIS death)".
- **Damage by hero.** `data/abilities.json` maps panel sources to heroes, filtered to this match's
  enemies: `Damage by hero (ability names matched to this match's enemies): Drifter 222 (Rend),
  Silver 168 (Boot Kick); not attributable to one hero: Melee 237, Gun Damage 190`. When the cause
  reads "Trooper" but hero abilities are on the panel: `the panel cause "Trooper" is misleading: hero
  abilities did 25% of the listed damage, so enemy heroes killed you, not troopers`.
- **Named-hero check.** `strangerHeroes()` finds hero names (all 44) in a reply that are not in this
  match's roster (most-read allies and enemies, once 3+ enemies are known); those sentences are
  dropped, and the answer is not spoken when nothing actionable is left — for every trigger,
  including Ask the coach now. "Seven seconds" is a number, not the hero.
- The prompt says to name only heroes of this match, that PLAYER PROFILE heroes and killers are from
  other matches, and the system-prompt example no longer names heroes.
- On the death screen the souls line uses the souls left to spend (rec12 had spent 6,073 down to 132
  and was told "spend your 6,073 souls"), and notes "already spent about 5,900 on this death screen".

## Reinforcing good plays

The player asked for rec12 (a win, 12/3/11, 7% dead): "point out and encourage the good behaviour
that caused our team and myself to do so well, not only point out bad." `AiHistory` derives cheap
good-play signals from the samples it already keeps (`GoodPlay`, newest last):

| Signal | Evidence (rec12) |
|---|---|
| `window-push` | 2+ enemies dead while you are in the enemy base, at an enemy objective banner, or 70%+ up a lane (13:54 with 6 dead, 27:40 with 5 dead); once per push (60 s) |
| `urn` | carrying the urn, then souls +1,000 within 25 s (16:46, +2,128 in 13 s) |
| `death-shop` | two death-screen reads at least the death-spend threshold below the peak, or the first alive read (14:50 death: about 5,900 spent) |
| `reset` | under 35% health, then 90%+ within 150 s without dying (25:14 at 29%) |
| `deathless` | 7 minutes without dying, then every 5 (since the match start or the last respawn) |
| `lead` | team souls lead +5,000 (or 5% of the team average) in about 3 minutes — context only, never praised alone |

A praisable signal is a cadence event (`WHY NOW: good play (see GOOD PLAYS): …`). The summary
lists the last 5 minutes' signals and the gate:

```text
GOOD PLAYS (this match, observed; evidence for praise):
- [13:54] pushed into the enemy base at 13:54 while 6 enemies were dead: a dead-enemy window turned into structure pressure
Praise: allowed (fresh good play: pushed into the enemy base at 13:54 while 6 enemies were dead: …)
```

The model may then open with one short, specific praise and the next action ("Good push while
three were dead: that is how a lead snowballs. Next, back off before they respawn."), focus
`reinforce`. **Gate** (`reinforceGate()`, enforced by `checkAdvice()`): a fresh good play (≤ 90 s,
not praised yet, **not one that led to a death**), none of the last 3 AI tips was praise (at most 1
in 4), and at least 150 s since the last praise. `deathAfter(history, play)`: a play is never
praised when a death followed within `DEATH_PRAISE_SECS` (20 s), when it was a push you never came
back from (every marker read between the push and the death in the enemy half), or a deathless
stretch that ended in a death. rec8's 21:01 death prompt said `Praise: allowed (fresh good play:
pushed Yellow lane deep at 20:14 …)` for the push that killed him at 20:40; now the line reads
`- [20:14] pushed Yellow lane deep … (led to the 20:40 death: never praise it)` and `Praise: not
allowed (the 20:14 play led to the 20:40 death)`. Without it, praise sentences ("Good…", "Nice…", "Well done", "That's how…") are
dropped and the rest must stand alone. A praise of death-screen shopping is not souls advice (the
souls gate does not cut it). Reinforcement has its own gate instead of the theme memory.

## Review-style advice: coaching angles

The summary ends with **COACHING ANGLES**: up to 4 live situations with the fix a coach would
give, marked `[profile: <weakness>]` when the player profile (strengths/weaknesses from earlier
matches) has the matching weakness; matched angles come first. The model is told to prefer them.

| Situation (from the snapshot and history) | Angle | Profile weaknesses it matches |
|---|---|---|
| Respawned ≤ 45 s ago | walk out with a teammate or catch the wave by your Guardian | Dies soon after respawning, Dies alone |
| Health < 60% and not rising | heal to full first (shopping does not heal), leave on the first big hit | Goes back in hurt, Stays in at low health, Gets burst down |
| A close-range diver alive (Abrams, Shiv, Yamato, Mo & Krill, Calico, Lash, Victor, Drifter, Billy) **and** an enemy near you, or walking out after a close-range death (a diver merely alive no longer counts: it is true all game and made rec11 repeat "max range" 15 times) | hold max range behind your front line, save the escape for their dive | Killed at close range, Same killer again |
| ≥ 2 enemies dead | group up and take the Guardian or Walker in the furthest pushed lane (named) | Enemy-dead windows unused |
| A visible enemy hero under 30% (and you ≥ 50%) | a kill chance with an ally; a low enemy is a target, not a threat | Enemy-dead windows unused |
| ≥ 2 allies dead and more than enemies | the fight is lost: back off together | Stays in lost fights |
| No ally near and ≥ 2 enemies missing | move toward a teammate | Dies alone, Pushes in with enemies missing |
| A status effect on you | out of stamina: don't start a trade; a debuff: disengage or cleanse | crowd control |

An angle whose concept was coached in the last 5 minutes in the same trigger episode is left out.
The latest rule death review (`death:review:*`, `active:close`, `active:repeat:*`) is quoted in full
for the death review; afterwards periodic prompts carry a compact `Last death at …` line (killer
from the rule review, cause, top damage, status effects).

## rec11 review: what the live AI coach said

rec11 (Violet, 37 min, OpenRouter with Claude Haiku 5.5) spoke **44 AI tips** on top of ~430 rule
cues. Each was graded against the snapshot at its clock (`frames[].snapshot`, the prompt's STATE
before 6:13):

| Grade | Tips | Examples |
|---|---|---|
| Helpful | 13 | 2:29 "You're at 24% health, go back to your Guardian and heal"; 3:08 "Blue has seven enemy troopers to your one"; 13:53 "Calico and Graves are down, group and push"; 17:16 "5,326 unspent souls, buy the 3,200 item"; most death reviews (Leech Therapy stun, Jar of Dead 879) |
| Generic | 8 | 3:48 "Three enemies are missing … don't cross the map alone"; 16:33 "treat the fog as hostile"; 29:54 "Four enemies are missing … Assume a dive is coming." |
| Repeated | 19 | "Calico is alive … hold max range behind your front line" at 5:21, 11:30, 14:27, 20:57, 23:19, 30:34; "Midboss is up, don't contest it alone" at 15:10, 15:48, 18:41, 21:43, 24:01, 32:37, 36:42; "fall back to your Guardian until X respawns" 6 times |
| Wrong | 4 | 1:43 "Enemy yellow is missing" (a lane colour as a hero); 4:44 Midboss advice in the laning phase; 17:57 "Calico is at 18% and melee range is how you've died most." (a low enemy is a target, and the action sentence was cut off); 27:58 "stay out of Graves's melee range" (Graves is a ranged caster) |

The model never passed in game (the only PASS was a manual ask before any state: "Manual
trigger with no clock"). Failure patterns and their fixes:

1. **One profile weakness restated every call.** The COACHING ANGLE "close-range divers alive"
   fired whenever Calico was alive (all game) because the profile has "Killed at close range" —
   and the rule coach's "Profile focus" line said the same. → the angle now needs an enemy near
   you; theme memory drops a restated weakness for 5 minutes unless a new trigger happens.
2. **"Midboss is up" as advice.** The icon is up almost all game, the brief said "Do not die alone
   while it is up" and an excerpt said "Midboss is up. Don't die alone." → the Objectives line
   mentions the Midboss only after 10:00, says when it is a team target, and Midboss excerpts are
   retrieved only with 3+ enemies dead.
3. **Lane colours read as heroes.** "1 missing (last seen 3s ago Yellow)" became "Enemy yellow".
   → "1 enemy hero missing (last seen 3s ago in Yellow lane)", "Green lane front …", and a
   system-prompt rule.
4. **Too chatty.** A 40 s interval gave ~1.3 tips/min; nothing told the model what changed. →
   adaptive cadence with WHY NOW; quiet checks every 90 s that invite PASS.
5. **Reasoning ate the budget.** 400-1,240 reasoning tokens per reply, 3-6.6 s, and 6 of 60
   exchanges stopped at `finish_reason: "length"` with no answer (4 live periodic calls at the
   1,024 limit). → Model reasoning Low (default) with headroom, reasoning tokens counted.
6. **Hero kits guessed.** The profile was learned on Grey Talon; the model applied it to Violet
   and called Graves a melee hero. → `HERO_ROLES` table (only heroes we are sure of; others say
   "role not in the coach table: do not guess the kit") and a profile/hero mismatch note.

## Offline evaluation

`web/scripts/ai-coach-eval.ts` replays a recording through the real controller with a **fake
model that answers with the rec11 model's stock lines** (max range, walk out, heal first, Midboss,
souls), using the player profile the live coach used (taken from the recording's prompts), and
prints the cadence, what would have been spoken and gated (with reasons) and two full sample
prompts (an event-driven periodic request and a death review). With `AI_EVAL_URL`,
`AI_EVAL_KEY`, `AI_EVAL_MODEL` (optional `AI_EVAL_PROVIDER=anthropic`,
`AI_EVAL_REASONING=off|low|default`, `AI_EVAL_N=5`) it also sends a handful of the event prompts
to that gateway and prints the answers and reasoning tokens. Keys are never printed.

```powershell
cd web
node --experimental-strip-types scripts/ai-coach-eval.ts            # defaults to frames/rec11/recording.json
node --experimental-strip-types scripts/ai-coach-eval.ts --quiet     # counts only
node --experimental-strip-types scripts/ai-coach-eval.ts <replay.jsonl> --quiet --prompt 21:10,42:12   # full prompts at those clocks
node --experimental-strip-types scripts/ai-review-eval.ts --replay <replay.jsonl> --cues <active_replay --all output> [--deaths]   # digest of a vision replay
```

coach-review-8 D11/D12 on the new rec8 / rec13 replays (fake model):

| Item | Before | After |
|---|---|---|
| Death prompt rec8 20:40 | asked at 21:01 in the shop: `cause unknown`; `Praise: allowed (fresh good play: pushed Yellow lane deep at 20:14 …)` | asked at 21:10: `cause Tankbuster · damage taken: Tankbuster 479, Barrage 459, Flying Slash 426, …`; `… (led to the 20:40 death: never praise it)`, `Praise: not allowed (the 20:14 play led to the 20:40 death)` |
| Death prompt rec8 42:05 | no team context; THROW RISK line kept | `Before this death: teammates dead at the death (top bar): 3 of 5 · you left our base at 41:44 with 3 teammates dead, 21 s before the death · you were in our half` and the THROW RISK line |
| Profile text | `… (melee hits x3, a hero Aura of Suffer...; … Spent 2.3k souls between the 10:12 deat` | `… came from melee range; … Spent 2.3k souls between the 10:12 death and the respawn (3 of 7 deaths).` |
| rec13 30:13-33:00 | `Midboss: taken by our team at about 30:13: our Rejuvenator makes the next fight or push safer` | `Rejuvenator: WE hold it until about 32:58 (1:54 left) (… at about 29:52; up to 3 revives in place): use it now with all five on a standing structure; never spend a credit alone`, then `ours runs out in about 3 s`, `ours ran out at about 32:58` |
| Digest rec8 20:40 / 42:05 | `alone no; forward with 3+ enemies unseen no` | `in the enemy half yes (deep, near their base)` / `top bar at the death: 3 of 5 teammates dead …; left our base at 41:44 with 3 teammates dead …; rule call …: "Go back to base: three teammates are dead …", then you left base anyway with 3 teammates still dead (an ignored call)` |
| Digest rec13 41:05 / 43:06 | `alone no` / `alone no; forward no` | `in the enemy half yes (deep, near their base); alone yes (nearest teammate 0.31 map away)` / `not read: your position in the last 12 s (… unknown)` |

rec11, 6:13-40:00 (33.8 game minutes, 600 frames, defaults: reasoning Low, 90 s quiet interval):

| | Before (live) | After (this build, fake stock-line model) |
|---|---|---|
| Gateway requests | 47 (1.39/min) | 34 first attempts (1.01/min): 21 on events, 6 quiet checks, 7 death reviews; plus 18 instant "cheap retries" that a real 3-6 s model would not get |
| Tips spoken | 35 (1.04/min) | 16 (0.47/min) |
| "max range" / "midboss" / "walk out" spoken | 10 / 8 / 7 | 6 (4 of them in death reviews) / 1 / 0 |
| Answers gated | — | 36: 18 near-duplicates, 10 repeats of a rule call (the "Profile focus" line), 8 restated weaknesses with no new trigger |

Event triggers on rec11: respawn 6, enemy-dead window 6, objective within 60 s 5, new phase 2,
souls tier 1, team souls swing 1. A real model is also told to PASS on routine checks, so it
should speak less than the fake one. The recordings are git-ignored, so the script is not part of
`npm run test:unit`; the unit tests cover the same rules.

rec12 (`scripts/ai-coach-eval.ts <main checkout>/frames/rec12/recording.json`): the previous
match's frames (rec11, Violet) are fed first, as in the live session (`--prepend none` skips
that). The fake model names the killer from the rule review or the damage-by-hero line, and praises
when the summary says `Praise: allowed`.

| | Before (live) | After (this build, fake model) |
|---|---|---|
| Match boundary | none (history carried over) | `[0:31] a post-game screen, then gameplay again` |
| Death prompts | 9:50 and 15:08 quoted the Violet match's "Baba … Graves" review; 22:14 had only "cause Trooper" | only rec12 heroes; 15:08 `Drifter 222 (Rend), Silver 168 (Boot Kick)`; 22:27 `the panel cause "Trooper" is misleading` |
| Tips spoken | 77 (2.72/min) | 17 (0.60/min), 3 of them praise (no deaths for 7 minutes at 8:35, the urn at 17:25, the 29% reset at 25:55) |
| Good plays found | — | reset 2:16, deathless 7:31, death-shop 10:17 and 14:54, window-push 13:54 and 27:40, urn 16:46, reset 25:39 (+4 team-lead context lines) |

rec13 (`scripts/ai-coach-eval.ts <main checkout>/frames/rec13/recording.json`, Grey Talon, a loss,
38.8 game minutes): the "before" sizes come from the live `ai-log.json` next to the recording (48
exchanges); the enemy items and scoreboard rows of the newer scan `replay1.jsonl` are merged by
game clock (`--merge none` skips it). The fake model is the rec11 stock-line model.

| | Before (live rec13) | After (this build, fake model) |
|---|---|---|
| Periodic prompt (system + user) | 9,722 chars avg (40 calls) | 7,056 chars avg (27% smaller) |
| Death prompt | 9,674 chars avg (8 calls) | 7,928 chars avg (18% smaller) |
| Generic wiki block | ~1,600 chars on 48 of 48 calls | none |
| AI calls on the theme of a rule call of the 30 s before | 8 of 29 (by the rule keys; the review counted 13) | 2 answers gated, 7 event requests left to the rule coach |
| Periodic requests during fights | — | 0 (34 fight episodes) |
| Deaths / AI death requests | 8 AI death calls (3 for the 41:05 death), 7 rule reviews | 6 deaths, 6 requests (one per death), no retry |

The new periodic state at 25:31 (two enemies dead, you carrying the urn), abridged:

```text
Trigger: periodic · clock 25:31 (lategame, from 20:00) · screen Gameplay
WHY NOW: enemy-dead window: 2 enemies dead (Infernus, Graves)
Word budget: at most 20 words (lategame) · heard about 6 s after this state
You: Grey Talon (long-range marksman) · level 26 · K/D/A 7/1/5 · profile is mostly from Violet games: check its weaknesses fit Grey Talon
Enemies: 4 alive, dead: Infernus, Graves (28s), not seen recently: 2
Enemy items (scoreboard, 5:18 ago): Infernus: Swift Striker, Enduring Speed, Titanic Magazine, Extra Regen; Abrams: Extra Regen; Mina: Tankbuster, Quicksilver Reload; Paradox: Sharpshooter, Restorative Shot; Graves: Arcane Surge, Enchanter's Emblem, Restorative Shot
Team souls: yours 151,000 vs enemy 142,000 (+9,000) · at 22:58 it was +17,000: your team fell 8,000 further behind since · THROW RISK: we were +17,000 and it is shrinking: protect the lead, no base dives without the Rejuvenator
Structures: not read: do not name a specific Guardian, Walker or Shrine as the target
THEMES COACHED (last 10 min; "blocked" = do not coach it again unless WHY NOW gives a new trigger for it):
- theme team: 20:32 (NEW trigger now: enemy-window)
- weakness "not walking out alone": 20:32 (blocked)
COACHING ANGLES (live situation → fix; [profile] = matches a known weakness):
- 2 enemies dead (Infernus, Graves, first back in 28s) → group up and push Blue lane (front 59% up) to its next standing structure now instead of farming your own half
```

## What is sent

The input budget is **about 2,500 tokens**, estimated as characters ÷ 4 and enforced by
`buildPrompt()` (`MAX_INPUT_CHARS = 10000`):

| Part | Budget | Source |
|---|---|---|
| System prompt | ~3,100 chars | `SYSTEM_PROMPT` in `web/src/ai-coach.ts` (one coach voice; the essentials of the old wiki block in rule 9) |
| Task line | ~250–800 chars | depends on the trigger (periodic / death / manual) |
| State summary | ≤ 3,000 chars | `buildSummary()`: the latest snapshot plus about 90 s of history, recent AI advice and coaching angles |
| Player profile | ≤ 500 chars | `setProfileProvider(fn)` callback, scrubbed of heroes of other matches (`scrubProfile()`), weaknesses about mastered skills left out and cut at a sentence / item end (`profileForPrompt()`, `clipAtBoundary()`) |
| Player level | ≤ 600 chars | `setMasteryProvider(fn)` (mastery.ts `promptText()`), cut at a boundary |
| Matchup line | ≤ 300 chars | `docs/heroes/<hero>.md` win rates vs the enemies you face |
| Excerpts | periodic none · death 1 · manual 2, ≤ 450 chars each | sections from strategy/jungle/timings docs, ranked by keyword score |

The generic **coach brief** (`COACH_BRIEF`, ~1,600 characters on all 48 rec13 calls) is gone: its
essentials (item tiers, haunted souls lost on death, a Walker's 3,675 team souls, the Midboss /
Rejuvenator rule) are one rule of the system prompt; timers come from STATE.

If the input is over budget, the app shrinks the parts in this order: excerpts, then the
profile, then the summary. Low-value summary lines (for example, lanes) are dropped before
high-value ones (deaths, HP, souls, recent calls).

The summary uses only fields the app has read, and the model is told that "unknown" never
means safe. It includes:
- the clock and phase (laning until about 8:00, midgame 8:00-20:00, lategame from 20:00)
- WHY NOW: what changed to trigger a periodic request
- your hero with its kit role from `HERO_ROLES` (or "role not in the coach table: do not guess
  the kit"), level and K/D/A (when read), and a note when the profile was learned on another hero
- HP and its 90/60/30-second trend
- status effects
- souls against the usual buy for the game time (saving / can afford), unsecured souls, how long
  you have held them, and whether souls advice is allowed (see the souls gate above)
- "Respawned N s ago" for 45 s after a respawn
- your items with names and costs, and souls invested per slot (weapon / vitality / spirit)
- your lane, how far up the lane you are, and the zone
- allies and enemies alive, dead (with respawn timers), not seen recently, or at low HP
- enemy kit roles for the heroes in the table
- team souls and the trend over about 3 minutes ("your team fell 6,000 further behind since")
- enemy heroes visible, near you, or missing on the minimap ("last seen 3s ago in Yellow lane")
- lane fronts ("Green lane front 60% up (troopers 3 yours v 2 theirs)")
- Midboss (after 10:00 only, with when it is a target) and Urn
- the next objective timers
- deaths, including the damage breakdown, killing blow, status effects before death, allies and
  enemies near you, and whether the death-spend threshold was reached
- the rule coach's death review (death trigger), or a compact "Last death at … (N ago): killer;
  cause; most damage; status effects" line afterwards
- RECENT AI ADVICE (the last 3, with themes), THEMES COACHED (blocked themes and weaknesses only,
  or the ones a NEW trigger frees) and recent rule calls with their themes
- COACHING ANGLES (live situation × player-profile weakness)
- rec13 additions (docs/coach-review-13.md item 12): `Word budget: at most 20 words (lategame) ·
  heard about 6 s after this state`; `MATCH FOCUS GOAL: … (so far: …)` (player profile,
  `setGoalProvider`); `Fight: ON NOW (3 heroes died in the last 10 s) · just WON a fight: 3 enemies
  died in the last 20 s (Infernus, Abrams, Mina) vs 0 of ours · you are at 30%: heal first,
  objective second` (or just LOST / an even trade); `Enemy-dead window: at least 2 stay dead about
  19 s after you are heard`; K/D/A from the HUD or your scoreboard row (kept between reads); `Your
  ultimate: ready` / `on cooldown (23 s)` (`abilities` slot 4); `Enemy items (scoreboard, 2:44
  ago): Infernus: Swift Striker, …` (the scoreboard rows by hero, else `enemy_items`: rec13 sent
  neither); `Fed enemy: Infernus 49,000 net worth vs next Lady Geist 41,000 (+8,000)`; the team
  souls trend over about 3 minutes with `THROW RISK: we were +46,000 and it is shrinking: protect
  the lead, no base dives without the Rejuvenator` (a peak lead of 15k+ that fell 5k+ in 3 minutes
  or 15k+ overall, or a 15k+ lead while the enemy took the Midboss); `Structures (read; a
  structure not listed is unknown, never a target): theirs: Blue Guardian destroyed; Yellow Walker
  standing · ours: …` (destroyed is final) or `Structures: not read: do not name a specific
  Guardian, Walker or Shrine as the target`; the Rejuvenator (coach-review-8 D2/D11,
  `rejuvenatorLine()`): from `midboss_taken_by` and the clock the Midboss icon went away (the
  kill), plus 6 s landing and 180 s of credits: `Rejuvenator: WE hold it until about 32:58 (1:54
  left) (Midboss taken by our team at about 29:52; up to 3 revives in place): use it now with all
  five on a standing structure; never spend a credit alone`, `Rejuvenator: THEY hold it until about
  42:46 (…): their deaths may revive in place, so no fights in their base until it runs out`, `ours
  runs out in about 8 s` (last 20 s), `ours ran out at about 32:58` (for 2 minutes); a revive read
  by the vision (optional `revived`) counts a credit used. The throw text follows the holder ("fight
  only together while our Rejuvenator lasts" / "they hold the Rejuvenator").

**Word budgets** (`wordBudget()`, item 14): death screen 30 words / 2 sentences, during a fight 10 /
1, laning (before 8:00) and the 20 s after a respawn 15, otherwise 20 (also right after a fight);
Ask the coach now 30. `parseAdvice()` enforces them, cutting an over-long first sentence at its last
clause (":" ";" "," not inside a number) rather than mid-phrase. **Structures in replies**
(`structureProblem()`): once any structure state was read, a sentence sending us to an enemy
Guardian / Walker / Shrine / Patron that is read destroyed or never read standing is dropped
("names their Blue Guardian, read as destroyed"); "heal at our Guardian" is not a target.

Example (rec11, event-driven periodic request at 19:03, from `scripts/ai-coach-eval.ts`; the
profile, knowledge and excerpts that follow are left out):

```text
TASK: Coach what WHY NOW says changed, with one specific call on a theme not in THEMES COACHED (unless the change is a NEW trigger for it). PASS if nothing new and specific is worth the player's attention or the rule coach already covers it.

STATE
Trigger: periodic · clock 19:03 (midgame, 8:00 to 20:00) · screen Gameplay
WHY NOW: objective within 60 s: Soul Urn spawns at 20:00 (in 57s)
You: Violet (role not in the coach table: do not guess the kit) · level 22 · profile is mostly from Grey Talon games: check its weaknesses fit Violet
HP 1,935/1,935 (100%) · HP% 90s/60s/30s ago→now: 91→100→97→100
Souls 2,526 (saving: the usual buy now is a 3,200 item; 674 short) · Souls advice: not allowed (saving for the usual 3,200 buy)
Your items (8): Improved Spirit (1,600), Mystic Shot (1,600), Healing Booster (1,600), Radiant Regeneration (3,200), Restorative Shot (800), Spirit Shielding (1,600), Opening Rounds (1,600), Rapid Recharge (3,200) · invested by slot: weapon 4,000, vitality 3,200, spirit 8,000
Area: Yellow lane · zone YORK : DOCKS
Allies: 5 alive, dead: Celeste (5s), low HP: Paige 23%
Enemies: 6 alive, not seen recently: 1
Enemy roles: Lady Geist close-range spirit brawler; Dynamo support and teamfight initiator; Calico close-range diver; Graves ranged spirit caster with summons; others not in the coach table
Enemy items seen: Debuff Reducer, Extra Spirit, Healing Booster, Kinetic Dash, Titanic Magazine, Rapid Recharge, Melee Lifesteal, Compress Cooldown, Restorative Shot
Team souls: yours 92,000 vs enemy 93,000 (-1,000) · at 16:11 it was -1,000: steady
Minimap: 4 enemy heroes visible
Lanes: Green lane front 37% up (troopers 0 yours v 3 theirs); Blue lane front 56% up (troopers 0 yours v 0 theirs); Yellow lane front 42% up (troopers 2 yours v 1 theirs)
Objectives: Midboss alive (team target only after a won fight)
Upcoming: Powerups spawn at 20:00 (in 0:57); Soul Urn spawns at 20:00 (in 0:57)
Deaths this match: 2 (at 10:08, 11:55)
Last death at 11:55 (7:08 ago): Baba did at least 297 damage with Granny Long Legs, 27% of what the panel lists; cause Granny Long Legs · most damage Granny Long Legs 297 · carrying_urn, slowed
RECENT AI ADVICE (yours):
- [10:14] healing: Melee finished you again. Next life, hold max range behind your front line and heal above 60% before you go back in.
- [13:40] objective: Enemies are down: group with your team and push the furthest lane now.
- [14:27] team: Fall back to your Guardian with the living team and hold until your allies respawn.
- [15:35] economy: Go back to your Guardian shop and spend your 3,599 souls on the usual buy now.
THEMES COACHED (last 10 min; "blocked" = do not coach it again unless WHY NOW gives a new trigger for it):
- theme healing: 10:14 (free again)
- theme objective: 13:40 (free again)
- theme team: 14:27 (blocked)
- theme economy: 15:35 (blocked)
- weakness "range against divers" [Killed at close range]: 10:14 (allowed: new trigger or 5 min passed)
- weakness "healing before fights" [Gets burst down]: 10:14 (allowed: new trigger or 5 min passed)
- weakness "waiting for the team": 14:27 (blocked)
RECENT COACH CALLS: none
```

A good reply uses the Urn timing (who carries, which lane is safe for the delivery), names
Calico or Graves only with what STATE says, and does not restate "max range" (blocked unless an
enemy comes near) — or PASSes.

## Prompt design

- **Evidence only.** The model gets the same uncertainty rules as the rule engine: never invent
  positions, cooldowns, items or timers, and treat unknown as unknown, not safe. This stops
  confident advice that the screen cannot support.
- **One coach voice, one decision.** The rule coach and the model are one coach: "we" for the
  team, one teammate by name, a reason of six words or fewer, a decision for the next 30 seconds
  rather than a description of now, inside the word budget of the moment (20 words; 15 in laning
  or after a respawn; 10 in a fight; 30 on the death screen). `parseAdvice()` enforces the limits
  even if the model ignores them; when two sentences are too long it keeps the one with the
  action, not a bare situation.
- **Situation → specific action → why.** The model is asked for one idea with names (heroes,
  "Green lane", items with costs, timers) and the principle in a few words, and to read WHY NOW
  first. Green, Blue and Yellow are lanes, never heroes.
- **No repeats, varied themes.** Recent rule calls, the AI's own recent advice and THEMES COACHED
  are included; the model must not coach a blocked theme or weakness unless there is a new
  trigger, must never restate a profile weakness as a generic reminder (the rule coach does),
  and is told that PASS (`"speak": null`) is good. Code enforces it (see "Repetition and souls
  gating").
- **Review-style, tied to the player.** The model is told that the rule coach already covers
  routine reminders and to prefer what prevents deaths and wins fights, coaching a profile
  weakness when a COACHING ANGLE shows the situation matches it.
- **Souls only when they matter.** Souls advice only when the summary says "Souls advice:
  allowed"; holding less than the usual buy is saving, not hoarding.
- **Strict JSON with a reason.** `{"speak", "priority", "why", "focus", "weakness"}` (`focus` is
  one of the nine themes or `reinforce`; `weakness` the profile weakness coached, or null). `why` must cite the
  evidence, and the panel shows it, so you can judge each call. The parser accepts code
  fences, text around the JSON and `<think>` blocks from local reasoning models. Anything else
  is shown as an error, not spoken.
- **Grounded knowledge, small and relevant.** The coach brief contains the numbers that matter
  in most situations. Excerpts are chosen by keyword score for the current situation: a death
  pulls the common-mistakes sections on dying and unsecured souls, the jungle pulls jungle
  tactics, and unspent souls at or above the usual buy pull the hoarding and buy-timing
  sections (unsecured-souls sections on death only above the death-spend threshold). Hero matchup
  win rates are labelled as correlational.

## Integration points (for other modules)

- `setProfileProvider(fn: () => string | null)` from `web/src/ai-coach.ts`: returns a short
  player-profile text (at most 500 characters are used, hero names of other matches scrubbed). The
  app calls it on every request, and it must not throw.
- `setSkillStateProvider(fn: (skill) => string | null)`: the skill tracker (`mastery.state()`,
  registered by main.ts when the player profile is on). A skill read `mastered` / `assumed` is not
  coached: profile weaknesses about it are left out of PLAYER PROFILE (and its Focus line), the
  coaching angles that remind it (respawn → walk out with a teammate, hurt → heal first, divers →
  max range, alone with enemies missing) are dropped, and souls advice is not allowed for a
  mastered death-screen / base buy unless 1.5× the usual buy sits unspent. `setMasteryProvider`
  gives the PLAYER LEVEL block ("pitch the advice at this level").
- `setGoalProvider(fn: () => string | null)`: the match focus goal(s) with progress so far
  (`PlayerProfile.goalText()`, registered by main.ts), the `MATCH FOCUS GOAL` line.
- `RULE_THEMES` / `ruleThemes(key)` / `ruleCovered(theme, history, now)`: the rule-call themes by
  key prefix; `wordBudget()`, `structuresLine()`, `structureProblem()`, `deathCovered()`,
  `scrubProfile()`, `AiHistory.fightState()` / `windowLeft()` / `standing()` are exported for tests.
- `AiCoachDeps.items(id)` (optional): item name, cost and slot for the summary. `ai-coach-ui.ts`
  loads them from `assets/items.json`.
- `liveTriggers()`, `themeMemory()`, `reasoningParams()` and `usageOf()` are pure and exported
  for other modules (for example an end-of-game review) and tests.
- `AiHistory.setAbilities(rows)`: `data/abilities.json` rows (published as `assets/abilities.json`;
  `ai-coach-ui.ts` loads it) map damage-panel sources to heroes (`abilityIndex()`, `damageByHero()`).
  `HERO_NAMES` (all 44 heroes) backs `strangerHeroes()`; `AiHistory.rosterOf()`, `reviewsFor(death)`,
  `good`, `boundaries` and `reinforceGate()` are exported for tests and other modules.
- Optional snapshot fields are used when present: `status_effects`, `death_damage`,
  `death_killing_blow`, `level`, `unsecured_souls`, `kills` / `deaths` / `assists`,
  `minimap.zone`, `players` (scoreboard rows: enemy items, your K/D/A), `enemy_items`,
  `hero_status[].networth` (fed enemy), `structures`, `midboss_taken_by`, `abilities` (slot 4 =
  ultimate) and `match_result` (end-of-game review).

## End-of-game review

When a match ends, the AI coach (if enabled) writes one longer review of the whole match: a verdict,
what went well **and why it worked** (first), up to three improvements by impact (with timestamps,
why it matters and how to practice), what to keep doing, a next-game focus, hero tips, the match
focus goals scored, and a spoken version of about 200 words (150-230, a little over a minute) that
opens with the result and the goal score, then specific praise after a good game, and is read aloud
and shown as text. Logic: `web/src/ai-review.ts` (DOM-free); UI: `web/src/ai-review-ui.ts`.

**Triggers.** Automatic when the AI coach is on and Settings > "End-of-game review" is checked
(default on): the PostGame screen, or a match the profile closes after at least 10 minutes of
observed game time: a new match, a long gap, the **end watch**, or **Stop** / the shared window
closing. Replays are never reviewed automatically; the Player page button **Review my last match**
reviews the last match: a live match still open is closed first when capture is not running (a
match whose end was never seen), else the last finished match (live or the last completed
replay, from the replay's own snapshots and rule cues); after a reload, the Player profile's last
live match. One review per match (key: HUD match id + hero + end minute + deaths, because the HUD
"match id" repeats across matches); a failed request can be retried, a reply that is not the JSON is
stored and shown as raw text.

**How a match end is seen** (rec13: a 38-minute match coached with the AI on got no review). rec13
ended while dead: "ArchMother Defeat" over the death screen with "Respawn in 1" (frame 590, read
Dead), over the Tab scoreboard (591, Scoreboard), a black frame (592, Loading), the MVP screens
(593-596, Unknown) and the post-game summary with PLAY AGAIN and "ESC BACK" (597-599, read as Shop
with the last hero and an extrapolated clock). No frame was PostGame and none ended the match, and
the user stopped 26 s later. Now:
- Vision (`crates/vision/src/scan.rs`, `screen.rs`): the Defeat / Victory banner is also checked on
  Dead and Scoreboard frames, and a Shop-anchored frame with the **PLAY AGAIN** button is PostGame
  (the summary header names the losing Patron for both teams, so it is not a result). Fixtures
  `00570`-`00573`. Re-scanning rec5 / rec8 / rec10 / rec12 / rec13 changes PostGame only on
  frames checked on the screenshots: the banner over a death screen (rec13 590-591, rec8 584-585
  Defeat, rec10 576-577 Victory with "Respawn in 85") and summary screens (rec13 597-599, rec8 and
  rec10 593-598, rec12 0-63 and 100: rec11's summary).
- End watch (`MATCH_END` / `MatchEndWatch` in `web/src/player-profile.ts`, shared by the Player
  profile and the review tracker): after >= 10 min observed, a black Loading frame and >= 3 Loading
  / Unknown frames 20 s or more after the last live frame (a moving clock on Gameplay / Dead /
  Scoreboard, or a shop with souls or while dead) end the match at that live frame. Never while a
  respawn timer runs (its end + 20 s; 90 s on a death screen without a timer read). Menus (pause),
  countdowns, shops without souls and HUD frames with a frozen clock (pause, end camera) neither count
  nor reset; Unknown frames alone never end a match (rec6 had 3-minute Unknown runs mid-match;
  alt-tab), Unknown frames that read a clock are a missed HUD. After a black frame, non-live frames
  are not game time (rec8: the summary added 38 s and an alive frame to its last death). Recorded
  live snapshots: rec13 ends at frame 598 (22 s after the scoreboard), rec8 at 595, rec3 at its final
  black screen; no other recording ends early (`web/scripts/match-end-check.ts` prints where a
  recording ends and its digest).
- Stop / share ended after >= 10 min observed closes the match (Player profile and review); a
  shorter one stays open with a Coach notice. Visible: the Coach page notice says the review is
  being written, ready (link to the Player page) or failed, or why there is none (AI coach off,
  review off in Settings, invalid settings, match too short); the Player page list says so too.

**rec13 additions (items 13-14).** The result is the Victory / Defeat banner when the vision reads
it (`match_result`, also over a death screen or the scoreboard: "Result: loss (the Defeat banner)");
**FOCUS GOALS** right after STRENGTHS (never dropped): the match's focus goals (set at its start by
the Player profile, passed by `ai-review-ui.ts`; else the stored match's; else the ones the earlier
matches give, marked "not announced live"), each MET / MISSED / not measurable with its evidence
and, when missed, its practice tip; **LEAD AND THROW** (never dropped) when the team souls lead
peaked at 15k+ and lost 15k+ (or 5k+ in a loss), with the turn; in OBJECTIVES SEEN the Midboss
taker (`midboss_taken_by`) and the lane structures read destroyed (with the clock first read gone)
and still standing. Budget 9,000 characters. The prompt scores the goals first, names a thrown lead
before anything else after a loss, uses structures and the Midboss only as read, and speaks in the
live coach's voice ("you", "we", names, short reasons, next action); the spoken text is about 200
words (150-230) and opens with the result and the goal score; the reply adds `focus_goals[{goal,
met, note}]`. The stored review keeps the digest's scored goals (`goals`), and the review panel
shows "Focus goal: Met / Missed" even without a model reply.

rec13 digest (`scripts/ai-review-eval.ts`; earlier matches rec8, rec11, rec12; the Defeat banner
simulated as `match_result` on frames 590-591, as the vision now reads it), new sections:

```text
Hero: Grey Talon (hero_orion). Result: loss (the Defeat banner). Game clock 5:52 to 44:13, 38.4 min observed (live).
K/D/A (scoreboard read at 44:13): 23/5/11.

FOCUS GOALS (set at the match start): score each one first
- Spend at the usual buy (1,600 / 3,200 / 6,400): no death holding half an item tier: MISSED: broken 2 times (19:30, 29:26) in 6 deaths. Practice: Shop when your wave is safe, and never walk into a fight with an item in your pocket.
- No deaths in their half with 3+ enemies unseen: MISSED: broken 1 time (23:09) in 6 deaths. Practice: When three enemies are missing, stay on your side of the wave.

LEAD AND THROW
Team souls lead peaked at +55k at 34:23 and was +14k at the last read 43:49 (41k lost; it started to fall by 34:26), and the match was lost: name the throw and the moment it turned.

OBJECTIVES SEEN
Midboss icon disappeared (taken; by which team not read) at 30:13, 39:38.
Soul Urn carried by: you 15:15-15:50; you 25:31-26:04.
```

(The rec13 export predates the structure and Midboss-taker fields, so those lines are absent; the
unit tests cover them. The digest now counts 6 deaths, not 7: see docs/player-profile.md.)

**Match digest** (`buildDigest`, deterministic, at most 9,000 characters; observed facts only, "?" /
"not read" for unknowns). A private `ReviewTracker` segments matches exactly like the player profile
and keeps a compact timeline of the current match only (rec12's first ~100 frames are the previous
Violet match and are not in its digest). Sections: hero, role, result (or "likely win/loss" from the
Patron health banner at the end, marked as not read), clock span and observed minutes, K/D/A when
read, allies and enemies (most-read, so stale reads drop out); **STRENGTHS** right after (`strengthsOf()`,
never dropped for budget: K/D/A ratio ≥ 3, time dead ≤ 12% or improved, deathless stretches of 5+
minutes, enemy-dead windows used to push with the team souls swing across them, the urn delivered,
souls spent on the death screen, the team souls lead since when and how far, enemy Patron pressure,
low-health moments survived, fights joined, metrics improved on earlier matches; with a `Verdict: a
good game …` line after a win or a clean game, or "No clear strength observed: do not invent
praise."); team souls every 5 minutes; your net
worth at 10/20/30 min; unspent souls against the usual tier (1,600 before 10:00, 3,200 before 20:00,
6,400 after); time dead and the time shares by phase (lane / jungle / base / shop / dead); every death
(cause, damage panel rows, killing blow, close-range share of the panel, CC/status before, HP fall,
the top bar at the death ("3 of 5 teammates dead, 0 of 6 enemies dead"), in the enemy half (and
"deep, near their base"), alone (nearest teammate marker; your own icon read as an ally within 0.04
is ignored), forward with enemies unseen, seconds after respawn, the last walk-out from our base
with the teammates dead then, a hold / back-off rule call before it and whether it was ignored,
souls held vs the tier, seconds dead) with the rule coach's death review. Position facts come from
your marker in the last 12 s alive; when it was not read they are "not read: your position … (alone,
enemy half and forward are unknown)", never "no" (coach-review-8 D12: rec8 20:40, a solo dive deep in
their base, and 42:05, a walk-out with three teammates dead, both read "alone no; forward no").
Stored timelines without these fields keep the profile's alone / forward values; fights joined / missed / while dead with times; enemy-dead windows used
/ unused; objectives seen (Midboss take with "we held the Rejuvenator until about mm:ss", Rejuvenator
revives read via the optional `revived` field, Urn carries, Patron banner); the rule coach's pattern calls;
the comparison with earlier profile matches (per metric: this match vs the pooled newest 10, verdict
improved / worse / same; the profile's known weaknesses with their verdict); up to 3 short knowledge
excerpts (hero matchup stats, sections retrieved for the worst metrics). Budget: the lowest-priority
lines (excerpts first) are dropped until it fits.

| Recording | Hero | Deaths | Digest | Strengths | vs earlier matches |
|---|---|---|---|---|---|
| rec11 (live export) | Violet | 7 | 7,784 chars | 2 (deathless stretches, death-screen shopping); not a good game | deaths/10 min, unspent at death, fights missed: worse |
| rec12 (live export) | Grey Talon | 3 | 7,541 chars | 10; good game (likely win) | time dead 17% -> 7%, deaths/10 min 1.8 -> 1.0, unspent at death: improved |

rec12 STRENGTHS as sent (`scripts/ai-review-eval.ts`):

```text
STRENGTHS (10: what went well, with evidence)
- K/D/A 12/3/11: 23 kills and assists for 3 deaths (ratio 7.7): big fight impact while handing the enemy few kill bounties.
- Dead only 7% of the game (121 s of 1732 s, 3 deaths), against 17% over 2 earlier matches: more time on the map for farm, waves and fights.
- Deathless stretches: 0:31-9:48 (9.3 min), 15:27-22:10 (6.7 min), 23:16-28:49 (5.6 min).
- Enemy-dead windows turned into pushes: 13:54 (6 enemies dead, you in the enemy base), team souls lead +1k across the window; 27:40 (5 enemies dead, you in the enemy base), team souls lead +16k across the window: dead enemies cannot defend, so a structure or base push is free value.
- Delivered the Soul Urn: 16:33 (+2,244 souls): team souls and a secured objective.
- Spent on the death screen: after the 14:50 death about 5.9k spent: you walked back out stronger instead of carrying the souls.
- Team souls lead from 6:32 to the end: +3k then, +52k at the last read 28:49: the team kept converting its fights into souls.
- Pressure on the enemy Patron (ARCHMOTHER) 27:56-28:10: the game-ending objective.
- Joined every team fight observed (1, none missed).
- Better than your earlier matches: deaths per 10 min 1.0 vs 1.8; deaths holding half an item tier+ unspent 67% vs 85%; team fights missed 0% vs 24%.
Verdict: a good game (likely win). Lead with these strengths and why each worked; give at least as many went_well items as improvements.
```

**Prompt** (`REVIEW_SYSTEM_PROMPT`, `buildReviewPrompt`): elite Deadlock coach persona; use only the
digest, cite timestamps, never invent events, say when data is missing; **lead with what went well**
(STRENGTHS and improved metrics) and say **why each worked: cause → effect on the team's result**;
**balance**: after a good game (the Verdict line, a win, or few deaths) at least as many went_well
items as improvements, and never invented praise; rank improvements by impact (deaths and dead time,
souls spending, fight/objective presence, map and waves); hero- and enemy-specific. After a good game
the user line says "It went well: open with the strengths and why they worked, then the fixes, then
what to keep doing and the next focus." Reply: strict JSON `{summary, went_well[] (2-5),
improvements[{title, evidence, why_it_matters, how_to_practice}] (1-3), keep_doing[] (1-3),
next_game_focus, hero_tips[], spoken}`; the spoken text opens with genuine, specific praise when the
game went well (never a generic "good game"), then the fixes, and ends with what to keep doing and the
next-game focus. The review panel shows "What went well", "Top improvements", "Keep doing" and "Next
game". The rec12 prompt is about 12,300 characters (~3,080 tokens). Max reply tokens: `max(aiMaxTokens x 3, 3072)`, at
most 8,192, plus the same reasoning headroom as live calls; timeout 120 s; a reply cut off by the
token limit is retried once with 8,192 tokens, then parsed as far as it got (`partial`).
`parseReview` accepts fenced JSON, text around it, `<think>` blocks and cut-off JSON (closed at the
last complete field).

**Speech.** The coach `Speaker` keeps one pending cue for 15 s and stops an utterance after 25 s, so
the review is read by `ReviewReader`: sentence by sentence, each with its own watchdog, with **Speak**
/ **Stop** buttons per review. It is read automatically when "Speak AI advice" is on and the voice is
not muted; Speak is refused while muted; mute or a High coach call stops it.

**Storage.** `localStorage` key `deadlock-coach-match-reviews-v1`, newest 20 (`ReviewStore`), with
the digest for reference; **Export reviews** downloads them as JSON. The API key is never stored with
a review. A short notice appears on the Coach page when a review arrives.

**Offline evaluation:** `node --experimental-strip-types scripts/ai-review-eval.ts [--quiet] [--all]
[--digest rec11]` builds the rec11 / rec12 digests (sizes, determinism, comparison verdicts) and
prints the full rec12 prompt. With `AI_REVIEW_URL`, `AI_REVIEW_KEY`, `AI_REVIEW_MODEL` (optional
`AI_REVIEW_PROVIDER=anthropic`, `AI_REVIEW_REASONING`, `AI_REVIEW_MAX_TOKENS`) it sends the prompt and
prints the parsed review. Keys are never printed or written.

Example (illustrative, written by hand for this document from the rec12 digest; not a model output):

```json
{"summary": "Your cleanest game in the profile, and you helped win it: 3 deaths, 7% of the game dead (down from 17%), and you turned enemy-dead windows into base pushes.",
 "went_well": ["At 13:54 and 27:40 you were in their base while five or six enemies were dead: dead enemies cannot defend, and the team souls lead grew by 16k across the second window", "You delivered the Urn at 16:33 (+2.2k souls): a secured objective", "After dying at 14:50 you spent about 5.9k on the death screen, so you walked back out a full item stronger", "Dead only 7% of the game, with 9, 7 and 6 minute stretches without dying: more time farming and fighting"],
 "improvements": [{"title": "Keep range against Drifter", "evidence": "14:50 and 22:10: Rend and melee were most of the damage panel", "why_it_matters": "Drifter hunts isolated targets; each death cost 37-66 s", "how_to_practice": "When Drifter is missing, stand behind a teammate and keep one stamina charge to escape"}],
 "keep_doing": ["Push the moment two or more enemies are dead", "Shop on the death screen"],
 "next_game_focus": "Do not jungle alone after 20:00 while Drifter is missing.",
 "hero_tips": ["As Grey Talon, take fights from max range behind your front line"],
 "spoken": "That was your cleanest game so far, and you played a real part in the win. When five or six of them were dead you were already in their base, and that is exactly how a lead turns into structures ..."}
```

## Start-of-game pep talk

When a new match starts, the coach gives one short, personal briefing (at most 120 spoken words,
about 20-40 seconds) on what to focus on this game: the forward-looking twin of the end-of-game
review. Every player gets it: with the AI coach on, the model writes it from a compact pre-game
digest; without the AI coach (or when the request fails or the reply is unusable) the rule coach
speaks a template built from the same digest. Settings > Advice categories > **Start-of-game pep
talk** (default on). Logic: `web/src/pregame.ts` (DOM-free); UI: `web/src/pregame-ui.ts`.

**Content, in this order.** (a) One line about the last game from the Player profile: praise
first when the last counted match clearly improved ("Last game was a real step up: you were dead 7%
of the time, down from 17%"), honest when it was rough (>= 15% dead, >= 2.5 deaths per 10 minutes
or 7+ deaths: "Honest look at last game: 7 deaths and 17% of the game spent dead, but you delivered
the Soul Urn without dying"), else steady; the first coached match sets a baseline. (b) The 1-2
focus goals the profile chose for this match (`PlayerProfile.goals`), each in plain words with a
concrete trigger (`GOAL_TRIGGERS`: "when two enemies are dead, we push the nearest tower together.
Don't farm"; "when you have 1,600, buy at the next safe moment"). (c) Matchup: the 1-2 enemy heroes
of this match's roster that threaten the player most (`threatsFor`): the plans.json threat table
(+2, its danger and the counter for the player's role, Grey Talon / Vindicta = long range), the
review.json ability lessons (+1), close-range divers (+1, +2 against a long-range hero) and the
heroes that killed the player in the last 5 matches (+3 per death, up to 3: "Drifter caught you
twice recently"); one counter-play each and, for the first, the counter item of its need
(review.json `counters`: Reactive Barrier against stuns, Dispel Magic against Stalker's Mark,
Spirit Shielding against spirit burst). Heroes outside the tables get a counter-play from their kit
role (`HERO_ROLES`). (d) A short motivating close ("Same discipline as last game. Let's go." after
an improved one). The template drops the counter item, then the second threat, then the second goal
until it fits. The panel also shows the focus, the matchup tips and a one-line plan.

**Skill tracker consistency** (mastery.ts, wired by main.ts when the Player profile is on:
`skillState: mastery.state`, `level: mastery.promptText`). The pep talk never coaches a mastered
(or level-assumed) skill: `setGoalVeto` already keeps such goals from being chosen, and
`buildPregameDigest` also drops a goal set before the skill was mastered, the "Main issue" and
profile weaknesses about mastered skills (`TRAIT_SKILLS` / `masteredTrait()`), and the counter item
when counter items / threat tracking are mastered. The digest carries a `PLAYER LEVEL` block (the
system prompt: never coach a skill it lists as mastered, pitch the talk at that level), and the
template's focus and plan follow the level (basic: farm from max range, group when enemies die;
intermediate: catch waves, push every enemy-dead window together; advanced: plan objectives a minute
ahead, never fight alone with a lead).

**When.** `PregameWatch` sees every live full-scan frame (never replays): a match starts at the
first clocked HUD frame, a clock more than 2 minutes below the last read, 5 minutes without a frame,
or an early clock after the Countdown screen (Loading screens also occur mid-match, rec12 9:36, so
they are no boundary). A match first seen after 3:00 gets no pep talk (rec13 starts at 5:52: the
panel says so). The briefing waits until the roster is known: at least 0:35 and 3 fresh roster reads
with 5+ enemy heroes read twice; at 1:30 it comes without matchup lines if the roster is still
unknown; after 2:30 (no frames in the window) it is skipped. **Stale roster guard:** the vision held
the previous match's roster (rec11's) for rec12's first reads at 0:31-0:42, so rosters read 2+ times
in the previous match do not vote. The own hero is named only when 60% of the fresh reads agree and
it is in the ally roster. One pep talk per match (key `pregame:<start wall s>:<start clock>`); the
last one is kept across reloads (`deadlock-coach-pregame-v1`) with Replay and Stop. The Player
profile's start focus cue is not spoken at an early match start while the pep talk is on (the pep
talk says the goals).

**Speech.** Read sentence by sentence (`ReviewReader`, as the review) after the coach's current call
finished; respects mute, and "Speak AI advice" for the AI text (the template is the rule coach's
voice). The speech gate (`main.ts` wraps `speaker.say`): a **High** safety call stops the pep talk
at once and is spoken; Normal / Low calls wait until it ends (spoken if under 15 s old). Also logged
in Recent calls as `pregame:ai` / `pregame:template`. Not spoken after 3:00 (shown only).

**AI prompt.** System: `PREGAME_SYSTEM_PROMPT` (the structure above, digest facts only, heroes of
this match only, plain words, <= 110 words). User: the PRE-GAME DIGEST (hero and roles, allies,
enemies, LAST GAME with a verdict, the profile's top traits, FOCUS GOALS with their triggers,
THREATS TO YOU with counter-play and counter item, the correlational matchup line from
docs/heroes). Reply: one JSON object `{"spoken", "focus", "matchup_tips": [], "plan"}`; fenced or
cut-off replies are repaired, `spoken` is cut to 120 words at a sentence end, and a reply naming a
hero who is not in this match is rejected (the template is spoken instead, the panel says why).
`max_tokens` = max(1,024, the setting), timeout 30 s, exchange log trigger `pregame`.

**Offline check** (`node --max-old-space-size=8000 --experimental-strip-types scripts/pregame-eval.ts`,
rec11 -> rec12 -> rec13 in wall-clock order through the Player profile and the watch): rec11 (first
seen at 6:13) and rec13 (5:52) are skipped; rec12 (start 0:31, after rec11 in the profile) gets its
pep talk at 0:50, the stale rec11 roster of 0:31-0:42 ignored:

> Honest look at last game: 7 deaths and 17% of the game spent dead, but you delivered the Soul Urn
> without dying. Today we clean up the rest. Your focus this match: spend your souls. When you have
> 1,600, buy at the next safe moment, never walk into a fight with an item in your pocket. Watch
> Drifter: his mark reaches across the map, so stay next to a teammate. If Drifter gets ahead, Dispel
> Magic is your counter item. Watch Abrams: his Shoulder Charge stuns on walls, so keep a teammate
> between you. You know what to do. Let's have a great game. (104 words)

Known limit: in rec12 the vision read the own hero as Mirage until 5:15 (Grey Talon is on the
same team), so the 0:50 digest says "Your hero: Mirage" and uses the default role's counters. The
prompt is about 3.2k characters (~800 tokens). `PREGAME_AI_URL` / `PREGAME_AI_MODEL` /
`PREGAME_AI_KEY` send it to a gateway.

Tests: `web/tests/pregame.unit.ts` (watch, threat table, digest, template, JSON parsing, gateway,
store) and `web/tests/browser/pregame.spec.ts` (template and AI paths with a mocked gateway, Replay /
Stop, reload, the High-call gate, a mid-game join, the setting).