export type ImageReference = {media_type:"image/jpeg";data:string};
const MAX_BYTES=512*1024;
export async function prepareImageReference(file:File):Promise<ImageReference> {
  if(!file.size || file.size>8*1024*1024) throw Error("Elige una imagen PNG o JPEG de hasta 8 MiB.");
  const bytes=new Uint8Array(await file.arrayBuffer());
  const view=new DataView(bytes.buffer);
  let width=0,height=0;
  if(bytes.length>=24 && [137,80,78,71,13,10,26,10].every((v,i)=>bytes[i]===v)) {
    width=view.getUint32(16);height=view.getUint32(20);
  } else if(bytes[0]===255 && bytes[1]===216) {
    for(let at=2;at+4<=bytes.length;) {
      if(bytes[at]!==255) break;
      if(bytes[at+1]===255){at++;continue;}
      const marker=bytes[at+1];at+=2;
      const size=view.getUint16(at);
      if(size<2 || at+size>bytes.length) break;
      if([192,193,194].includes(marker) && size>=8){height=view.getUint16(at+3);width=view.getUint16(at+5);break;}
      if(marker===218 || marker===217) break;
      at+=size;
    }
  }
  if(!width || !height || width>10000 || height>10000 || width*height>20000000) throw Error("No se pudo leer una imagen PNG o JPEG de hasta 20 megapíxeles.");
  const bitmap=await createImageBitmap(file);
  try {
    const scale=Math.min(1,1536/Math.max(bitmap.width,bitmap.height));
    const canvas=document.createElement("canvas");canvas.width=Math.max(1,Math.round(bitmap.width*scale));canvas.height=Math.max(1,Math.round(bitmap.height*scale));
    const ctx=canvas.getContext("2d");if(!ctx) throw Error("No se pudo preparar la imagen.");
    ctx.fillStyle="white";ctx.fillRect(0,0,canvas.width,canvas.height);ctx.drawImage(bitmap,0,0,canvas.width,canvas.height);
    // Re-encoding drops the original file name and EXIF/GPS metadata.
    for(const quality of [.9,.75,.6]) {
      const blob=await new Promise<Blob|null>(resolve=>canvas.toBlob(resolve,"image/jpeg",quality));
      if(blob && blob.size<=MAX_BYTES) {
        const url=await new Promise<string>((resolve,reject)=>{const reader=new FileReader();reader.onload=()=>resolve(String(reader.result));reader.onerror=()=>reject(Error("No se pudo preparar la imagen."));reader.readAsDataURL(blob);});
        return {media_type:"image/jpeg",data:url.slice(url.indexOf(",")+1)};
      }
    }
    throw Error("La imagen preparada es demasiado grande. Recórtala o reduce su resolución.");
  } finally { bitmap.close(); }
}
