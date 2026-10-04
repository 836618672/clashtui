export type Json = any;
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
  traffic: Json;
  memory: Json;
  connections: Json;
  logs: LogRow[];
  cursor: number;
  dropped: boolean;
  status: Record<
    string,
    { connected: boolean; updated_at: number; error?: string }
  >;
}
export interface Resource {
  id: string;
  value: Json;
}
