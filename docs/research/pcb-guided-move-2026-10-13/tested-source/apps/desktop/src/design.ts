import type { Dimensions, DiscoveredSchematic, DiscoveredPcb } from "./contracts";

export const units = {
  mm: { label: "Milímetros (mm)", factor: 1 },
  cm: { label: "Centímetros (cm)", factor: 10 },
  m: { label: "Metros (m)", factor: 1000 },
  in: { label: "Pulgadas (in)", factor: 25.4 },
  ft: { label: "Pies (ft)", factor: 304.8 },
} as const;
export type Unit = keyof typeof units;
export type Turn = { role: "user" | "assistant"; content: string };
export const pcbFactors:Record<string,number> = {mm:units.mm.factor,in:units.in.factor,mils:units.in.factor/1000};
type PcbDirection = "right" | "left" | "up" | "down";
// shortcut: one planar move from the inspected position; connected agents handle broader placement intent.
export function guidedPcb(content:string, component:Pick<DiscoveredPcb["components"][number],"x_mm"|"y_mm">, pending?:PcbDirection): {reply:string;awaiting?:PcbDirection;x_mm?:number;y_mm?:number} {
  const text=content.normalize("NFD").replace(/[\u0300-\u036f]/g,"").toLowerCase().trim();
  if(/^(?:cancel(?:a|ar)?|olvida(?:lo|la)?|no\s+(?:(?:lo|la)\s+)?(?:muevas|quiero|hagas)|(?:i\s+)?(?:do not|don't|don’t)\s+(?:move|want))\b/.test(text))return {reply:"Movimiento cancelado. No cambié el borrador."};
  const move=text.match(/^(?:mueve(?:lo|la)?|move(?: it)?)\s+(.*?)\s*(?:(?:a la|hacia la|hacia|to the|to)\s+)?(derecha|izquierda|arriba|abajo|right|left|up|down)$/);
  const directions:Record<string,PcbDirection>={derecha:"right",izquierda:"left",arriba:"up",abajo:"down",right:"right",left:"left",up:"up",down:"down"};
  const direction=move ? directions[move[2]] : pending;
  if(!direction)return {reply:"Con el componente seleccionado, dime cuánto moverlo desde su posición medida, por ejemplo «muévelo 2 mm a la derecha». Podemos confirmar la distancia primero. Para otra colocación o varios movimientos, usa Claude Code o Codex."};
  const labels={right:"a la derecha",left:"a la izquierda",up:"hacia arriba",down:"hacia abajo"};
  const question=`¿Qué distancia quieres moverlo ${labels[direction]} desde su posición medida? Confirma un valor positivo y su unidad, por ejemplo «2 mm» o «0,1 pulgadas». No elegiré la distancia por ti.`;
  const measure=(move?.[1] ?? text).trim().match(/^\+?(\d+(?:[.,]\d+)?)\s*(mm|milimetros?|millimeters?|cm|centimetros?|centimeters?|m|metros?|meters?|in|inch(?:es)?|pulgadas?|ft|feet|foot|pies?|pie|mils?)$/);
  if(!measure)return {reply:question,awaiting:direction};
  const spelling=measure[2];
  const factor=/^mils?$/.test(spelling) ? pcbFactors.mils : /^(mm|mili|milli)/.test(spelling) ? units.mm.factor : /^(cm|centi)/.test(spelling) ? units.cm.factor : /^m/.test(spelling) ? units.m.factor : /^(in|pulg)/.test(spelling) ? units.in.factor : units.ft.factor;
  const distance=Number(measure[1].replace(",","."))*factor;
  const x=component.x_mm+(direction==="right" ? distance : direction==="left" ? -distance : 0);
  const y=component.y_mm+(direction==="down" ? distance : direction==="up" ? -distance : 0);
  if(!Number.isFinite(distance) || distance<=0 || [x,y].some(value=>!Number.isFinite(value) || Math.abs(value)>10000 || Math.abs(value*1e6-Math.round(value*1e6))>0.00001) ||
    Math.round(x*1e6)===Math.round(component.x_mm*1e6) && Math.round(y*1e6)===Math.round(component.y_mm*1e6))return {reply:"Confirma una distancia positiva con precisión de un nanómetro y una posición final dentro de ±10000 mm. El borrador no cambió.",awaiting:direction};
  const x_mm=Number(x.toFixed(6)),y_mm=Number(y.toFixed(6));
  return {x_mm,y_mm,reply:`Anoté ${measure[1]} ${spelling} ${labels[direction]} desde la posición medida (${component.x_mm}, ${component.y_mm}) mm: nueva posición (${x_mm}, ${y_mm}) mm. Se usa la vista del plano, con derecha horizontal positiva y abajo vertical positiva. Prepara y revisa el movimiento antes de aprobar; puede dejar pistas desconectadas. Todavía no se modificó ningún archivo.`};
}
// shortcut: guided box edits accept one absolute size; connected agents handle broader design intent.
export function guidedBox(content:string, pending?:keyof Dimensions): {reply:string;dimension?:keyof Dimensions;value_mm?:number;awaiting?:boolean} {
  const text=content.normalize("NFD").replace(/[\u0300-\u036f]/g,"").toLowerCase().trim();
  if(/^(?:cancel(?:a|ar)?|olvida(?:lo|la)|no (?:cambies|quiero|hagas|deseo|necesito)|(?:i\s+)?(?:do not|don['’]t) (?:change|want|make))\b/.test(text))return {reply:"El borrador no cambió. Podemos elegir otra medida cuando quieras."};
  const names:Record<string,keyof Dimensions>={alto:"height",altura:"height",alta:"height",height:"height",tall:"height",taller:"height",ancho:"width",ancha:"width",width:"width",wide:"width",wider:"width",largo:"length",larga:"length",longitud:"length",length:"length",long:"length",longer:"length"};
  const labels={length:"largo",width:"ancho",height:"alto"};
  const direct=text.match(/^(?:cambia|ajusta|pon|change|set)\s+(?:(?:el|la|the)\s+)?(alto|altura|height|ancho|width|largo|longitud|length)\s+(?:a|en|to|at)\s+(.+)$/);
  const relative=text.match(/^(?:hazla|hazlo)\s+mas\s+(alta|alto|ancha|ancho|larga|largo)$/) ?? text.match(/^make it\s+(taller|wider|longer)$/);
  const sized=text.match(/^(?:hazla|hazlo|make it)\s+(?:de\s+)?(.+?)\s+(?:de\s+)?(alto|alta|ancho|ancha|largo|larga|tall|wide|long)$/);
  const dimension=direct ? names[direct[1]] : relative ? names[relative[1]] : sized ? names[sized[2]] : pending;
  if(!dimension)return {reply:"Elige una caja medida y dime qué quieres cambiar, por ejemplo «hazla más alta» o «cambia el ancho a 2 pulgadas». Para otra forma u operación, usa Claude Code o Codex o confirma los campos de la propuesta."};
  const question=`¿Qué ${labels[dimension]} final quieres para esta caja? Confirma una medida y su unidad, por ejemplo «2 cm» o «0,5 pulgadas». Se conservan las demás medidas del borrador; no elegiré el tamaño por ti.`;
  const literal=direct?.[2] ?? sized?.[1] ?? text;
  const measurement=relative ? null : literal.match(/^\+?(\d+(?:[.,]\d+)?)\s*(mm|milimetros?|millimeters?|cm|centimetros?|centimeters?|m|metros?|meters?|in|inch(?:es)?|pulgadas?|ft|feet|foot|pies?|pie)$/);
  if(!measurement)return {reply:question,dimension,awaiting:true};
  const spelling=measurement[2];
  const unit:Unit=/^(mm|mili|milli)/.test(spelling) ? "mm" : /^(cm|centi)/.test(spelling) ? "cm" : /^m/.test(spelling) ? "m" : /^(in|inch|pulg)/.test(spelling) ? "in" : "ft";
  const value_mm=Number(measurement[1].replace(",","."))*units[unit].factor;
  if(!Number.isFinite(value_mm) || value_mm<0.1 || value_mm>1000)return {reply:`Confirma un ${labels[dimension]} entre 0,1 y 1000 mm, o su equivalente en otra unidad. El borrador no cambió.`,dimension,awaiting:true};
  return {dimension,value_mm,reply:`Anoté el ${labels[dimension]} final: ${measurement[1]} ${spelling}. Se conservaron las otras medidas del borrador. Prepara la propuesta para revisar el cambio y aprobarlo; todavía no se modificó ningún archivo.`};
}
export function schematicKind(library: string): string {
  const kinds: Record<string,string> = {"Device:R":"Resistencia","Device:R_US":"Resistencia","Device:R_Small":"Resistencia","Device:C":"Condensador","Device:C_Small":"Condensador","Device:L":"Bobina","Device:D":"Diodo","Device:LED":"LED"};
  return Object.hasOwn(kinds,library) ? kinds[library] : (library.split(":").at(-1) || "Componente");
}
// shortcut: guided edits match inspected labels and explicit values; use connected agents for open-ended circuit reasoning.
export function guidedSchematic(content: string, components: DiscoveredSchematic["components"], selected: string): {reply:string;reference?:string;value?:string} {
  const normalize = (text:string) => text.normalize("NFD").replace(/[\u0300-\u036f]/g, "").toLowerCase().trim().replace(/^(?:la|el|the)\s+/, "").replace(/\s+(?:de|of)\s+/g, " ").replace(/\bresistor\b/g,"resistencia").replace(/\bcapacitor\b/g,"condensador").replace(/\s+/g, " ");
  const match=normalize(content).match(/^(?:cambia|modifica|change|modify)\s+(.+?)\s+(?:a|to)\s+(.+)$/);
  const followup=content.trim().match(/^(?:c[aá]mbia(?:la|lo)|modif[ií]ca(?:la|lo)|(?:change|modify)\s+it)\s+(?:a|to)\s+(.{1,1024})$/i);
  if(!match && !followup)return {reply:"El esquema ya tiene una lista de componentes y sus valores. Puedes decir «cambia la resistencia de 10k a 22k» o elegir un componente y decir «cámbiala a 22k». Si no conoces el valor necesario, describe su función con Claude Code o Codex antes de confirmar el cambio."};
  // Values remain literal user text; normalized labels are only for selecting inspected components.
  const value=followup?.[1].trim() ?? content.trim().match(/^(?:cambia|modifica|change|modify)\s+.+?\s+(?:a|to)\s+(.{1,1024})$/i)?.[1].trim();
  if(!value)return {reply:"Confirma un único valor nuevo de hasta 1024 caracteres. El borrador no cambió."};
  const candidates=components.filter(component=>followup ? component.reference===selected : [component.reference,component.value,schematicKind(component.library),`${schematicKind(component.library)} ${component.value}`].some(label=>normalize(label)===normalize(match![1])));
  if(candidates.length!==1)return {reply:candidates.length ? `Hay ${candidates.length} componentes que coinciden. Elige uno en la lista y confirma el valor nuevo; el borrador no cambió.` : "No pude identificar un componente único de la hoja seleccionada. Elige uno en la lista y confirma el valor nuevo; el borrador no cambió."};
  const component=candidates[0];
  if(component.reference.startsWith("#"))return {reply:"Ese símbolo es de solo lectura en esta operación. El borrador no cambió."};
  return {reference:component.reference,value,reply:`Anoté ${schematicKind(component.library)} · ${component.value || "sin valor"} → ${value}. Revisa el componente seleccionado y prepara la propuesta para confirmar el valor actual antes de aprobar.`};
}
export function guidedBlink(turns: Turn[]): { reply: string; source?: string } {
  const prefix = "Para el ejemplo de parpadeo en Arduino Uno:";
  let index = turns.length - 1;
  const parts = [turns[index]?.content ?? ""];
  while (index >= 2 && turns[index - 1].role === "assistant" && turns[index - 1].content.startsWith(prefix)) {
    parts.unshift(turns[index - 2].content);
    index -= 2;
  }
  const request = parts.join(" ");
  const guidance = "Describe qué debe hacer Arduino Uno. Podemos reunir la función y los tiempos antes de revisar el código. Para otra placa, un LED externo u otra función, usa Claude Code o Codex o pega tu código para revisarlo.";
  const intent = request.replace(/\bno\s+s[ée](?=\s|[,.!?]|$)|\bi\s+don['’]t\s+know\b/gi, "");
  if (!/parpade|blink/i.test(intent) || /\b(no|not|never|don['’]t|externo|external)\b/i.test(intent)) return { reply: guidance };
  // Keep decimal commas within their value; uncertainty applies to the latest relevant clause.
  const clauses = parts.flatMap(part => part.split(/[;!?]|,(?!\d)|\b(?:pero|but)\b/i));
  const uncertain = /\bno\s+s[ée](?=\s|[,.!?]|$)|\bi\s+don['’]t\s+know\b|\b(?:quiz[aá]s|tal\s+vez|maybe|perhaps|might)\b/i;
  const board = clauses.filter(clause => /\b(?:placa|board|arduino\s+uno)\b/i.test(clause)).at(-1) ?? "";
  if (!/\barduino\s+uno\b/i.test(board) || uncertain.test(board)) return { reply: `${prefix} ¿qué placa tienes? Este ejemplo está verificado para Arduino Uno con AVR 1.8.8. Confirma «Arduino Uno» si es tu placa; si no sabes, revisa el nombre impreso antes de preparar código.` };
  const led = clauses.filter(clause => /\bled\b/i.test(clause)).at(-1) ?? "";
  if (!/led\s+(?:integrado|built[ -]?in)|built[ -]?in\s+led/i.test(led) || uncertain.test(led)) return { reply: `${prefix} ¿quieres usar el LED integrado de la placa? Confirma «LED integrado». Para un LED externo necesitamos conocer su conexión antes de escribir código.` };
  const timeUnit = /(?<![\p{L}_])(?:milisegundos?|milliseconds?|ms|segundos?|seconds?|s)(?![\p{L}\p{N}_])/iu;
  const timePattern = /(?<![\p{L}\p{N}_.,+\-−–—/×*:])([+\-−–—]?\s*\d[\d.,/+\-−–—×*:\s]*?)\s*(milisegundos?|milliseconds?|ms|segundos?|seconds?|s)(?![\p{L}\p{N}_])/giu;
  // Unsupported newer notation must ask for clarification instead of reusing an older interval.
  const timing = [...parts].reverse().find(part => timeUnit.test(part)) ?? "";
  const times = [...timing.matchAll(timePattern)];
  const timeClause = clauses.filter(clause => timeUnit.test(clause)).at(-1) ?? "";
  if (times.length !== 1 || uncertain.test(timeClause)) return { reply: `${prefix} ¿cuánto tiempo debe permanecer encendido y cuánto apagado? Este ejemplo usa el mismo intervalo para ambos. Confirma un único intervalo, por ejemplo «1 segundo» o «250 ms»; no elegiré un tiempo por ti.` };
  const literal = times[0][1].trim();
  const [whole, fraction = ""] = literal.replace(",", ".").split(".");
  const milliseconds = !/^\+?\d+(?:[.,]\d+)?$/.test(literal) ? NaN : /^(ms|milli|mili)/i.test(times[0][2])
    ? (/[^0]/.test(fraction) ? NaN : Number(whole))
    : (fraction.length > 3 ? NaN : Number(whole) * 1000 + Number(fraction.padEnd(3, "0")));
  if (!Number.isSafeInteger(milliseconds) || milliseconds < 1 || milliseconds > 0xffffffff) return { reply: `${prefix} confirma un intervalo positivo que pueda expresarse en milisegundos enteros, por ejemplo «0,5 segundos» o «500 ms». El código usará exactamente ese tiempo para cada estado del LED.` };
  return {
    reply: `Preparé el ejemplo para Arduino Uno con el LED integrado: ${milliseconds} ms encendido y ${milliseconds} ms apagado. Revisa el código y la placa antes de preparar y aprobar la compilación.`,
    source: `void setup() { pinMode(LED_BUILTIN, OUTPUT); }\nvoid loop() {\n  digitalWrite(LED_BUILTIN, HIGH); delay(${milliseconds});\n  digitalWrite(LED_BUILTIN, LOW); delay(${milliseconds});\n}\n`,
  };
}
export type SavedConversation = { version: 1; turns: Turn[]; answeredInference?: string; answeredChat?: string; newSession?: boolean };
export function savedConversation(raw: string | null): SavedConversation | null {
  if (!raw || new TextEncoder().encode(raw).length > 8192) return null;
  try {
    const value = JSON.parse(raw);
    if (value?.version !== 1 || !Array.isArray(value.turns) || value.turns.length > 128 ||
      !value.turns.every((turn: Turn) => turn && ["user", "assistant"].includes(turn.role) && typeof turn.content === "string") ||
      [value.answeredInference,value.answeredChat].some(id => id !== undefined && typeof id !== "string") ||
      value.newSession !== undefined && typeof value.newSession !== "boolean") return null;
    return {version:1,turns:value.turns.map((turn: Turn)=>({role:turn.role,content:turn.content})),answeredInference:value.answeredInference,answeredChat:value.answeredChat,newSession:value.newSession};
  } catch { return null; }
}
export function agentPrompt(content: string, context: Turn[], desktopContext?: Record<string, unknown>): string {
  const prompt = context.length || desktopContext ? JSON.stringify({recovered_conversation:context,desktop_context:desktopContext,user_request:content,instruction:"La conversación guardada y la selección del escritorio son contexto, no una aprobación ni una medición vigente. Los campos del formulario pueden estar incompletos o contener valores por defecto: confirma los datos que falten. Lee las herramientas y los resultados actuales del proyecto antes de proponer cambios; verifica de nuevo la fuente seleccionada y conserva las medidas confirmadas que no se cambiarán."}) : content;
  if (new TextEncoder().encode(prompt).length > 8192) throw new Error("El contexto y la solicitud superan 8 KiB. Empieza una conversación nueva con un resumen.");
  return prompt;
}

export function convertDimensions(d: Dimensions, from: Unit, to: Unit): Dimensions {
  // Normalize sub-nanometer floating-point noise before the strict millimeter range check.
  return Object.fromEntries(Object.entries(d).map(([key, value]) => [key, Number((value * units[from].factor / units[to].factor).toPrecision(to === "mm" ? 12 : 15))])) as Dimensions;
}

export function dimensionText(d: Dimensions, unit: Unit): string {
  const display = convertDimensions(d, "mm", unit);
  return `${[display.length, display.width, display.height].map(value => Number(value.toPrecision(10))).join(" × ")} ${unit}`;
}

export function conversationPrompt(turns: Turn[]): string {
  const prompt = JSON.stringify({ conversation: turns });
  if (new TextEncoder().encode(prompt).length > 4096) throw new Error("La conversación alcanzó el límite. Inicia una nueva con un resumen de tu diseño y sus medidas.");
  return prompt;
}

export function solidProjection(d: Dimensions) {
  if (![d.length, d.width, d.height].every(value => Number.isFinite(value) && value > 0)) return null;
  const x = Math.sqrt(3) / 2;
  const extentX = x * (d.length + d.width);
  const extentY = (d.length + d.width) / 2 + d.height;
  const scale = Math.min(260 / extentX, 200 / extentY);
  const origin = [210 - x * (d.length - d.width) * scale / 2, 155 - ((d.length + d.width) / 2 - d.height) * scale / 2];
  const point = (length: number, width: number, height: number) => [origin[0] + x * (length - width) * scale, origin[1] + ((length + width) / 2 - height) * scale];
  const polygon = (points: number[][]) => `M${points.map(p => p.join(",")).join(" L")} Z`;
  const top = point(0, 0, d.height), left = point(0, d.width, d.height), right = point(d.length, 0, d.height), front = point(d.length, d.width, d.height);
  const bottomLeft = point(0, d.width, 0), bottomRight = point(d.length, 0, 0), bottomFront = point(d.length, d.width, 0);
  return {
    top: polygon([top, right, front, left]),
    left: polygon([left, front, bottomFront, bottomLeft]),
    right: polygon([front, right, bottomRight, bottomFront]),
    lengthLabel: [(bottomLeft[0] + bottomFront[0]) / 2, (bottomLeft[1] + bottomFront[1]) / 2 + 18],
    widthLabel: [(bottomRight[0] + bottomFront[0]) / 2, (bottomRight[1] + bottomFront[1]) / 2 + 18],
    heightLabel: [bottomRight[0] + 8, (right[1] + bottomRight[1]) / 2],
  };
}
