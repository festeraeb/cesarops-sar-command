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

const KEY = "cesarops-sar-command-v1";
const isTauri = () => "__TAURI_INTERNALS__" in window;
const read = (): CaseRecord[] => JSON.parse(localStorage.getItem(KEY) || "[]");
const write = (cases: CaseRecord[]) => localStorage.setItem(KEY, JSON.stringify(cases));

export async function listCases(): Promise<CaseRecord[]> {
  return isTauri() ? invoke("list_cases") : read();
}

export async function getCase(id: string): Promise<CaseRecord | undefined> {
  if (isTauri()) return invoke("get_case", { id });
  return read().find((item) => item.id === id);
}

export async function createCase(input: Omit<CaseRecord, "id" | "created_at" | "status" | "updates" | "clues" | "team_members">): Promise<CaseRecord> {
  if (isTauri()) return invoke("create_case", { input });
  const record: CaseRecord = {
    ...input,
    id: crypto.randomUUID(),
    created_at: new Date().toISOString(),
    status: "active",
    updates: [],
    clues: [],
    team_members: [],
  };
  const cases = read();
  if (cases.some((item) => item.mission_id === record.mission_id)) throw new Error("Mission ID already exists");
  cases.unshift(record);
  write(cases);
  return record;
}

export async function addLog(caseId: string, kind: "update" | "clue", item: Omit<LogItem, "id" | "case_id" | "kind">): Promise<LogItem> {
  if (isTauri()) return invoke(kind === "clue" ? "add_clue" : "add_update", { caseId, input: item });
  const cases = read();
  const record = cases.find((entry) => entry.id === caseId);
  if (!record) throw new Error("Case not found");
  const result: LogItem = { ...item, id: crypto.randomUUID(), case_id: caseId, kind };
  record[kind === "clue" ? "clues" : "updates"].push(result);
  write(cases);
  return result;
}

export async function saveTeamMember(caseId: string, member: Omit<TeamMember, "id" | "case_id">): Promise<TeamMember> {
  if (isTauri()) return invoke("add_team_member", { caseId, input: member });
  const cases = read();
  const record = cases.find((entry) => entry.id === caseId);
  if (!record) throw new Error("Case not found");
  const result = { ...member, id: crypto.randomUUID(), case_id: caseId };
  record.team_members.push(result);
  write(cases);
  return result;
}

export async function exportCaseJson(caseId: string): Promise<string> {
  if (isTauri()) return invoke("export_case_json", { caseId });
  const record = await getCase(caseId);
  if (!record) throw new Error("Case not found");
  return JSON.stringify(record, null, 2);
}

export async function exportReportPdf(caseId: string, reportType: string): Promise<number[] | null> {
  if (!isTauri()) return null;
  return invoke("export_report_pdf", { caseId, reportType });
}
