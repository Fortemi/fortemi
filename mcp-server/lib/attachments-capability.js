// Discovers whether the Fortemi API has attachment ingestion enabled (#1160).
//
// The API reports `capabilities.attachments_enabled` on `GET /health`. When it is
// `false`, attachment tools are hidden from tools/list and calls fail cleanly with
// the API's stable `attachments_disabled` code instead of emitting upload commands
// that would be rejected.

export const ATTACHMENTS_DISABLED_CODE = "attachments_disabled";

export const ATTACHMENT_TOOL_NAMES = new Set([
  "manage_attachments",
  "upload_attachment",
  "list_attachments",
  "get_attachment",
  "download_attachment",
  "delete_attachment",
]);

/** Absent or unknown capability means enabled; only an explicit `false` disables. */
export function attachmentsEnabledFromHealth(health) {
  return health?.capabilities?.attachments_enabled !== false;
}

export function filterAttachmentTools(toolList, attachmentsEnabled) {
  if (attachmentsEnabled) return toolList;
  return toolList.filter((tool) => !ATTACHMENT_TOOL_NAMES.has(tool.name));
}

export function attachmentsDisabledError() {
  const error = new Error(
    `${ATTACHMENTS_DISABLED_CODE}: attachment ingestion is disabled on this Fortemi deployment ` +
      "(FORTEMI_ATTACHMENTS_ENABLED=false). Store the content as note text instead."
  );
  error.code = ATTACHMENTS_DISABLED_CODE;
  return error;
}

/**
 * Cached probe. `fetchHealth` returns the parsed /health body. A failed probe keeps
 * the last known state (initially enabled); the API still fails closed on its own.
 */
export function createAttachmentsCapabilityProbe(fetchHealth, { ttlMs = 30_000, now = Date.now } = {}) {
  let enabled = true;
  let checkedAt = -Infinity;
  return async function isAttachmentsEnabled() {
    if (now() - checkedAt < ttlMs) return enabled;
    try {
      enabled = attachmentsEnabledFromHealth(await fetchHealth());
      checkedAt = now();
    } catch {
      // Keep the previous state; retry on the next call.
    }
    return enabled;
  };
}
