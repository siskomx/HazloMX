import { savedConversation, type Turn } from "./design";

const legacyScopes: Record<string, string> = {
  codex: "Codex", claude: "Claude Code", guided: "Guía anterior · FreeCAD",
  "guided:arduino": "Guía anterior · Arduino CLI", "guided:agent": "Guía anterior · Arduino Agent",
  "guided:drawing": "Guía anterior · CADCraft", "guided:konnect": "Guía anterior · esquema KiCad",
  "guided:pcb": "Guía anterior · PCB KiCad", "guided:kicad": "Guía anterior · revisión KiCad",
  "guided:cadpilot": "Guía anterior · CADPilot", "guided:slice": "Guía anterior · OrcaSlicer",
};
export type LegacyConversation = { key: string; label: string; turns: Turn[] };
export function legacyConversations(read: (key: string) => string | null, project: [string, string] | null): LegacyConversation[] {
  if (!project) return [];
  return Object.entries(legacyScopes).flatMap(([scope, label]) => {
    const key = `design-chat:${JSON.stringify([...project, scope])}`;
    const saved = savedConversation(read(key));
    return saved?.turns.length ? [{key, label, turns: saved.turns}] : [];
  });
}
