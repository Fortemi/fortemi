import { strict as assert } from "node:assert";
import { test } from "node:test";
import {
  ATTACHMENTS_DISABLED_CODE,
  attachmentsDisabledError,
  attachmentsEnabledFromHealth,
  createAttachmentsCapabilityProbe,
  filterAttachmentTools,
} from "../lib/attachments-capability.js";

test("only an explicit false disables attachments", () => {
  assert.equal(attachmentsEnabledFromHealth({ capabilities: { attachments_enabled: false } }), false);
  assert.equal(attachmentsEnabledFromHealth({ capabilities: { attachments_enabled: true } }), true);
  assert.equal(attachmentsEnabledFromHealth({ capabilities: {} }), true);
  assert.equal(attachmentsEnabledFromHealth(undefined), true);
});

test("attachment tools are hidden only when disabled", () => {
  const tools = [{ name: "list_notes" }, { name: "manage_attachments" }, { name: "upload_attachment" }];
  assert.deepEqual(filterAttachmentTools(tools, true), tools);
  assert.deepEqual(filterAttachmentTools(tools, false).map((t) => t.name), ["list_notes"]);
});

test("disabled error carries the stable API code", () => {
  const error = attachmentsDisabledError();
  assert.equal(error.code, ATTACHMENTS_DISABLED_CODE);
  assert.match(error.message, /^attachments_disabled:/);
});

test("probe caches within ttl and keeps last state on failure", async () => {
  let clock = 0;
  let calls = 0;
  let response = { capabilities: { attachments_enabled: false } };
  const probe = createAttachmentsCapabilityProbe(
    async () => {
      calls += 1;
      if (response instanceof Error) throw response;
      return response;
    },
    { ttlMs: 100, now: () => clock }
  );
  assert.equal(await probe(), false);
  assert.equal(await probe(), false);
  assert.equal(calls, 1);
  clock = 200;
  response = new Error("down");
  assert.equal(await probe(), false);
  response = { capabilities: { attachments_enabled: true } };
  assert.equal(await probe(), true);
});
