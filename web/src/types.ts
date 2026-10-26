export type Priority = 'Low' | 'Normal' | 'High';
export interface Cue { key: string; text: string; priority: Priority; cooldown_secs: number }
export interface Point { x: number; y: number }
export interface LaneState { ally_troopers: number; enemy_troopers: number; ally_progress?: number | null; enemy_progress?: number | null; push?: number | null }
export interface MinimapState {
  observations: number; green: LaneState; blue: LaneState; yellow: LaneState;
  midboss_up: boolean; urn?: 'Visible' | 'Ground' | 'CarriedByAlly' | 'CarriedByEnemy' | 'CarriedByYou' | null;
  urn_pos?: Point | null; enemies: Point[]; allies: Point[]; enemy_count: number;
}
export interface Snapshot {
  game_clock_secs?: number | null; souls?: number | null; screen?: string | null;
  dead?: boolean; respawn_secs?: number | null; death_cause?: string | null; match_id?: number | null;
  items?: string[]; enemy_items?: string[]; ally_heroes?: string[]; enemy_heroes?: string[];
  ally_networth?: number | null; enemy_networth?: number | null;
  minimap?: MinimapState | null; players?: { ally: boolean; hero?: string; items: string[] }[];
}
export interface Reading { status: 'ok' | 'missing' | 'pending' | 'skipped' | 'error'; value: unknown; score: number | null;
  bounds: { x: number; y: number; w: number; h: number } | null; source: string }
export interface StageTrace extends Omit<Reading, 'source'> { id: string; operation: string; inputs: string[]; elapsed_ms: number;
  image_size: [number, number] | null; error: string | null }
export interface PerceptionReport { schema_version: number; profile_id: string; frame_id: number; timestamp_ms: number;
  profile_revision: string; frame_size: [number, number]; fields: Record<string, Reading>; stages: StageTrace[] }
export interface Analysis { snapshot: Snapshot; cues: Cue[]; perception?: PerceptionReport | null }
export interface Options { timers: boolean; economy: boolean; map: boolean; counters: boolean; deaths: boolean }
export interface Settings {
  muted: boolean; voice: string; rate: number; volume: number; interval: number; record: boolean; autoCrop: boolean;
  options: Options; crop: { x: number; y: number; w: number; h: number };
}
export interface RecordedFrame { id?: number; created: number; timestamp: number; snapshot: Snapshot; cues: Cue[]; image: Blob; perception?: PerceptionReport | null; profile_id?: string }
export interface Manifest { schema: number; version: string; files: Record<string, { bytes: number; sha256: string }> }
export interface Knowledge { id: string; title: string; category: string; body: string }
export interface GameItem { class_name: string; name: string; cost: number; tier: number }
export type WorkerReply = { type: 'progress'; message: string; fraction: number }
  | { type: 'ready'; version: string }
  | { type: 'result'; id: number; generation: number; analysis: Analysis; elapsed: number }
  | { type: 'error'; message: string; id?: number };
export type ProfileReply = { type: 'profile_ready'; requestId: number } | { type: 'profile_error'; requestId: number; message: string };
