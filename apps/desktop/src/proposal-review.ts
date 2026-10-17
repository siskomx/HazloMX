import type { Snapshot } from "./contracts";

const unitNames = {mm:"milímetros", cm:"centímetros", m:"metros", in:"pulgadas", ft:"pies", mils:"milésimas de pulgada"};
const drawingActions: Record<string, string> = {
  line:"Dibujar línea", pline:"Dibujar línea de varios segmentos", circle:"Dibujar círculo", arc:"Dibujar arco",
  rectang:"Dibujar rectángulo", polygon:"Dibujar polígono", ellipse:"Dibujar elipse", point:"Añadir punto",
  "point.multiple":"Añadir varios puntos", xline:"Dibujar línea de construcción", ray:"Dibujar semirrecta", spline:"Dibujar curva",
  donut:"Dibujar anillo", erase:"Borrar elementos", move:"Mover elementos", copy:"Copiar elementos", rotate:"Girar elementos",
  scale:"Cambiar escala", mirror:"Reflejar elementos", stretch:"Estirar elementos", offset:"Crear desplazamiento paralelo",
  trim:"Recortar elementos", extend:"Prolongar elementos", fillet:"Redondear encuentro", chamfer:"Biselar encuentro",
  explode:"Separar elementos compuestos", "draworder.front":"Traer al frente", "draworder.back":"Enviar al fondo",
  break:"Cortar elemento", breakatpoint:"Cortar elemento en un punto", join:"Unir elementos", overkill:"Eliminar duplicados", select:"Seleccionar elementos",
};
const number = (value: number) => value.toLocaleString("es-MX", {useGrouping:false, maximumSignificantDigits:15});
const position = (x: number, y: number) => `horizontal ${number(x)}, vertical ${number(y)} mm`;
const circularMeasures: Record<string, Record<string, readonly [string, string?]>> = {
  "Part::Cylinder": {Radius:["distancia del centro al borde de la base", "diámetro del círculo de la base"]},
  "Part::Sphere": {Radius:["distancia del centro al borde de la esfera", "diámetro de la esfera completa"]},
  "Part::Cone": {
    Radius1:["distancia del centro al borde de la base", "diámetro del círculo de la base"],
    Radius2:["distancia del centro al borde del otro extremo", "diámetro del círculo del otro extremo"],
  },
  "Part::Torus": {
    Radius1:["distancia del centro del aro al centro del tubo"],
    Radius2:["distancia del centro al borde del tubo", "diámetro del círculo que forma el tubo"],
  },
};
function doubledLength(value: unknown): string | undefined {
  const parsed = typeof value === "number" ? [value,"mm"] as const
    : typeof value === "string" ? value.trim().match(/^([+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?)\s*(mm|cm|m|in|ft)$/)?.slice(1) : undefined;
  if (!parsed) return;
  const doubled = Number(parsed[0])*2;
  if (Number.isFinite(doubled)) return `${number(doubled)} ${parsed[1]}`;
}

/** Describe the approved input, never an assistant's claim that it performed a change. */
export function proposalFacts(run: Snapshot): string[] {
  const input = run.input;
  const facts: string[] = [];
  if(input.native_tools_version){
    facts.push("Usar las herramientas nativas de " + input.program + ".", "Objetivo: " + input.description,
      "El programa ejecutará las llamadas del catálogo conectado; puedes revisar sus argumentos en los detalles.",
      "Hazlo pedirá una lectura nativa al terminar. Las respuestas no son una certificación independiente de geometría.",
      "Cancelar detiene la espera; no revierte cambios ya aceptados. Revisa el programa antes de repetir una llamada.");
    return facts;
  }
  if(input.live_drawing_version){
    const op=input.operation, unit=unitNames[input.units];
    if(op.action==="export_dxf") {
      facts.push("Exportar una copia del dibujo abierto de CADCraft a DXF.","Dibujo: "+input.before.title,"Objetivo: "+input.description,
        "La copia incluye los cambios actuales sin guardar y conserva las unidades y la escala del dibujo.",
        "Hazlo crea un archivo nuevo en este proyecto y verifica su geometría y unidades al abrirlo con CADCraft.",
        "El dibujo abierto no se guarda, no cambia de nombre y conserva su estado. Exportar no añade un paso de deshacer.");
      return facts;
    }
    facts.push("Cambiar el dibujo abierto en CADCraft.","Dibujo: "+input.before.title,"Objetivo: "+input.description);
    if(op.action==="circle") facts.push(`Crear un círculo de ${number(op.diameter)} ${unit} de diámetro (ancho completo), con centro horizontal ${number(op.center[0])}, vertical ${number(op.center[1])} ${unit}.`);
    else facts.push(`Mover el círculo ${number(op.delta[0])} ${unit} en horizontal y ${number(op.delta[1])} ${unit} en vertical.`);
    facts.push("Verás el cambio en CADCraft y puedes deshacerlo allí. No se guarda ni exporta el dibujo.","Hazlo comprobará la identidad nativa y releerá la geometría. Cancelar detiene la espera y no deshace una edición aceptada.");
  } else if(input.live_agent_version){
    if(input.save) facts.push("Guardar el sketch abierto con Arduino Agent.", "Sketch: " + input.before.name,
      "Objetivo: " + input.description, "Se guardará el código actual, incluida tu nota y los cambios sin guardar.",
      "El IDE usa el archivo actual; no se elige otra ruta ni se reemplaza el código del editor.",
      "Guardar no se deshace como una edición. Las preferencias del IDE pueden actuar al guardar; si cambian el código, Hazlo pedirá revisar el estado actual.",
      "Hazlo comprobará que el IDE confirmó el guardado y que el mismo código sigue abierto sin cambios pendientes.");
    else {
    facts.push("Cambiar el sketch abierto de Arduino Agent.", "Sketch: " + input.before.name, "Objetivo: " + input.description,
      input.before.dirty ? "La lectura incluye cambios sin guardar. Revisa que el código propuesto conserve lo que necesitas." : "La lectura corresponde al código actual del IDE.",
      "Puedes comparar el código actual y el propuesto en los detalles del cambio.",
      "El IDE conserva su historial de deshacer y sus preferencias de guardado. No se carga código a una placa.",
      "Hazlo releerá el código nativo y comprobará que coincide con el cambio aprobado.");
    }
  } else if(input.live_pcb_version){
    if(input.export_copy) facts.push("Exportar una copia editable de la PCB abierta de KiCad.", "PCB: "+input.board_name, "Objetivo: "+input.description, "KiCad aporta el documento actual, incluidos sus cambios sin guardar. Hazlo conserva una copia nueva y verificada en este proyecto.", "No se guarda ni se cambia el documento abierto, ni sus ajustes de proyecto. No se incluyen archivos externos ni se vuelven a trazar pistas.", "Hazlo comprobará la copia completa y que el documento abierto conserve su estado aprobado.");
    else {
    const move=input.movement!;
    const component=input.before.components.find(item=>item.uuid===move.uuid);
    const label=component?.value ? component.value+" ("+move.reference+")" : move.reference;
    const scale=input.units==="in" ? 25.4 : input.units==="mils" ? .0254 : 1;
    const coordinates=(x:number,y:number)=>"horizontal "+number(x/scale)+", vertical "+number(y/scale)+" "+unitNames[input.units];
    const dx=Math.round((move.x_mm-move.expected_x_mm)*1e6)/1e6/scale,dy=Math.round((move.y_mm-move.expected_y_mm)*1e6)/1e6/scale;
    const distance=(v:number)=>number(Math.abs(v))+" "+(Math.abs(v)===1 ? {mm:"milímetro",in:"pulgada",mils:"milésima de pulgada"}[input.units] : unitNames[input.units]);
    const displacement=[dx ? distance(dx)+(dx>0 ? " a la derecha" : " a la izquierda") : "",
      dy ? distance(dy)+(dy>0 ? " hacia abajo" : " hacia arriba") : ""].filter(Boolean).join(" y ");
    facts.push("Mover un componente en la PCB abierta de KiCad.", "PCB: " + input.board_name, "Objetivo: " + input.description,
      "Desplazamiento: " + displacement + ".",
      label + ": " + coordinates(move.expected_x_mm,move.expected_y_mm) + " → " + coordinates(move.x_mm,move.y_mm) + ".",
      "KiCad mostrará el cambio en el documento abierto. El archivo todavía no se guardará.",
      "Las pistas conservan su posición; este movimiento no vuelve a conectarlas.",
      "Hazlo comprobará el movimiento y releerá el estado nativo antes de darlo por terminado.");
    }
  } else if(input.live_cad_version){
    const saving = input.operations[0]?.action === "save_as";
    const exporting = input.operations[0]?.action === "export_step";
    facts.push(saving ? "Guardar el documento abierto de FreeCAD." : exporting ? "Exportar la pieza abierta a STEP." : "Cambiar el documento abierto de FreeCAD.", "Objetivo: " + input.description);
    const actions:Record<string,string>={create_object:"Crear objeto",edit_object:"Cambiar objeto",delete_object:"Eliminar objeto",boolean:"Combinar sólidos",fillet:"Redondear bordes",chamfer:"Biselar bordes",loft:"Crear forma entre secciones",sweep:"Crear barrido",mirror:"Reflejar",pattern:"Crear patrón",move:"Mover",color:"Cambiar color",variables:"Cambiar variables",sketch:"Crear boceto",pad:"Extruir",pocket:"Crear vaciado",revolution:"Crear revolución",groove:"Crear ranura",thickness:"Cambiar espesor",draft:"Crear ángulo de salida",datum_plane:"Crear plano de referencia",hull:"Crear envolvente"};
    let explainedDiameter=false;
    const names:Record<string,string>={Length:"largo",Width:"ancho",Height:"alto",Radius:"distancia del centro al borde",Radius1:"primer radio (del centro al borde)",Radius2:"segundo radio (del centro al borde)",Placement:"posición"};
    for(const operation of saving || exporting ? [] : input.operations){
      const current=input.before.find(object=>object.name===operation.obj_name);
      facts.push(`${Object.hasOwn(actions,operation.action) ? actions[operation.action] : operation.action}: ${current?.label ?? operation.obj_name ?? "nuevo objeto"}.`);
      for(const [key,value] of Object.entries(operation.obj_properties ?? {})){
        const type=current?.type ?? operation.obj_type ?? "";
        const measure=Object.hasOwn(circularMeasures,type) && Object.hasOwn(circularMeasures[type],key) ? circularMeasures[type][key] : undefined;
        const previous=current && Object.hasOwn(current.properties,key) ? current.properties[key] : undefined;
        const proposedDiameter=measure?.[1] ? doubledLength(value) : undefined;
        const previousDiameter=measure?.[1] ? doubledLength(previous) : undefined;
        const diameter=proposedDiameter!==undefined && (previous===undefined || previousDiameter!==undefined);
        const label=diameter ? measure![1]! : measure?.[0] ?? (Object.hasOwn(names,key) ? names[key] : key);
        const dimension=["Length","Width","Height","Radius","Radius1","Radius2"].includes(key);
        const display=(v:unknown)=>typeof v==="number" && dimension ? `${number(v)} mm` : typeof v==="string" ? v : JSON.stringify(v);
        if(diameter && !explainedDiameter){facts.push("Diámetro significa ancho completo: de un borde al opuesto pasando por el centro del círculo.");explainedDiameter=true;}
        facts.push(`${label}: ${previous!==undefined ? (diameter ? previousDiameter : display(previous)) + " → " : ""}${diameter ? proposedDiameter : display(value)}.`);
      }
    }
    if (saving) facts.push("FreeCAD abrirá Guardar como para que elijas el archivo y confirmes allí cualquier sobrescritura.", "Guardar no es una edición de geometría que pueda deshacerse. Hazlo verificará el archivo guardado y releerá el documento.");
    else if (exporting) {
      const object = input.before.find(object => object.name === input.operations[0].obj_name);
      facts.push("Pieza: " + (object?.label ?? input.operations[0].obj_name) + ".", "FreeCAD pedirá un archivo nuevo. No se sobrescriben destinos existentes.", "Se verifican los sólidos, volumen, tamaño y posición del STEP. El documento abierto no cambia ni se guarda al exportar.");
    }
    else facts.push("Las acciones forman una transacción que puedes deshacer en FreeCAD. No se guarda ni exporta el modelo.");
    facts.push("Cancelar detiene la espera; una operación ya aceptada puede completarse después. Releeremos FreeCAD antes del siguiente cambio.");
  } else if (input.konnect_version) {
    facts.push(`Documento: ${input.source.entry}.`);
    if (input.pcb_move) {
      const move = input.pcb_move;
      facts.push(`Mover el componente ${move.reference}.`, `Posición actual: ${position(move.expected_x_mm,move.expected_y_mm)}.`, `Nueva posición: ${position(move.x_mm,move.y_mm)}.`, "Las posiciones son absolutas en la placa; se muestran en milímetros.");
    } else facts.push(`Cambiar el valor del componente ${input.edit.reference}: ${input.edit.expected_value} → ${input.edit.value}.`);
  } else if (input.cadpilot_version) {
    const {request} = input, p = request.parameters;
    facts.push(`Documento: ${input.source_name}.`);
    const operations = {inspect:"Inspeccionar y medir",edit_box:"Cambiar las medidas de un bloque",add_box:"Añadir un bloque",add_cylinder:"Añadir un cilindro"};
    facts.push(`${operations[request.operation]} · objeto ${request.object}.`);
    if (request.operation !== "inspect") {
      const current = input.preview?.measurements[request.object]?.dimensions_mm;
      if (request.operation === "edit_box" && current) facts.push(`Medidas actuales en milímetros: largo ${number(current.length)}, ancho ${number(current.width)}, alto ${number(current.height)}.`);
      facts.push(`Medidas en ${unitNames[request.units]}.`);
      if (request.operation === "add_cylinder") facts.push(`Diámetro completo: ${number(p.radius*2)}. Alto: ${number(p.height)}.`);
      else facts.push(`Largo: ${number(p.length)}. Ancho: ${number(p.width)}. Alto: ${number(p.height)}.`);
      if (request.operation.startsWith("add_")) facts.push(`Posición de origen: horizontal ${number(p.x)}, vertical ${number(p.y)}, elevación ${number(p.z)}.`);
    }
  } else if (input.orca_version) {
    const {settings} = input;
    const [min,max] = input.mesh.bounds_mm;
    facts.push(`Modelo: ${input.source_name}.`, `Interpretar las coordenadas del archivo en ${unitNames[input.units]}.`, `Tamaño interpretado en milímetros: largo ${number(max[0]-min[0])}, ancho ${number(max[1]-min[1])}, alto ${number(max[2]-min[2])}.`, `Preparar archivos de impresión para ${settings.machine} con ${settings.filament}.`, `Perfil: ${settings.process}.`, `Cama seleccionada: ${settings.bed}.`, `Boquilla: ${number(settings.nozzle_mm)} mm. Altura de capa: ${number(settings.layer_mm)} mm.`, `Temperaturas: boquilla ${number(settings.nozzle_c)} °C; cama ${number(settings.bed_c)} °C.`, "Solo se generan archivos; no se envían a una impresora.");
  } else if (input.kicad_version) {
    facts.push(`Documento: ${input.entry}.`, "Revisar las reglas del diseño y generar informes y exportaciones.", `Unidades del informe: ${unitNames[input.units]}. No cambian la escala del diseño.`);
  } else if (input.cadcraft_version) {
    facts.push(`Dibujo: ${input.source_name}.`, `Unidades del dibujo: ${unitNames[input.units]}.`, `Guardar el resultado como ${input.output_format.toUpperCase()}.`);
    if (input.native_tools) {
      facts.push(`Descripción de la IA: ${input.native_tools.intent}.`);
      for (const [index, call] of input.native_tools.calls.entries()) facts.push(`${index+1}. ${Object.hasOwn(drawingActions,call.command) ? drawingActions[call.command] : call.command}.`);
      facts.push("Los elementos y parámetros exactos de cada acción están en los detalles del cambio.");
    } else if (input.circle) facts.push(`Añadir un círculo de diámetro ${number(input.circle.radius*2)}, con centro horizontal ${number(input.circle.center[0])}, vertical ${number(input.circle.center[1])}.`);
    if (input.allow_loss) facts.push("La conversión permite perder elementos no compatibles. Revisa esta pérdida antes de aprobar.");
  } else if (input.agent_version) {
    facts.push(`Sketch: ${input.source_name}.`, "Guardar una copia con el código propuesto. Puedes comparar el original y el cambio en los detalles.", "No se carga código a una placa ni se modifica el archivo original.");
  } else if (input.dimensions_mm) {
    const d = input.dimensions_mm;
    facts.push("Crear un bloque rectangular en FreeCAD.", `Medidas del resultado en milímetros: largo ${number(d.length)}, ancho ${number(d.width)}, alto ${number(d.height)}.`);
  } else {
    facts.push(`Comprobar que el código compila para ${input.fqbn === "arduino:avr:uno" ? "Arduino Uno" : input.fqbn}.`, "Se generarán el sketch, el informe y los archivos compilados. Revisa el código en los detalles.", "No se carga código a una placa.");
  }
  facts.push(input.live_agent_version || input.live_drawing_version || input.live_cad_version || input.live_pcb_version ? "El informe de la operación nativa se guarda en el proyecto." : "El resultado se guarda en el proyecto; los archivos originales se conservan.");
  if (run.audit?.destination) facts.push(`Carpeta de resultado dentro del proyecto: ${run.audit.destination}`);
  return facts;
}
