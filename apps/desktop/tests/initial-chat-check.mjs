import assert from "node:assert/strict";

// Shared read-only assertions; a browser fixture does not establish installed native acceptance.
export async function checkInitialChat(page, snapshot) {
  await page.locator("#open-project:enabled").waitFor({state:"attached"});
  assert.equal(snapshot.project, null);
  assert.equal(snapshot.job, null);
  assert.match(await page.locator("#conversation").innerText(), /No necesitas conocer/);
  assert.equal(await page.locator("#length, #width, #height, #pcb-x, #konnect-value, #sketch-source, #workflow").count(), 0, "Legacy engineering controls replaced the chat");
  assert.deepEqual(await page.locator("#program option").evaluateAll(options => options.map(option => option.value)), ["planning", "freecad", "drawing", "arduino", "agent", "kicad", "slice"]);
  assert.deepEqual(await page.locator("#provider option").evaluateAll(options => options.map(option => option.value)), ["codex", "claude", "ollama_local", "ollama_cloud"]);
  assert.equal(await page.locator("#program").inputValue(), "planning");
  assert.equal(await page.locator("#program-actions button").count(), 0);
  assert.equal(await page.locator("#prompt").inputValue(), "");
  assert.equal(await page.locator("#send").isDisabled(), true);
  assert.equal(await page.locator("#approve").isDisabled(), true);
  assert.equal(await page.locator("#approval").isVisible(), false);
  assert.equal(await page.locator("#create-project").isDisabled(), false);
  assert.equal(await page.locator("#open-project").isDisabled(), false);
  assert.equal(await page.locator("#connections").getAttribute("open"), null);
  return {program_groups: 6, agent_choices: 4, no_engineering_forms: true, no_implicit_proposal: true, send_disabled_without_project: true};
}
