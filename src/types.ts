export type Level = [string, string];
export interface TapeRow {
  id: string;
  at_ms: number;
  price: string;
  qty: string;
  aggressor: "buy" | "sell";
}
export interface Sample {
  at_ms: number;
  price: string | null;
  cvd: string;
}
export interface Session {
  id: number;
  source: string;
  started_ms: number;
  event_count: number;
  closed: boolean;
}
export interface View {
  mode: "simulation" | "live" | "replay";
  health: string;
  note: string;
  book_state: string;
  trade_state: string;
  book_age_ms: number | null;
  trade_age_ms: number | null;
  event_offset_ms: number | null;
  update_id: string | null;
  bids: Level[];
  asks: Level[];
  spread: string | null;
  imbalance: number | null;
  last_price: string | null;
  cvd: string;
  buy_volume: string;
  sell_volume: string;
  cvd_complete: boolean;
  trade_count: number;
  duplicates: number;
  gaps: number;
  tape: TapeRow[];
  history: Sample[];
  now_ms: number;
  paused: boolean;
  speed: number;
  recording: number | null;
  recorded_events: number;
  recording_error: string | null;
  db_path: string;
  replay: {
    session_id: number;
    cursor: number;
    total: number;
    finished: boolean;
  } | null;
}
export type Request =
  | { action: "view" | "sessions" | "inject_gap" }
  | { action: "mode"; mode: "simulation" | "live" }
  | { action: "pause"; paused: boolean }
  | { action: "speed"; speed: number }
  | { action: "record"; enabled: boolean }
  | { action: "replay"; session_id: number };
