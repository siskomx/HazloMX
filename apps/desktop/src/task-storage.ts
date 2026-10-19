import { invoke } from "@tauri-apps/api/core";
export function taskRequest<T>(project:[string,string],request:Record<string,unknown>):Promise<T> {
  return invoke<T>("task_action",{projectId:project[0],projectDirectory:project[1],request});
}
export async function referenceData(file:File):Promise<string>{
  if(!file.size || file.size>32*1024*1024) throw Error("Elige un archivo de hasta 32 MiB.");
  const bytes=new Uint8Array(await file.arrayBuffer());let result="";
  for(let offset=0;offset<bytes.length;offset+=32768) result+=String.fromCharCode(...bytes.subarray(offset,offset+32768));
  return btoa(result);
}
