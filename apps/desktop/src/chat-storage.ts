import { CHAT_HISTORY_BYTES, savedConversation } from "./design";

let writes: Promise<void> = Promise.resolve();

async function transaction(key: string, value?: string): Promise<string | null> {
  const database = await new Promise<IDBDatabase>((resolve, reject) => {
    const request = indexedDB.open("hazlomexa-chat", 1);
    let blocked = false;
    request.onupgradeneeded = () => request.result.createObjectStore("conversations");
    request.onerror = () => reject(request.error);
    request.onblocked = () => { blocked = true; reject(Error("Cierra las otras ventanas de Hazlo para guardar la conversación.")); };
    request.onsuccess = () => { if (blocked) request.result.close(); else resolve(request.result); };
  });
  try {
    return await new Promise<string | null>((resolve, reject) => {
      const tx = database.transaction("conversations", value === undefined ? "readonly" : "readwrite", { durability: "strict" });
      const store = tx.objectStore("conversations");
      const request = value === undefined ? store.get(key) : store.put(value, key);
      tx.onabort = () => reject(tx.error ?? Error("No se confirmó el guardado de la conversación."));
      tx.onerror = () => reject(tx.error ?? Error("No se pudo guardar la conversación."));
      tx.oncomplete = () => {
        if (value !== undefined || request.result === undefined) resolve(null);
        else if (typeof request.result === "string" && savedConversation(request.result, CHAT_HISTORY_BYTES)) resolve(request.result);
        else reject(Error("La conversación guardada no es válida. Sus datos se conservan."));
      };
      if (value !== undefined && tx.durability !== "strict") tx.abort();
    });
  } finally { database.close(); }
}

export async function readConversation(key: string): Promise<string | null> {
  await writes.catch(() => {});
  return transaction(key);
}

export function writeConversation(key: string, value: string): Promise<void> {
  if (!savedConversation(value, CHAT_HISTORY_BYTES)) return Promise.reject(Error("La conversación supera el límite de guardado o no es válida."));
  const saved = writes.catch(() => {}).then(async () => { await transaction(key, value); });
  writes = saved;
  return saved;
}
