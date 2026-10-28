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
- **Death:** once per death, after the death panel has been read (or 2.5 s after death). This
  is a deeper "why you died and what to change" answer of up to 3 sentences.
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

## What is sent

The input budget is **about 2,500 tokens**, estimated as characters ÷ 4 and enforced by
`buildPrompt()` (`MAX_INPUT_CHARS = 10000`):

| Part | Budget | Source |
|---|---|---|
| System prompt | ~1,300 chars | `SYSTEM_PROMPT` in `web/src/ai-coach.ts` |
| Task line | ~250 chars | depends on the trigger (periodic / death / manual) |
| State summary | ≤ 3,000 chars | `buildSummary()`: the latest snapshot plus about 90 s of history |
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
- unspent and unsecured souls, how long you have held them, and the item tier you can afford
- your items
- your lane, how far up the lane you are, and the zone
- allies and enemies alive, dead (with respawn timers), not seen recently, or at low HP
- team souls
- enemies visible, near you, or missing on the minimap
- lane fronts
- Midboss and Urn
- the next objective timers
- deaths, including the damage breakdown and killing blow when available
- recent spoken calls, so the model does not repeat them

Example request (Anthropic, death review, abridged):

```json
{
  "model": "claude-3-5-haiku-latest", "max_tokens": 300, "temperature": 0.3,
  "system": "You are an elite Deadlock coach … Reply with ONLY one JSON object …",
  "messages": [{ "role": "user", "content": "TASK: The player just died. Using the death evidence, say the most likely reason and the one change for the next life …\n\nSTATE\nTrigger: death · clock 11:45 (midgame) · screen Dead · YOU ARE DEAD\nYou: Haze\nUnspent souls 2,400 (unsecured 600) · can afford a 1,600 item\nYour items (1): Headshot Booster\nArea: Blue lane, 70% up lane\nEnemy heroes: Graves\nObjectives: Midboss icon visible (up)\nUpcoming: Guardians reach minimum resist at 12:00 (in 0:15)\nDeath at 11:45: cause Graves · last alive HP 30% · unspent before death 2,400 (unsecured 600) · respawn 20s\nRECENT COACH CALLS (already spoken):\n- [11:40] rule: You have 2,400 unspent souls.\n\nKNOWLEDGE\nDEADLOCK FACTS …\nMatchup stats for Haze (correlational win rates): weak vs Graves 47.2%.\n\nREFERENCE EXCERPTS\n[Common Mistakes › 5. Dying with haunted (unsecured) souls] …\n\nReply with the JSON object only." }]
}
```

Expected reply:
`{"speak": "Graves caught you 70% up Blue with 2,400 unspent souls. Spend at the shop before you walk back, and hold near your Guardian until the wave arrives.", "priority": "Normal", "why": "death at 11:45 to Graves, 2,400 unspent, lane pushed to 70%", "focus": "death"}`

## Prompt design

- **Evidence only.** The model gets the same uncertainty rules as the rule engine: never invent
  positions, cooldowns, items or timers, and treat unknown as unknown, not safe. This stops
  confident advice that the screen cannot support.
- **One fix, short, imperative.** Advice is spoken mid-fight. The limit is two sentences and
  30 words (three sentences and 45 words for a death review). `parseAdvice()` enforces these
  limits even if the model ignores them.
- **No repeats.** Recent rule-based calls are included, and the model is told not to repeat
  them unless it adds a concrete how-to. `PASS` (`"speak": null`) is an explicit, cheap option.
- **Strict JSON with a reason.** `{"speak", "priority", "why", "focus"}`. `why` must cite the
  evidence, and the panel shows it, so you can judge each call. The parser accepts code
  fences, text around the JSON and `<think>` blocks from local reasoning models. Anything else
  is shown as an error, not spoken.
- **Grounded knowledge, small and relevant.** The coach brief contains the numbers that matter
  in most situations. Excerpts are chosen by keyword score for the current situation: a death
  pulls the common-mistakes sections on dying and unsecured souls, the jungle pulls jungle
  tactics, and many unspent souls pull the hoarding and buy-timing sections. Hero matchup
  win rates are labelled as correlational.

## Integration points (for other modules)

- `setProfileProvider(fn: () => string | null)` from `web/src/ai-coach.ts`: returns a short
  player-profile text (at most 900 characters are used). The app calls it on every request, and
  it must not throw.
- Optional snapshot fields are used when present: `status_effects`, `death_damage`,
  `death_killing_blow`, `level`, `unsecured_souls`, `kills` / `deaths` / `assists` and
  `minimap.zone`.
