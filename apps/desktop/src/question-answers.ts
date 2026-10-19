import type {ChatQuestion} from "./contracts";

/** Wire format shared with the owned Codex question handler; never a renderer-supplied question. */
export async function questionAnswers(question:ChatQuestion,values:Record<string,FormDataEntryValue>){
  const answers:Record<string,string>={};
  for(const item of question.questions){
    if(item.isSecret)throw Error("Gestiona las credenciales en el cliente oficial; Hazlo no guarda preguntas secretas.");
    const value=String(values[item.id]??"").trim();
    if(!value || new TextEncoder().encode(value).length>2048)throw Error("Responde cada pregunta con hasta 2 KiB de texto.");
    answers[item.id]=value;
  }
  const data=JSON.stringify(["hazlo_question_answers_v1",question.id,question.questions.map(item=>[item.id,item.question,answers[item.id]])]);
  const hash=Array.from(new Uint8Array(await crypto.subtle.digest("SHA-256",new TextEncoder().encode(data))),b=>b.toString(16).padStart(2,"0")).join("");
  return {answers,hash};
}
