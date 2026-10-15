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

/** Describe the approved input, never an assistant's claim that it performed a change. */
export function proposalFacts(run: Snapshot): string[] {
  const input = run.input;
  const facts: string[] = [];
  if(input.live_cad_version){
    facts.push("Cambiar el documento abierto de FreeCAD.", "Objetivo: " + input.description);
    const actions:Record<string,string>={create_object:"Crear objeto",edit_object:"Cambiar objeto",delete_object:"Eliminar objeto",boolean:"Combinar sólidos",fillet:"Redondear bordes",chamfer:"Biselar bordes",loft:"Crear forma entre secciones",sweep:"Crear barrido",mirror:"Reflejar",pattern:"Crear patrón",move:"Mover",color:"Cambiar color",variables:"Cambiar variables",sketch:"Crear boceto",pad:"Extruir",pocket:"Crear vaciado",revolution:"Crear revolución",groove:"Crear ranura",thickness:"Cambiar espesor",draft:"Crear ángulo de salida",datum_plane:"Crear plano de referencia",hull:"Crear envolvente"};
    const names:Record<string,string>={Length:"largo",Width:"ancho",Height:"alto",Radius:"radio",Placement:"posición"};
    for(const operation of input.operations){
      const current=input.before.find(object=>object.name===operation.obj_name);
      facts.push(`${Object.hasOwn(actions,operation.action) ? actions[operation.action] : operation.action}: ${current?.label ?? operation.obj_name ?? "nuevo objeto"}.`);
      for(const [key,value] of Object.entries(operation.obj_properties ?? {})){
        const label=Object.hasOwn(names,key) ? names[key] : key;
        const dimension=["Length","Width","Height","Radius","Radius1","Radius2"].includes(key);
        const display=(v:unknown)=>typeof v==="number" && dimension ? `${number(v)} mm` : typeof v==="string" ? v : JSON.stringify(v);
        facts.push(`${label}: ${current && key in current.properties ? display(current.properties[key]) + " → " : ""}${display(value)}.`);
      }
    }
    facts.push("Las acciones forman una transacción que puedes deshacer en FreeCAD. No se guarda ni exporta el modelo.", "Cancelar detiene la espera; una edición ya aceptada puede completarse después. Releeremos FreeCAD antes del siguiente cambio.");
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
  facts.push(input.live_cad_version ? "El informe de la transacción se guarda en el proyecto." : "El resultado se guarda en el proyecto; los archivos originales se conservan.");
  if (run.audit?.destination) facts.push(`Carpeta de resultado dentro del proyecto: ${run.audit.destination}`);
  return facts;
}
