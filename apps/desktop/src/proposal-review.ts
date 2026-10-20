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
  if(input.public_document_version){
    return ["Permitir el envío de esta dirección por internet; esta autorización es distinta de editar un programa y solo vale para esta tarea y sus entradas actuales.","Dirección completa que recibirá el sitio: "+input.url,"Para qué: "+input.purpose,"El sitio recibirá también la ruta y todos los valores de la consulta. Pueden revelar información de la tarea: esta dirección es el contenido concreto que autorizas enviar.","No se adjuntan archivos ni se envían cookies, credenciales de sesión o el resto del historial. Eso no garantiza que la dirección esté libre de datos privados.","Una lectura de texto UTF-8 de hasta 512 KiB y 20 segundos. Hazlo no sigue redirecciones, ejecuta scripts ni reintenta automáticamente.","La respuesta queda local con la tarea. Compartirla con una IA requiere un permiso posterior; esta consulta no comparte otras referencias."];
  }
  if(run.audit?.task) facts.push("Esta aprobación solo permite el cambio mostrado en esta conversación. Cambiar la tarea, sus mensajes o referencias invalida la propuesta; no permite instalar herramientas, compartir nuevas referencias ni controlar hardware.");
  if(input.native_tools_version || input.live_cad_version || input.live_agent_version || input.live_pcb_version || input.live_drawing_version){
    const agreement=input.agreement;
    if(agreement){
      facts.push(agreement.kind==="provisional_draft"?"Preparar un borrador provisional con las condiciones indicadas.":"Realizar este lote de software con las condiciones indicadas.","Resultado propuesto: "+agreement.outcome);
      for(const value of agreement.assumptions)facts.push("Supuesto propuesto, no medición: "+value);
      for(const value of agreement.constraints)facts.push("Límite que debe conservarse: "+value);
      for(const value of agreement.losses)facts.push("Pérdida que aceptarías con este lote: "+value);
      facts.push("Cómo se propone comprobarlo: "+agreement.verification,"Aprobar acepta estas condiciones para este lote. No convierte supuestos ni planes de comprobación en hechos verificados.");
    }else facts.push("Propuesta anterior sin resultado y supuestos estructurados; pide a Hazlo que la prepare de nuevo antes de continuar.");
  }
  if(input.native_tools_version){
    if(!input.guard?.checks?.some(check=>check.role==="outcome"))facts.push("No hay comprobaciones automáticas del resultado acordado. Una respuesta del programa no demostrará que la tarea esté terminada.");
    for(const check of input.guard?.checks??[])facts.push(check.role==="identity"?`Mantener la identidad de ${check.label}.`:`Comprobar ${check.label}: ${JSON.stringify(check.expected)}.`);
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
      if(input.native_export_check_version===1)facts.push("La copia debe conservar los mismos datos DXF al reabrirla y serializarla con CADCraft. Si necesita referencias externas, se conserva pendiente de revisión.","La vista, los recursos de representación y la función física no se comprueban.");
      return facts;
    }
    facts.push("Cambiar el dibujo abierto en CADCraft.","Dibujo: "+input.before.title,"Objetivo: "+input.description);
    if(op.action==="circle") facts.push(`Crear un círculo de ${number(op.diameter)} ${unit} de diámetro (ancho completo), con centro horizontal ${number(op.center[0])}, vertical ${number(op.center[1])} ${unit}.`);
    else facts.push(`Mover el círculo ${number(op.delta[0])} ${unit} en horizontal y ${number(op.delta[1])} ${unit} en vertical.`);
    facts.push(input.native_working_version===1?"Verás el cambio en CADCraft y puedes deshacerlo allí. El original permanece sin guardar; Hazlo incluye una nueva copia DXF y comprueba sus datos, geometría y unidades al reabrirla por separado. Se comparan las capas y los nombres de estilos y hojas; no todas sus propiedades. La vista y la función física no se comprueban.":"Verás el cambio en CADCraft y puedes deshacerlo allí. No se guarda ni exporta el dibujo.","Hazlo comprobará la identidad nativa y releerá la geometría. Cancelar detiene la espera y no deshace una edición aceptada.");
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
    if(input.buffer_checkpoint?.working_version) facts.push("Hazlo incluye una nueva copia del proyecto y conserva sus archivos junto al código de las pestañas abiertas. La copia usa otra carpeta; un guardado solicitado explícitamente conserva su destino nativo. Los registros internos de tareas quedan aparte.","En la copia, el texto abierto se guarda en UTF-8 con sus saltos de línea. Los archivos cerrados conservan sus bytes originales. La captura no guarda ni modifica las otras pestañas.","Arduino CLI reabre la estructura del sketch por separado y comprueba los archivos. No se compila ni se comprueban las bibliotecas externas, la vista del IDE o la función física.","La copia admite hasta 256 archivos, 128 carpetas y 32 MiB, con 8 MiB por archivo y hasta dos intentos nuevos. Si cambian las pestañas, Hazlo conserva el trabajo y pide revisar la copia; nunca repite la edición para recuperarla. El contenido permanece local salvo que apruebes compartirlo.");
    else if(input.buffer_checkpoint) facts.push("Hazlo conserva en este proyecto un registro local del código de las pestañas abiertas al terminar, incluidos sus cambios sin guardar. La captura no guarda ni modifica las otras pestañas.","Ese registro no es una copia completa del proyecto: los archivos cerrados, sus dependencias y una reapertura por separado siguen sin comprobarse. Su contenido permanece local salvo que apruebes compartirlo.");
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
    facts.push(saving ? "Guardar el documento abierto de FreeCAD." : exporting ? "Exportar la pieza abierta a STEP." : input.working_version && !input.operations.length ? "Conservar una versión del documento abierto de FreeCAD." : "Cambiar el documento abierto de FreeCAD.", "Objetivo: " + input.description);
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
    else if(input.working_version)facts.push(input.operations.length?"Las ediciones forman una transacción que puedes deshacer en FreeCAD.":"Conservar una nueva versión del documento actual, sin repetir ni cambiar su geometría.","Incluye una nueva copia nativa en la carpeta de esta operación y su comprobación al reabrirla en un proceso aislado, con propiedades, unidades, geometría y recursos nativos.","Se conservan el documento abierto, sus cambios actuales sin guardar y su archivo original. No se usa Guardar como ni se sobrescribe el original.","Esta comprobación admite documentos independientes de cajas, cilindros, esferas, conos y toros. Si no puede capturarse o comprobarse, el trabajo queda pendiente y la edición no se repite.","La vista gráfica y la función física no se comprueban. La carpeta conserva los archivos parciales si falla una etapa.");
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
