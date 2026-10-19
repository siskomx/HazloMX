import assert from "node:assert/strict";
import {readFile} from "node:fs/promises";
import {createHash,webcrypto} from "node:crypto";
import {transformWithOxc} from "vite";

globalThis.crypto??=webcrypto;
const {code}=await transformWithOxc(await readFile(new URL("../src/question-answers.ts",import.meta.url),"utf8"),"question-answers.ts");
const {questionAnswers}=await import(`data:text/javascript;base64,${Buffer.from(code).toString("base64")}`);
const question={id:"11111111-1111-4111-8111-111111111111",questions:[{id:"units",question:'¿Medida? "μ"\n',header:"not retained",isOther:true,isSecret:false}]};
const canonical='["hazlo_question_answers_v1","11111111-1111-4111-8111-111111111111",[["units","¿Medida? \\"μ\\"\\n","sí 🧰\\\\mm"]]]';
const result=await questionAnswers(question,{units:" sí 🧰\\mm "});
assert.deepEqual(result.answers,{units:"sí 🧰\\mm"});
assert.equal(result.hash,createHash("sha256").update(canonical).digest("hex"));
await assert.rejects(questionAnswers({...question,questions:[{...question.questions[0],isSecret:true}]},{units:"credential"}));
for(const answer of [" ","é".repeat(1025)])await assert.rejects(questionAnswers(question,{units:answer}));
assert.equal((await questionAnswers(question,{units:"é".repeat(1024)})).answers.units.length,1024);
const large={...question,questions:[0,1,2].map(n=>({...question.questions[0],id:`q${n}`,question:"q".repeat(4096)}))};
assert.equal(Object.keys((await questionAnswers(large,Object.fromEntries([0,1,2].map(n=>[`q${n}`,"a".repeat(2048)])))).answers).length,3);
console.log("Owned question codec: canonical Unicode bytes, trimmed answers, secret refusal and UTF-8 bounds passed.");
