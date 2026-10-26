import type { Cue, Settings } from './types';
export class Speaker {
  private generation = 0;
  private talking = false;
  private pending?: { cue: Cue; at: number };
  private timer?: ReturnType<typeof setTimeout>;
  private settings: Settings;
  onStatus: (s: string) => void = () => {};
  constructor(settings: Settings) { this.settings = settings; }
  configure(settings: Settings) { this.settings = settings; if (settings.muted) this.cancel(); }
  voices() { return window.speechSynthesis?.getVoices() || []; }
  cancel() {
    this.generation++; this.talking = false; this.pending = undefined;
    clearTimeout(this.timer); window.speechSynthesis?.cancel();
  }
  say(cue: Cue, test = false) {
    if ((!test && this.settings.muted) || !window.speechSynthesis) return;
    if (this.talking) {
      if (cue.priority === 'High') this.cancel();
      else { this.pending = { cue, at: Date.now() }; return; }
    }
    const generation = this.generation;
    const u = new SpeechSynthesisUtterance(cue.text);
    const voices = this.voices();
    const voice = voices.find(v => v.voiceURI === this.settings.voice)
      || voices.find(v => v.localService && v.lang.startsWith('en'));
    if (voice) u.voice = voice;
    u.rate = this.settings.rate; u.volume = this.settings.volume;
    this.talking = true; this.onStatus(voice?.localService ? 'Speaking · local voice' : 'Speaking · system voice');
    const finish = (message = 'Voice ready') => {
      if (generation !== this.generation) return;
      clearTimeout(this.timer); this.talking = false; this.onStatus(message);
      const next = this.pending; this.pending = undefined;
      if (next && Date.now() - next.at < 15000) this.say(next.cue);
    };
    u.onend = () => finish(); u.onerror = () => finish('Voice unavailable — check browser audio');
    // Avoid an indefinitely stuck OS/browser speech queue.
    this.timer = setTimeout(() => { if (generation === this.generation) { this.cancel(); this.onStatus('Speech timed out; queue cleared'); } }, 25000);
    window.speechSynthesis.speak(u);
  }
}
