"use strict";
const crypto = require("node:crypto");
const path = require("node:path");

const LIMIT = 65536;
const hash = text => crypto.createHash("sha256").update(text, "utf8").digest("hex");
const uuid = value => typeof value === "string" && /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i.test(value);
function fields(value, expected) {
  return value && typeof value === "object" && !Array.isArray(value)
    && Object.keys(value).sort().join("|") === expected.slice().sort().join("|");
}

function createEditorTools(api) {
  const instance = crypto.randomUUID(), documents = new WeakMap();
  let busy = false;

  function observe(editor) {
    const doc = editor?.document;
    if (!doc || doc.isClosed || doc.uri.scheme !== "file"
        || !/^[a-z]:[\\/]/i.test(doc.uri.fsPath) || path.extname(doc.uri.fsPath).toLowerCase() !== ".ino"
        || typeof doc.isDirty !== "boolean" || !Number.isSafeInteger(doc.version) || doc.version < 1 || ![1, 2].includes(doc.eol)) throw Error("document");
    const source = doc.getText();
    if (typeof source !== "string" || Buffer.byteLength(source, "utf8") > LIMIT || source.includes("\0")) throw Error("limit");
    if (!documents.has(doc)) documents.set(doc, crypto.randomUUID());
    return {editor_version:1, instance_id:instance, document_token:documents.get(doc),
      name:path.basename(doc.uri.fsPath), version:doc.version, state_sha256:hash(source),
      source, eol:doc.eol, dirty:doc.isDirty, source_is_untrusted_data:true};
  }

  async function request(input) {
    let sent = false, confirmedRefusal = false, ownsBusy = false;
    try {
      if (fields(input, ["action"]) && input.action === "inspect") {
        return {ok:true, result:observe(api.window.activeTextEditor)};
      }
      if (!fields(input, ["action", "job_id", "instance_id", "document_token", "version", "state_sha256", "source"])
          || input.action !== "apply" || !uuid(input.job_id) || busy) throw Error("request");
      const editor = api.window.activeTextEditor, before = observe(editor);
      if (input.instance_id !== before.instance_id || input.document_token !== before.document_token
          || input.version !== before.version || input.state_sha256 !== before.state_sha256
          || typeof input.source !== "string" || input.source.includes("\0")
          || Buffer.byteLength(input.source, "utf8") > LIMIT || input.source === before.source
          || (before.eol === 1 ? input.source.includes("\r") : /(^|[^\r])\n|\r(?!\n)/.test(input.source))) throw Error("stale");
      busy = true; ownsBusy = true;
      const range = new api.Range(editor.document.positionAt(0), editor.document.positionAt(before.source.length));
      sent = true;
      const applied = await editor.edit(edit => edit.replace(range, input.source), {undoStopBefore:true, undoStopAfter:true});
      if (!applied) { confirmedRefusal = true; throw Error("native_refused"); }
      const after = observe(editor);
      if (after.document_token !== before.document_token || after.version <= before.version || after.source !== input.source) throw Error("readback");
      return {ok:true, result:{editor_version:1, job_id:input.job_id, status:"completed",
        before_sha256:before.state_sha256, after, native_edit_confirmed:true}};
    } catch {
      return {ok:false, outcome:sent && !confirmedRefusal ? "uncertain" : "refused", error:"native_editor_not_confirmed"};
    } finally {
      if (ownsBusy) busy = false;
    }
  }
  return {instance_id:instance, request};
}

module.exports = {createEditorTools};
