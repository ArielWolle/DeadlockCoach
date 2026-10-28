export type Priority = 'Low' | 'Normal' | 'High';
/** Parallel line-recognition pool status, reported with each analysis result. */
export interface PoolStatus { state: string; active: boolean; avg_ms: number | null; fallbacks: number }
export interface Cue { key: string; text: string; priority: Priority; cooldown_secs: number; timer_spawn_ms?:number }
export interface Point { x: number; y: number }
export interface LaneState { ally_troopers: number; enemy_troopers: number; ally_progress?: number | null; enemy_progress?: number | null; push?: number | null }
export interface MinimapState {
  observations: number; green: LaneState; blue: LaneState; yellow: LaneState;
  midboss_up: boolean; urn?: 'Visible' | 'Ground' | 'CarriedByAlly' | 'CarriedByEnemy' | 'CarriedByYou' | null;
  urn_pos?: Point | null; enemies: Point[]; allies: Point[]; enemy_count: number;
  you?:Point|null; your_lane?:'Green'|'Blue'|'Yellow'|null; your_progress?:number|null;
  /** Hero rings tracked across frames; unseen_ms > 0 means last-seen (missing), not dead. */
  heroes?: HeroTrack[];
  /** true = Yellow drawn on the left (latched per match). */
  lanes_mirrored?: boolean | null;
  /** Zone label under the minimap, e.g. "YORK : FACTORY". */
  zone?: string | null; zone_lane?: 'Green' | 'Blue' | 'Yellow' | null;
  zone_area?: 'Lane' | 'Jungle' | 'Mid' | 'OwnBase' | 'EnemyBase' | 'Base' | null;
}
export interface HeroTrack { id: number; ally: boolean; pos: Point; unseen_ms: number; hits: number; velocity?: Point | null; lane?: 'Green' | 'Blue' | 'Yellow' | null }
export interface Snapshot {
  hero?:string|null;own_networth?:number|null;hero_status?:HeroStatus[];
  game_clock_secs?: number | null; souls?: number | null; screen?: string | null;
  dead?: boolean; respawn_secs?: number | null; death_cause?: string | null; match_id?: number | null;
  items?: string[]; enemy_items?: string[]; ally_heroes?: string[]; enemy_heroes?: string[];
  ally_networth?: number | null; enemy_networth?: number | null;
  minimap?: MinimapState | null; players?: { ally: boolean; hero?: string; items: string[]; kills?: number | null; deaths?: number | null; assists?: number | null }[];
  kills?: number | null; deaths?: number | null; assists?: number | null;
  hp?:number|null; max_hp?:number|null; health_observations?:number; health_delta_ratio?:number|null;
  coaching_context?:{focus_lane?:'Green'|'Blue'|'Yellow'|null};
  /** Age (ms) of `souls`/`respawn_secs`: 0 = read this frame, >0 = held/extrapolated (≤10 s). */
  souls_age_ms?: number | null; respawn_age_ms?: number | null;
  level?: number | null;
  /** "N HAUNTED SOULS"; null = not read (not zero). */
  unsecured_souls?: number | null;
  death_damage?: DamageRow[]; death_killing_blow?: string | null; death_total_damage?: number | null;
  /** e.g. "stunned", "silenced", "out_of_stamina", "knocked_off_zipline", or a debuff name. Empty = none read. */
  status_effects?: string[];
}
export interface DamageRow { source: string; hits?: number | null; damage: number }
export interface HeroStatus {ally:boolean;slot:number;hero?:string|null;critical?:boolean|null;health_ratio?:number|null;networth?:number|null;networth_observations:number;observations:number;position?:Point|null;position_observations:number;lane?:string|null;
  /** false = enemy not seen recently (no top-bar health bar); not dead. */
  bar_visible?:boolean|null;health_observations?:number;dead?:boolean|null;respawn_secs?:number|null;
  /** Consecutive respawn reads consistent with a 1 s/s countdown. */
  respawn_observations?:number}
export interface Reading { status: 'ok' | 'missing' | 'pending' | 'skipped' | 'error'; value: unknown; score: number | null;
  bounds: { x: number; y: number; w: number; h: number } | null; source: string; bounds_origin?:'configured'|'detected' }
export interface StageTrace extends Omit<Reading, 'source'> { id: string; operation: string; inputs: string[]; elapsed_ms: number;
  image_size: [number, number] | null; error: string | null;bounds_origin?:'configured'|'detected';
  regions?:Array<{id:string;label:string;bounds:{x:number;y:number;w:number;h:number};origin:'configured'|'detected';score:number|null}> }
export interface PerceptionReport { schema_version: number; profile_id: string; frame_id: number; timestamp_ms: number;
  profile_revision: string; frame_size: [number, number]; fields: Record<string, Reading>; stages: StageTrace[] }
export interface TacticalDecision {action:string;text:string;rationale:string;lane:string|null;lane_source:string;evidence:string[];missing:string[];
  sources:Array<{path:string;section:string;basis:string;excerpt:string}>;conditional:boolean;cue_key:string;priority:Priority;speak:boolean}
export interface Analysis { snapshot: Snapshot; cues: Cue[]; perception?: PerceptionReport | null; tactics?:TacticalDecision|null }
export interface Options { timers: boolean; economy: boolean; map: boolean; counters: boolean; deaths: boolean; laning:boolean }
export interface Settings {
  muted: boolean; voice: string; rate: number; volume: number; interval: number; record: boolean; autoCrop: boolean;
  options: Options; crop: { x: number; y: number; w: number; h: number };
  laneFocus:'auto'|'Green'|'Blue'|'Yellow';
  heroFocus:string;
  // --- Optional AI coach (web/src/ai-coach.ts). Browser-only; never sent to the Rust engine. ---
  aiEnabled: boolean; aiProvider: 'openai' | 'anthropic'; aiBaseUrl: string; aiModel: string;
  /** Stored only in this browser's localStorage; never logged or exported. */
  aiKey: string; aiInterval: number; aiMaxTokens: number; aiSpeak: boolean; aiHeaders: string;
  /** Player profile (player-profile.ts): learn from matches and speak one focus reminder. Local only. */
  playerProfile:boolean;
}
export interface RecordedFrame { id?: number; created: number; timestamp: number; snapshot: Snapshot; cues: Cue[]; image: Blob; perception?: PerceptionReport | null; profile_id?: string }
export interface Manifest { schema: number; version: string; files: Record<string, { bytes: number; sha256: string }> }
export interface Knowledge { id: string; title: string; category: string; body: string }
export interface GameItem { class_name: string; name: string; cost: number; tier: number }
export type WorkerReply = { type: 'progress'; message: string; fraction: number }
  | { type: 'ready'; version: string }
  | { type: 'result'; id: number; generation: number; analysis: Analysis; elapsed: number; pool?: PoolStatus }
  | { type: 'error'; message: string; id?: number };
export type ProfileReply = { type: 'profile_ready'; requestId: number } | { type: 'profile_error'; requestId: number; message: string };
