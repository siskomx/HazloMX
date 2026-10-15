"use strict";
const {test} = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs/promises");
const path = require("node:path");
const crypto = require("node:crypto");
const {execFile} = require("node:child_process");
const {promisify} = require("node:util");

const child = String.raw`
const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),Module=require('node:module');
let reads=0;
const api={env:{appName:process.env.OWNED_EDITOR_APP},window:{get activeTextEditor(){reads++;throw Error('Must not read before authentication');}}};
const loader=Module._load;
Module._load=function(name,...args){return name==='vscode'?api:loader.call(this,name,...args);};
const extension=require(process.env.OWNED_EDITOR_EXTENSION);Module._load=loader;
const context={subscriptions:[]},root=path.join(process.env.USERPROFILE,'.arduinoIDE','hazlomexa-editor');
(async()=>{
  if(process.env.OWNED_EXPECT_REJECT==='1'){
    assert.throws(()=>extension.activate(context),{message:'Native editor connection directory is invalid.'});
    assert(!fs.existsSync(path.join(process.env.OWNED_REAL_CONFIG,'hazlomexa-editor')));
  }else{
    extension.activate(context);
    if(process.env.OWNED_EDITOR_APP==='Arduino Agent'){
      let files=[];for(let i=0;i<100;i++){files=fs.existsSync(root)?fs.readdirSync(root):[];if(files.length)break;await new Promise(r=>setTimeout(r,20));}
      assert.equal(files.length,1);
      const record=JSON.parse(fs.readFileSync(path.join(root,files[0]),'utf8'));
      assert.equal(record.editor_version,1);assert.match(record.token,/^[0-9a-f]{64}$/);
      assert.equal(files[0],record.instance_id+'.json');
      assert.equal(record.pipe,'\\\\.\\pipe\\hazlomexa-arduino-'+record.instance_id);
      for(const item of context.subscriptions)item.dispose();assert.deepEqual(fs.readdirSync(root),[]);
    }else{assert(!fs.existsSync(root));assert.equal(context.subscriptions.length,0);}
  }
  assert.equal(reads,0);console.log('Owned profile test passed');
})().catch(()=>{console.error('Owned profile test failed');process.exitCode=1;for(const item of context.subscriptions)item.dispose();});
`;

async function runFixture(app, junction=false) {
  const target=await fs.realpath(path.resolve(__dirname,"../../target"));
  const root=path.join(target,"native-editor-profile-"+crypto.randomUUID());
  const profile=path.join(root,"profile"), real=path.join(root,"real-config");
  await fs.mkdir(profile,{recursive:true});
  try {
    if(junction){await fs.mkdir(real);await fs.symlink(real,path.join(profile,".arduinoIDE"),"junction");}
    else await fs.mkdir(path.join(profile,".arduinoIDE"));
    const env={...process.env,USERPROFILE:profile,OWNED_EDITOR_APP:app,OWNED_REAL_CONFIG:real,
      OWNED_EXPECT_REJECT:junction?"1":"0",OWNED_EDITOR_EXTENSION:path.resolve(__dirname,"../../apps/desktop/src-tauri/resources/arduino-editor/extension.cjs")};
    delete env.HAZLOMEXA_AGENT_EDITOR_ROOT;
    const result=await promisify(execFile)(process.execPath,["-e",child],{env,timeout:7000,maxBuffer:65536,windowsHide:true});
    assert.equal(result.stdout.trim(),"Owned profile test passed");assert.equal(result.stderr,"");
  } finally {
    assert.equal(path.dirname(await fs.realpath(root)),target);
    await fs.rm(root,{recursive:true,force:true});
  }
}
test("default-profile activation advertises the companion and stays dormant in another IDE", async()=>{
  await runFixture("Arduino Agent");await runFixture("Visual Studio Code");
});
test("a redirected profile is refused before creating connection metadata",async()=>{await runFixture("Arduino Agent",true);});
