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
- **Advice interval:** default 40 s, minimum 15 s.
- **Max reply tokens:** default 200. Reasoning models may need more.
- **Speak AI advice:** respects **Mute voice**.
- **Extra request headers:** a JSON object, for example `{"HTTP-Referer": "https://example.com"}`
  for OpenRouter, or `{"api-key": "…"}` for an Azure-style proxy. These header values are
  treated as secrets in error messages too.

Request shapes:
- **OpenAI-compatible:** `POST {base}/chat/completions` with `Authorization: Bearer <key>`
  (left out when no key is set) and `{model, messages:[system,user], max_tokens, temperature: 0.3}`.
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

- **Periodic:** every *interval* seconds, but only while coaching live, alive, and on the
  Gameplay screen. It skips the request within 10 s of a High-priority rule call, such as
  low health.
- **Death:** once per death, after the rule coach's death review has been spoken or 4 s after
  the death panel has been read (8 s at most). This is a deeper "why you died and what to
  change" answer of up to 3 sentences, built from the damage panel, the killing blow, the
  status effects of the last 10 s alive, the health before death, who was near you and the
  rule death review (which names the killer hero). Opening the shop or scoreboard from the
  death screen is the same death, not a respawn followed by a second death.
- **On demand:** **Ask the coach now**. In Replay lab and when you are not live, the answer is
  shown but not spoken.

Safeguards:
- Only one request is in flight at a time.
- Live-trigger answers older than 15 s are dropped.
- Pause, Stop and starting a replay cancel the request (`AbortController`).
- After an error, automatic requests back off exponentially: 5 s, 10 s, 20 s, … up to 5 min.
  The backoff is shown in the panel and under **Runtime diagnostics → AI coach**.
- Spoken AI advice uses the existing speaker with cue keys `ai:<trigger>:<focus>` and a 30 s
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
- **No same focus twice in a row.** Answers are normalized to a topic (`topicOf()`: economy,
  positioning, health, team, objective, wave, map, jungle, death). An automatic periodic answer
  on the same topic as the previous AI call within 3 minutes is not spoken, unless it is
  High priority and not about souls. "souls" and "items" are the same topic, and an answer whose
  every sentence is about souls is economy whatever its tag.
- **No near-duplicates.** An answer whose content words overlap ≥ 60% (of the shorter text) with
  an AI call from the last 5 minutes or a rule call from the last 90 s is not spoken.
- **RECENT AI ADVICE.** The last 4 delivered AI calls (10 minutes) are in the prompt with their
  clock and topic, so the model can vary on its own.
- **Cheap retry only.** If an automatic answer is rejected and came back within 2.5 s, the coach
  asks once more with "YOUR PREVIOUS ANSWER WAS NOT USED (<reason>) … give advice on a different
  focus, or PASS". Otherwise it simply does not speak.
- **On demand is not gated.** **Ask the coach now** shows the model's answer as is (the prompt
  rules still apply).

## Review-style advice: coaching angles

The summary ends with **COACHING ANGLES**: up to 4 live situations with the fix a coach would
give, marked `[profile: <weakness>]` when the player profile (strengths/weaknesses from earlier
matches) has the matching weakness; matched angles come first. The model is told to prefer them.

| Situation (from the snapshot and history) | Angle | Profile weaknesses it matches |
|---|---|---|
| Respawned ≤ 45 s ago | walk out with a teammate or catch the wave by your Guardian | Dies soon after respawning, Dies alone |
| Health < 60% and not rising | heal to full first (shopping does not heal), leave on the first big hit | Goes back in hurt, Stays in at low health, Gets burst down |
| A close-range diver alive (Abrams, Shiv, Yamato, Mo & Krill, Calico, Lash, Victor, Drifter, Billy) and an enemy near you, a close-range last death, or the profile weakness | hold max range behind your front line, save the escape for their dive | Killed at close range, Same killer again |
| ≥ 2 enemies dead | group up and take a Guardian, Walker or the furthest lane | Enemy-dead windows unused |
| ≥ 2 allies dead and more than enemies | the fight is lost: back off together | Stays in lost fights |
| No ally near and ≥ 2 enemies missing | move toward a teammate | Dies alone, Pushes in with enemies missing |
| A status effect on you | out of stamina: don't start a trade; a debuff: disengage or cleanse | crowd control |

The latest rule death review (`death:review:*`, `active:close`, `active:repeat:*`) is quoted for
5 minutes after a death, so live advice can refer back to it.

## Offline evaluation

`web/scripts/ai-coach-eval.ts` replays a recording through the real controller with a **fake
model that answers "spend your souls" every time**, using a player profile learned from earlier
recordings, and prints what would have been spoken plus three full sample prompts (mid-game
periodic, death review, just after a respawn):

```powershell
cd web
node --experimental-strip-types scripts/ai-coach-eval.ts            # defaults to frames/rec8/replay1.jsonl
node --experimental-strip-types scripts/ai-coach-eval.ts --quiet     # counts and spoken/skipped list only
```

On rec8 (600 frames, 13:32–50:20): 94 requests (45 of them cheap retries) and 4 answers spoken,
all death reviews; 1 of them mentions souls (17:00, 2,695 souls ≥ the 1,600 death-spend
threshold). The other 90 answers (45 triggers, each with its retry) were skipped: 39 × "saving
for the usual 3,200 / 6,400 buy", 2 × "the rule coach already said it" (20:40, 39:17),
2 × "names a tier cheaper than 3,200" (18–19 min, "buy a 1,600 item") and 2 × "near-duplicate
of the AI call" (death reviews 3–4 min after an identical one). The death reviews at 28:02,
42:12 and 49:47 kept only their non-souls sentence. ("Same focus" never fired here because the
souls gate already stopped every periodic souls answer; the unit tests cover it.) The recordings are git-ignored, so the script is not part of `npm run test:unit`;
the unit tests cover the same rules.

## What is sent

The input budget is **about 2,500 tokens**, estimated as characters ÷ 4 and enforced by
`buildPrompt()` (`MAX_INPUT_CHARS = 10000`):

| Part | Budget | Source |
|---|---|---|
| System prompt | ~2,200 chars | `SYSTEM_PROMPT` in `web/src/ai-coach.ts` |
| Task line | ~250–500 chars | depends on the trigger (periodic / death / manual) |
| State summary | ≤ 3,000 chars | `buildSummary()`: the latest snapshot plus about 90 s of history, recent AI advice and coaching angles |
| Player profile | ≤ 900 chars | `setProfileProvider(fn)` callback, if registered |
| Coach brief | ~1,500 chars | `COACH_BRIEF`: key facts from `docs/timings` and `docs/strategy` |
| Matchup line | ≤ 300 chars | `docs/heroes/<hero>.md` win rates vs the enemies you face |
| Excerpts | ≤ 3 × 700 chars | sections from strategy/jungle/timings docs, ranked by keyword score |

If the input is over budget, the app shrinks the parts in this order: excerpts, then the
profile, then the summary. Low-value summary lines (for example, lanes) are dropped before
high-value ones (deaths, HP, souls, recent calls).

The summary uses only fields the app has read, and the model is told that "unknown" never
means safe. It includes:
- the clock and phase
- your hero, level and K/D/A (when read)
- HP and its 90/60/30-second trend
- status effects
- souls against the usual buy for the game time (saving / can afford), unsecured souls, how long
  you have held them, and whether souls advice is allowed (see the souls gate above)
- "Respawned N s ago" for 45 s after a respawn
- your items
- your lane, how far up the lane you are, and the zone
- allies and enemies alive, dead (with respawn timers), not seen recently, or at low HP
- team souls
- enemies visible, near you, or missing on the minimap
- lane fronts
- Midboss and Urn
- the next objective timers
- deaths, including the damage breakdown, killing blow, status effects before death, allies and
  enemies near you, and whether the death-spend threshold was reached
- the rule coach's death review for the latest death
- RECENT AI ADVICE (what the AI already said, with topics) and recent rule calls
- COACHING ANGLES (live situation × player-profile weakness)

Example (rec8, death review at 28:02, from `scripts/ai-coach-eval.ts`, abridged):

```text
TASK: The player just died. Explain WHY from the death evidence (damage by source, killing blow, killer hero named by the rule death review, status effects or crowd control before death, health before, allies near), then give the ONE change for the next life, tied to a PLAYER PROFILE weakness when it matches. Talk about souls only if STATE says "Souls advice: allowed". Up to 3 short sentences, at most 45 words.

STATE
Trigger: death · clock 28:02 (lategame) · screen Dead · YOU ARE DEAD
You: Grey Talon · level 27
Souls 2,418 on the death screen (below the 3,200 death-spend threshold, half the usual 6,400 buy) · Souls advice: not allowed (2,418 is below the 3,200 death-spend threshold: not a souls death)
Allies: 3 alive, dead: Grey Talon (56s), Shiv (7s), Venator (24s)
Enemies: 5 alive, dead: Kelvin (41s), not seen recently: 3
Death at 27:57: cause Melee · damage taken: Melee 525, Flying Slash 501, Gun Damage 389, Melee 131, Express Shot 124, Power Slash 121 · killing blow Melee · last alive HP 32% · allies near you 0, enemies near 0 · unspent before death 2,394 · death-spend threshold 3,200: not reached · respawn 56s
Rule death review (already spoken): Why you died: Yamato did 35% of the damage (622) with Flying Slash and Power Slash. You went from 86% to dead in 15 seconds. You were in their half with 4 enemies not seen by your team. …

PLAYER PROFILE (from earlier matches; coach these weaknesses when the live situation matches)
Player profile (4 matches, mostly Grey Talon, 16% dead, 2.6/10min deaths). Weak: Killed at close range: 6 of 7 deaths with a known cause came from melee range …; Dies soon after respawning: 8 of 12 deaths within 90 s of respawning [improving]; …
```

A good reply cites Yamato's slashes, being alone in their half with four enemies unseen, and one
change (walk out with a teammate and hold max range behind the front line) — and no souls.

## Prompt design

- **Evidence only.** The model gets the same uncertainty rules as the rule engine: never invent
  positions, cooldowns, items or timers, and treat unknown as unknown, not safe. This stops
  confident advice that the screen cannot support.
- **One fix, short, imperative.** Advice is spoken mid-fight. The limit is two sentences and
  30 words (three sentences and 45 words for a death review). `parseAdvice()` enforces these
  limits even if the model ignores them.
- **No repeats, varied topics.** Recent rule calls and the AI's own recent advice are included;
  the model must not reuse a focus within 3 minutes and must not repeat rule calls unless it
  adds a concrete how-to. `PASS` (`"speak": null`) is an explicit, cheap option. Code enforces it
  (see "Repetition and souls gating").
- **Review-style, tied to the player.** The model is told that the rule coach already covers
  routine reminders and to prefer what prevents deaths and wins fights, coaching a profile
  weakness when a COACHING ANGLE shows the situation matches it.
- **Souls only when they matter.** Souls advice only when the summary says "Souls advice:
  allowed"; holding less than the usual buy is saving, not hoarding.
- **Strict JSON with a reason.** `{"speak", "priority", "why", "focus"}`. `why` must cite the
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
  player-profile text (at most 900 characters are used). The app calls it on every request, and
  it must not throw.
- Optional snapshot fields are used when present: `status_effects`, `death_damage`,
  `death_killing_blow`, `level`, `unsecured_souls`, `kills` / `deaths` / `assists` and
  `minimap.zone`.
