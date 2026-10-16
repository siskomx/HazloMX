import assert from "node:assert/strict";
import {readFile} from "node:fs/promises";
import {transformWithOxc} from "vite";

const source = await readFile(new URL("../src/proposal-review.ts",import.meta.url),"utf8");
const {code} = await transformWithOxc(source,"proposal-review.ts");
const {proposalFacts} = await import(`data:text/javascript;base64,${Buffer.from(code).toString("base64")}`);
const review = input => proposalFacts({input}).join("\n");

const live=review({live_cad_version:1,description:"Haz la carcasa más larga",before:[{name:"InternalBox",label:"Carcasa",properties:{Length:"25.4 mm"}}],operations:[{action:"edit_object",obj_name:"InternalBox",obj_properties:{Length:30.4}}]});
assert.match(live,/documento abierto de FreeCAD/);
assert.match(live,/Cambiar objeto: Carcasa/);
assert.match(live,/largo: 25.4 mm → 30.4 mm/);
assert.match(live,/No se guarda ni exporta el modelo/);
assert(!live.includes("InternalBox") && !live.includes("archivos originales se conservan"));

const save=review({live_cad_version:1,description:"Guarda la carcasa",before:[],operations:[{action:"save_as"}]});
assert.match(save,/Guardar el documento abierto de FreeCAD/);
assert.match(save,/FreeCAD abrirá Guardar como/);
assert.match(save,/Guardar no es una edición de geometría que pueda deshacerse/);
assert(!save.includes("No se guarda ni exporta") && !save.includes("nuevo objeto"));

const exported=review({live_cad_version:1,description:"Exporta la carcasa",before:[{name:"InternalBox",label:"Carcasa",properties:{}}],operations:[{action:"export_step",obj_name:"InternalBox"}]});
assert.match(exported,/Exportar la pieza abierta a STEP/);
assert.match(exported,/Pieza: Carcasa/);
assert.match(exported,/No se sobrescriben destinos existentes/);
assert.match(exported,/documento abierto no cambia ni se guarda/);
assert(!exported.includes("InternalBox") && !exported.includes("transacción que puedes deshacer"));

const coneInput={live_cad_version:1,description:"Ensanchamos los dos extremos del embudo",before:[{name:"InternalCone",label:"Embudo",type:"Part::Cone",properties:{Radius1:"10 mm",Radius2:5}}],operations:[{action:"edit_object",obj_name:"InternalCone",obj_properties:{Radius1:"0.5 in",Radius2:"0.25 in"}}]};
const coneOriginal=JSON.stringify(coneInput), cone=review(coneInput);
assert.match(cone,/diámetro del círculo de la base: 20 mm → 1 in/);
assert.match(cone,/diámetro del círculo del otro extremo: 10 mm → 0.5 in/);
assert(!cone.includes("Radius1") && !cone.includes("Radius2") && !cone.includes("InternalCone"));
assert.equal(cone.split("Diámetro significa ancho completo").length-1,1);
assert.equal(JSON.stringify(coneInput),coneOriginal,"Review must not rescale the approved native payload");
const torus=review({live_cad_version:1,description:"Ajusta el aro",before:[{name:"Ring",label:"Aro",type:"Part::Torus",properties:{Radius1:"10 mm",Radius2:"2 mm"}}],operations:[{action:"edit_object",obj_name:"Ring",obj_properties:{Radius1:"0.5 in",Radius2:"0.125 in"}}]});
assert.match(torus,/distancia del centro del aro al centro del tubo: 10 mm → 0.5 in/);
assert.match(torus,/diámetro del círculo que forma el tubo: 4 mm → 0.25 in/);
assert(!torus.includes("Radius1") && !torus.includes("Radius2"));
for(const [unit,value] of [["mm","0.5"],["cm",".5"],["m","5e-1"],["in","0.5"],["ft","0.5"]]) {
  const facts=review({live_cad_version:1,description:"Crea un cilindro",before:[],operations:[{action:"create_object",obj_type:"Part::Cylinder",obj_name:"Shaft",obj_properties:{Radius:`${value} ${unit}`}}]});
  assert(facts.includes(`diámetro del círculo de la base: 1 ${unit}.`));
}
const sphere=review({live_cad_version:1,description:"Cambia la esfera",before:[{name:"Sphere",label:"Bola",type:"Part::Sphere",properties:{Radius:5}}],operations:[{action:"edit_object",obj_name:"Sphere",obj_properties:{Radius:10}}]});
assert.match(sphere,/diámetro de la esfera completa: 10 mm → 20 mm/);
for(const value of ["1/2 in","(5 mm + 2 mm)","1,000 mm","Infinity mm"]){
  const facts=review({...coneInput,operations:[{action:"edit_object",obj_name:"InternalCone",obj_properties:{Radius1:value}}]});
  assert(facts.includes(`distancia del centro al borde de la base: 10 mm → ${value}.`));
  assert(!facts.includes("diámetro del círculo"),"Unknown expressions must not be presented as converted diameters");
}
const unknownCircular=review({live_cad_version:1,description:"Revisa otro objeto",before:[{name:"Unknown",label:"Otra pieza",type:"Other::Type",properties:{}}],operations:[{action:"edit_object",obj_name:"Unknown",obj_properties:{Radius1:10,constructor:5}}]});
assert.match(unknownCircular,/primer radio \(del centro al borde\): 10 mm/);
assert.match(unknownCircular,/constructor: 5/);
assert(!unknownCircular.includes("undefined") && !unknownCircular.includes("function Object"));

assert.match(review({dimensions_mm:{length:25.4,width:12.7,height:5}}),/milímetros: largo 25.4, ancho 12.7, alto 5/);
assert.match(review({fqbn:"arduino:avr:uno",source:"code"}),/compila para Arduino Uno/);
assert.match(review({fqbn:"arduino:avr:uno",source:"code"}),/No se carga código/);
assert.match(review({agent_version:"0.7.1",source_name:"Sketch.ino"}),/copia con el código propuesto/);
const nativeAgent=review({live_agent_version:1,description:"Reduce el tiempo de espera",before:{name:"Sketch.ino",dirty:true}});
const nativeSave=review({live_agent_version:1,save:true,description:"Guarda mi sketch",before:{name:"Sketch.ino",dirty:true}});
assert.match(nativeSave,/Guardar el sketch abierto/);assert.match(nativeSave,/no se elige otra ruta/);
assert.match(nativeSave,/Guardar no se deshace/);assert.match(nativeSave,/sin cambios pendientes/);
assert(!nativeSave.includes("Código propuesto"));
assert.match(nativeAgent,/sketch abierto de Arduino Agent/);
assert.match(nativeAgent,/cambios sin guardar/);
assert.match(nativeAgent,/preferencias de guardado/);
assert.match(nativeAgent,/No se carga código a una placa/);
assert(!nativeAgent.includes("Guardar una copia") && !nativeAgent.includes("archivos originales se conservan"));
for (const [unit,label] of [["mm","milímetros"],["cm","centímetros"],["m","metros"],["in","pulgadas"],["ft","pies"]]) {
  const request={operation:"add_cylinder",object:"Cylinder",units:unit,parameters:{radius:0.5,height:2,x:0,y:-1,z:3}};
  const facts=review({cadpilot_version:"0.7.7",source_name:"Part.FCStd",request});
  assert(facts.includes(`Medidas en ${label}`));
  assert.match(facts,/Diámetro completo: 1\. Alto: 2/);
  assert.match(facts,/horizontal 0, vertical -1, elevación 3/);
}
for (const operation of ["edit_box","add_box"]) {
  const facts=review({cadpilot_version:"0.7.7",source_name:"Part.FCStd",request:{operation,object:"Box",units:"in",parameters:{length:1,width:2,height:3,x:0,y:0,z:0}}});
  assert.match(facts,/Largo: 1\. Ancho: 2\. Alto: 3/);
  assert.equal(facts.includes("Posición de origen"),operation==="add_box");
}
assert.match(review({cadpilot_version:"0.7.7",source_name:"Part.FCStd",request:{operation:"inspect",object:"Box",units:"mm",parameters:{}}}),/Inspeccionar y medir/);
assert.match(review({cadpilot_version:"0.7.7",source_name:"Part.FCStd",request:{operation:"edit_box",object:"Box",units:"in",parameters:{length:1,width:2,height:3}},preview:{measurements:{Box:{dimensions_mm:{length:10,width:20,height:30}}}}}),/actuales en milímetros: largo 10, ancho 20, alto 30[\s\S]*Medidas en pulgadas[\s\S]*Largo: 1/);
const schematic=review({konnect_version:"0.13.0",source:{entry:"Circuit.kicad_sch"},edit:{reference:"R1",expected_value:"1k",value:"4.7k"}});
assert.match(schematic,/R1: 1k → 4.7k/);
const pcb=review({konnect_version:"0.13.0",source:{entry:"Board.kicad_pcb"},pcb_move:{reference:"J1",expected_x_mm:20,expected_y_mm:-10,x_mm:45.4,y_mm:-10}});
for (const [units,x,y,expected] of [["mm",201,198,"1 milímetro a la derecha y 2 milímetros hacia arriba"],
  ["in",197.46,200,"0.1 pulgadas a la izquierda"],["mils",200,200.254,"10 milésimas de pulgada hacia abajo"]]) {
  const input={live_pcb_version:1,board_name:"Board.kicad_pcb",description:"Mueve la pieza elegida",units,
    before:{components:[{uuid:"chosen",reference:"R1",value:"10k"}]},
    movement:{uuid:"chosen",reference:"R1",expected_x_mm:200,expected_y_mm:200,x_mm:x,y_mm:y}};
  const original=JSON.stringify(input), facts=review(input);
  assert(facts.includes("Desplazamiento: "+expected+"."));
  assert.match(facts,/Las pistas conservan su posición/);
  assert.equal(JSON.stringify(input),original,"Natural movement review must not alter the native proposal");
}
assert.match(pcb,/actual: horizontal 20, vertical -10 mm/);
assert.match(pcb,/Nueva posición: horizontal 45.4, vertical -10 mm/);
assert.match(pcb,/posiciones son absolutas/);
for (const [unit,label] of [["mm","milímetros"],["in","pulgadas"],["mils","milésimas de pulgada"]]) {
  const facts=review({kicad_version:"10.0.7",entry:"Board.kicad_pcb",units:unit});
  assert(facts.includes(`Unidades del informe: ${label}. No cambian la escala`));
}
const drawing={cadcraft_version:"0.5.0",source_name:"Plano.dxf",units:"in",output_format:"dxf",circle:{center:[1,2],radius:0.5},allow_loss:false};
assert.match(review(drawing),/pulgadas[\s\S]*diámetro 1, con centro horizontal 1, vertical 2/);
assert(!review(drawing).includes("permite perder"));
assert.match(review({...drawing,allow_loss:true}),/permite perder elementos/);
const rust=await readFile(new URL("../src-tauri/src/cadcraft.rs",import.meta.url),"utf8");
const commands=[...rust.slice(rust.indexOf("fn native_command("),rust.indexOf("fn bounded_params(")).matchAll(/"([a-z.]+)"/g)].map(match=>match[1]);
assert.equal(commands.length,33);
for (const command of commands) {
  const facts=review({...drawing,circle:null,native_tools:{intent:"Descripción que no sustituye las acciones",calls:[{command,params:{}}]}});
  assert(!facts.includes(`1. ${command}.`),`Native command ${command} needs a readable action`);
  assert.match(facts,/parámetros exactos/);
}
const unknown=review({...drawing,native_tools:{intent:"untrusted",calls:[{command:"constructor",params:{}}]}});
assert.match(unknown,/1\. constructor\./);
assert(!unknown.includes("function Object"));
const slice=review({orca_version:"2.4.2",source_name:"Part.stl",units:"in",mesh:{bounds_mm:[[-25.4,0,-1],[0,50.8,2]]},settings:{machine:"Printer",filament:"PLA",process:"Fine",bed:"High Temp Plate",nozzle_mm:0.4,layer_mm:0.2,nozzle_c:210,bed_c:60}});
assert.match(slice,/archivo en pulgadas/);
assert.match(slice,/Cama seleccionada: High Temp Plate/);
assert.match(slice,/interpretado en milímetros: largo 25.4, ancho 50.8, alto 3/);
assert.match(slice,/Boquilla: 0.4 mm\. Altura de capa: 0.2 mm/);
assert.match(slice,/no se envían a una impresora/);
const folder=proposalFacts({input:{fqbn:"arduino:avr:uno"},audit:{destination:"owned-output"}});
assert.equal(folder.at(-1),"Carpeta de resultado dentro del proyecto: owned-output");
assert(!folder.join(" ").includes("undefined"));
console.log("PASS: source-derived Spanish approval facts; all operations, 33 native drawing actions, explicit units, diameter, absolute PCB positions, compile/slice scope and destination");
