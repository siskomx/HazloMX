import assert from "node:assert/strict";
import {readFile} from "node:fs/promises";
import {transformWithOxc} from "vite";

const source = await readFile(new URL("../src/proposal-review.ts",import.meta.url),"utf8");
const {code} = await transformWithOxc(source,"proposal-review.ts");
const {proposalFacts} = await import(`data:text/javascript;base64,${Buffer.from(code).toString("base64")}`);
const review = input => proposalFacts({input}).join("\n");

assert.match(review({dimensions_mm:{length:25.4,width:12.7,height:5}}),/milímetros: largo 25.4, ancho 12.7, alto 5/);
assert.match(review({fqbn:"arduino:avr:uno",source:"code"}),/compila para Arduino Uno/);
assert.match(review({fqbn:"arduino:avr:uno",source:"code"}),/No se carga código/);
assert.match(review({agent_version:"0.7.1",source_name:"Sketch.ino"}),/copia con el código propuesto/);
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
const slice=review({orca_version:"2.4.2",source_name:"Part.stl",units:"in",mesh:{bounds_mm:[[-25.4,0,-1],[0,50.8,2]]},settings:{machine:"Printer",filament:"PLA",process:"Fine",nozzle_mm:0.4,layer_mm:0.2,nozzle_c:210,bed_c:60}});
assert.match(slice,/archivo en pulgadas/);
assert.match(slice,/interpretado en milímetros: largo 25.4, ancho 50.8, alto 3/);
assert.match(slice,/Boquilla: 0.4 mm\. Altura de capa: 0.2 mm/);
assert.match(slice,/no se envían a una impresora/);
const folder=proposalFacts({input:{fqbn:"arduino:avr:uno"},audit:{destination:"owned-output"}});
assert.equal(folder.at(-1),"Carpeta de resultado dentro del proyecto: owned-output");
assert(!folder.join(" ").includes("undefined"));
console.log("PASS: source-derived Spanish approval facts; all operations, 33 native drawing actions, explicit units, diameter, absolute PCB positions, compile/slice scope and destination");
