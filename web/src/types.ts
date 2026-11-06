export type Json = unknown;
export interface Profile {
  name: string;
  url: string | null;
  type: Json;
  no_pp: boolean;
  update_with_proxy: boolean;
}
export interface State {
  core: string;
  current: string;
  revision: string;
  profiles: Profile[];
  templates: string[];
  service_running: boolean | null;
  system_proxy: boolean | null;
  service_install: boolean;
}
export interface Document {
  kind: "profile" | "template" | "override";
  name: string;
  core: string;
  revision: string;
  content: string;
}
export interface LogRow {
  id: number;
  time: number;
  value: { type: string; payload: string };
}
export interface StreamState {
  generation?: string;
  reset?: boolean;
  traffic: { up?: number | null; down?: number | null } | null;
  memory: { inuse: number } | null;
  connections: Connections | null;
  logs: LogRow[];
  cursor: number;
  dropped: boolean;
  status: Record<
    string,
    { connected: boolean; updated_at: number; error?: string | null }
  >;
}
export interface Resource {
  id: string;
  value: {
    name?: string;
    type?: string;
    vehicleType?: string;
    payload?: string;
    proxy?: string;
    disabled?: boolean;
    ruleCount?: number;
    updatedAt?: string;
    proxies?: Proxy[];
    subscriptionInfo?: {
      Upload?: number;
      Download?: number;
      Total?: number;
      Expire?: number;
    };
    [key: string]: unknown;
  };
}

export interface Proxy {
  name: string;
  type: string;
  all?: string[];
  now?: string;
  fixed?: string;
  udp?: boolean;
  history?: { time?: string; delay: number }[];
  [key: string]: unknown;
}
export interface Connection {
  id: string;
  upload?: number;
  download?: number;
  chains?: string[];
  rule?: string;
  rulePayload?: string;
  metadata?: {
    host?: string;
    destinationIP?: string;
    destinationPort?: string | number;
    process?: string;
    processPath?: string;
    network?: string;
    [key: string]: unknown;
  };
  [key: string]: unknown;
}
export interface Connections {
  connections: Connection[];
  uploadTotal?: number;
  downloadTotal?: number;
  [key: string]: unknown;
}
export interface ActionResult {
  config?: CoreResponses["runtime"];
  revision?: string;
  content?: string;
  url?: string;
  valid?: boolean;
  partial_failure?: boolean;
  results?: { ok: boolean; [key: string]: unknown }[];
  current?: Resource[];
  [key: string]: unknown;
}
export type JobResult =
  | { ok: true; value: ActionResult }
  | { ok: false; error: string; code?: string; operation_result?: unknown };
export interface JobSnapshot {
  id?: string;
  pending: boolean;
  action?: string;
  state?: "running" | "succeeded" | "completed" | "failed" | "interrupted";
  started_at?: number | null;
  finished_at?: number | null;
  result?: JobResult | null;
}
export interface CoreResponses {
  proxies: { proxies: Record<string, Proxy> };
  connections: Connections;
  streams: StreamState;
  resources: Resource[];
  runtime: { config: Record<string, unknown>; available: string[] };
  status: { version: { version: string }; runtime: CoreResponses["runtime"] };
  dns: unknown;
}
