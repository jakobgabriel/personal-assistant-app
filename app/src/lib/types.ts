// Spiegel der Typen aus `crates/tory-core/src/model.rs` und `config.rs`.
//
// Bewusst von Hand gepflegt statt generiert: es sind wenige Typen, und eine
// Abweichung faellt beim ersten `svelte-check` auf. Wer hier etwas aendert,
// aendert es auch drueben — die Namen sind absichtlich identisch.

export type SourceKind = "obsidian" | "mindwtr" | "nocodb" | "gmail" | "feeds";
export type Urgency = "critical" | "high" | "normal" | "info";
export type TimeKind = "at" | "due" | "window" | "since";

export interface SourceRef {
  kind: SourceKind;
  instance: string;
  label: string;
}

export type Action =
  | { kind: "open_url"; url: string }
  | { kind: "open_note"; vault: string; path: string }
  | { kind: "show_note"; source: string; path: string }
  | { kind: "complete_task"; source: string; task_id: string }
  | { kind: "open_detail"; source: string; item_id: string };

export interface Signal {
  id: string;
  source: SourceRef;
  title: string;
  subtitle?: string;
  excerpt?: string;
  at?: string;
  time_kind?: TimeKind;
  window_end?: string;
  urgency: Urgency;
  badge?: string;
  tags?: string[];
  action?: Action;
  completable?: boolean;
  dedup_key?: string;
}

export interface OverviewLine {
  text: string;
  note?: string;
  urgency?: Urgency;
}

export interface Overview {
  source: SourceRef;
  metric: string;
  metric_raw?: number;
  caption: string;
  note?: string;
  lines?: OverviewLine[];
  progress?: number;
}

export type SyncFault =
  | { kind: "offline" }
  | { kind: "auth_expired"; detail: string }
  | { kind: "rate_limited"; retry_after?: string }
  | { kind: "misconfigured"; detail: string }
  | { kind: "server"; status: number; detail: string }
  | { kind: "unknown"; detail: string };

export interface SyncState {
  source: string;
  last_ok?: string;
  last_attempt?: string;
  fault?: SyncFault;
  failures: number;
}

export interface Card {
  source: SourceRef;
  overview: Overview | null;
  sync: SyncState;
  enabled: boolean;
}

export interface Alert {
  source: SourceRef;
  message: string;
  needs_action: boolean;
}

export interface Dashboard {
  now: string;
  top: Signal[];
  rest: Signal[];
  cards: Card[];
  alerts: Alert[];
}

export interface SyncReport {
  synced: string[];
  failed: { source: string; message: string }[];
  skipped: string[];
}

export interface Completion {
  text: string;
  input_tokens?: number;
  output_tokens?: number;
  model: string;
}

export interface AppInfo {
  version: string;
  platform: string;
  data_dir: string;
}

// --- Konfiguration ---------------------------------------------------------

export interface Cadence {
  minutes: number;
}

/** In Rust per `#[serde(flatten)]` eingebettet — deshalb hier flach. */
export interface SourceCommon {
  instance: string;
  label: string;
  enabled: boolean;
  cadence: Cadence;
  order: number;
}

export type VaultAccess =
  | { kind: "local"; path: string }
  | { kind: "webdav"; base_url: string; username: string; password_key: string };

export interface ObsidianSource extends SourceCommon {
  vault_name: string;
  access: VaultAccess;
  include_folders: string[];
  exclude_folders: string[];
  read_tasks: boolean;
  pinned_tags: string[];
  scan_limit: number;
}

export interface MindwtrSource extends SourceCommon {
  base_url: string;
  token_key: string;
  statuses: string[];
  include_undated: boolean;
  horizon_days: number;
}

export interface NocodbTableMap {
  table_id: string;
  label: string;
  view_id?: string;
  title_field: string;
  subtitle_field?: string;
  date_field?: string;
  status_field?: string;
  done_values: string[];
  filter?: string;
  limit: number;
}

export interface NocodbSource extends SourceCommon {
  base_url: string;
  token_key: string;
  tables: NocodbTableMap[];
}

export interface GmailQuery {
  label: string;
  query: string;
  urgency: string;
}

export interface GmailSource extends SourceCommon {
  client_id: string;
  redirect_uri: string;
  queries: GmailQuery[];
  per_query_limit: number;
}

export type FeedTopic = "local" | "global" | "weather";

export interface Feed {
  url: string;
  label: string;
  topic: FeedTopic;
  enabled: boolean;
}

export interface FeedsSource extends SourceCommon {
  feeds: Feed[];
  headline_limit: number;
  max_age_hours: number;
}

export type AiProviderKind = "anthropic" | "open_ai" | "ollama";

export interface AiConfig {
  enabled: boolean;
  provider: AiProviderKind;
  model: string;
  base_url?: string;
  api_key_key?: string;
  max_tokens: number;
  titles_only: boolean;
}

export interface DashboardConfig {
  top_signals: number;
  morning_brief_at?: string;
  theme: string;
  card_order: string[];
}

export interface Config {
  version: number;
  dashboard: DashboardConfig;
  obsidian: ObsidianSource[];
  mindwtr: MindwtrSource[];
  nocodb: NocodbSource[];
  gmail: GmailSource[];
  feeds: FeedsSource[];
  ai: AiConfig;
}

export const sourceId = (s: SourceRef): string => `${s.kind}:${s.instance}`;
export const signalKey = (s: Signal): string => `${sourceId(s.source)}/${s.id}`;
