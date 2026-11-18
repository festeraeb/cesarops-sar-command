import { invoke } from "@tauri-apps/api/core";

export type CaseRecord = {
  id: string;
  mission_id: string;
  domain: string;
  objective: string;
  priority: string;
  status: string;
  created_at: string;
  subject_name: string;
  subject_age?: number;
  medical_conditions: string[];
  lkp_lat?: number;
  lkp_lon?: number;
  lkp_description?: string;
  lkp_time?: string;
  contacts: Contact[];
  updates: LogItem[];
  clues: LogItem[];
  team_members: TeamMember[];
};

export type Contact = { name: string; phone: string; relationship: string };
export type LogItem = {
  id: string; case_id: string; time: string; role: string; text: string;
  lat?: number; lon?: number; kind: "update" | "clue";
};
export type TeamMember = {
  id: string; case_id: string; name: string; callsign: string; role: string; status: string;
};


// ---- Module / adapter layer --------------------------------------------
// Every capability is a module that registers itself (id, name, version,
// provides) and is reached through an adapter transport:
//   tauri : local, offline - Rust commands in the desktop app (invoke)
//   http  : cloud - the FCU web host (POST api/call/<cmd>)
//   local : browser-only fallback store (no server found), cases only
// A command has ONE entry point (`call`); the transport decides how it runs.

export type TransportInfo = { kind: "local" | "cloud"; available: boolean; note: string };
export type FunctionInfo = { name: string; available: boolean; note: string };
export type ModuleInfo = {
  id: string; name: string; version: string; kind: "core" | "external"; description: string;
  provides: string[]; transports: TransportInfo[]; functions: FunctionInfo[]; link?: string | null;
};
export type AppConfig = { product: string; mode: "tester" | "demo" | "desktop" | "browser"; banner: string; user?: string | null };

type Transport = {
  name: "tauri" | "http" | "local";
  label: string;
  call<T>(cmd: string, args: Record<string, unknown>): Promise<T>;
  modules(): Promise<ModuleInfo[]>;
  config(): Promise<AppConfig>;
};

const isTauri = () => "__TAURI_INTERNALS__" in window;
const apiUrl = (path: string) => new URL("api/" + path, document.baseURI).toString();

const tauriTransport: Transport = {
  name: "tauri", label: "Desktop - offline, local database",
  call: (cmd, args) => invoke(cmd, args),
  modules: () => invoke("list_modules"),
  config: async () => ({ product: "Field Command Unit", mode: "desktop", banner: "" }),
};

async function httpJson<T>(url: string, init?: RequestInit): Promise<T> {
  const response = await fetch(url, { credentials: "same-origin", ...init });
  const text = await response.text();
  let body: unknown;
  try { body = JSON.parse(text); } catch { throw new Error(`Server returned an unexpected reply (${response.status})`); }
  if (!response.ok) throw new Error((body as { error?: string })?.error ?? `Request failed (${response.status})`);
  return body as T;
}

const httpTransport: Transport = {
  name: "http", label: "Cloud - FCU web host",
  call: (cmd, args) => httpJson(apiUrl("call/" + cmd), {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args ?? {}),
  }),
  modules: () => httpJson(apiUrl("modules")),
  config: () => httpJson(apiUrl("config")),
};

const KEY = "cesarops-sar-command-v1";
const read = (): CaseRecord[] => JSON.parse(localStorage.getItem(KEY) || "[]");
const write = (cases: CaseRecord[]) => localStorage.setItem(KEY, JSON.stringify(cases));
function findCase(cases: CaseRecord[], id: string) {
  const record = cases.find((entry) => entry.id === id);
  if (!record) throw new Error("Case not found");
  return record;
}

const localTransport: Transport = {
  name: "local", label: "Browser - offline local store (no server)",
  modules: async () => [
    { id: "core-cases", name: "Cases, intake and teams", version: "0.1.0", kind: "core", description: "Call intake, mission log, clues, team dispatch.", provides: [], transports: [{ kind: "local", available: true, note: "browser storage" }], functions: [] },
  ],
  config: async () => ({ product: "Field Command Unit", mode: "browser", banner: "" }),
  async call<T>(cmd: string, a: Record<string, unknown>): Promise<T> {
    const cases = read();
    const out = ((): unknown => {
      switch (cmd) {
        case "list_cases": return cases;
        case "get_case": return findCase(cases, a.id as string);
        case "create_case": {
          const input = a.input as Record<string, unknown>;
          const record = { ...input, id: crypto.randomUUID(), created_at: new Date().toISOString(), status: "active", updates: [], clues: [], team_members: [] } as unknown as CaseRecord;
          if (cases.some((item) => item.mission_id === record.mission_id)) throw new Error("Mission ID already exists");
          cases.unshift(record); write(cases); return record;
        }
        case "add_update": case "add_clue": {
          const record = findCase(cases, a.caseId as string);
          const kind = cmd === "add_clue" ? "clue" : "update";
          const item = { ...(a.input as object), id: crypto.randomUUID(), case_id: record.id, kind } as LogItem;
          record[kind === "clue" ? "clues" : "updates"].push(item); write(cases); return item;
        }
        case "add_team_member": {
          const record = findCase(cases, a.caseId as string);
          const item = { ...(a.input as object), id: crypto.randomUUID(), case_id: record.id } as TeamMember;
          record.team_members.push(item); write(cases); return item;
        }
        case "export_case_json": return JSON.stringify(findCase(cases, a.caseId as string), null, 2);
        case "export_report_pdf": return null;
        default: throw new Error(`${cmd} is not available in the offline browser store`);
      }
    })();
    return out as T;
  },
};

let transport: Transport = localTransport;

/** Choose the transport once at start: Tauri, else the web host if one answers, else browser store. */
export async function initBridge(): Promise<{ transport: Transport["name"]; label: string; config: AppConfig; modules: ModuleInfo[] }> {
  if (isTauri()) transport = tauriTransport;
  else {
    try {
      const probe = await fetch(apiUrl("config"), { credentials: "same-origin" });
      const type = probe.headers.get("content-type") ?? "";
      if (type.includes("json")) transport = httpTransport; // 200 or an explicit JSON refusal
      else transport = localTransport;
    } catch { transport = localTransport; }
  }
  const config = await transport.config();
  const modules = await transport.modules();
  return { transport: transport.name, label: transport.label, config, modules };
}

export const resetDemo = () => httpJson<{ ok: boolean }>(apiUrl("reset"), { method: "POST" });
const call = <T>(cmd: string, args: Record<string, unknown> = {}) => transport.call<T>(cmd, args);

export const listCases = () => call<CaseRecord[]>("list_cases");
export const getCase = (id: string) => call<CaseRecord>("get_case", { id });
export const createCase = (input: Omit<CaseRecord, "id" | "created_at" | "status" | "updates" | "clues" | "team_members">) =>
  call<CaseRecord>("create_case", { input });
export const addLog = (caseId: string, kind: "update" | "clue", input: Omit<LogItem, "id" | "case_id" | "kind">) =>
  call<LogItem>(kind === "clue" ? "add_clue" : "add_update", { caseId, input });
export const saveTeamMember = (caseId: string, input: Omit<TeamMember, "id" | "case_id">) =>
  call<TeamMember>("add_team_member", { caseId, input });
export const exportCaseJson = (caseId: string) => call<string>("export_case_json", { caseId });
export const exportReportPdf = (caseId: string, reportType: string) =>
  call<number[] | null>("export_report_pdf", { caseId, reportType });
