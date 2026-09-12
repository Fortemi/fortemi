import fs from "node:fs";
import Ajv2020 from "ajv/dist/2020.js";

// Core #422 owns these wire rules; the consumer pin binds schema and test vectors.
const schema = JSON.parse(fs.readFileSync(new URL("./dataset-capability.schema.json", import.meta.url), "utf8"));
const ajv = new Ajv2020({ strict: true, allErrors: false });
ajv.addSchema(schema);
const requestStructure = ajv.compile({ $ref: schema.$id + "#/$defs/request" });
const descriptorStructure = ajv.compile({ $ref: schema.$id + "#/$defs/descriptor" });
const versionPattern = new RegExp(schema.$defs.semver.pattern);

function parseVersion(value) {
  if (typeof value !== "string" || value.length > schema.$defs.semver.maxLength) return null;
  const match = versionPattern.exec(value);
  if (!match || match[0] !== value) return null;
  const core = match.slice(1, 4).map(Number);
  if (!core.every(Number.isSafeInteger)) return null;
  return { core, prerelease: match[4]?.split(".") ?? [] };
}

export function compareCapabilityVersions(left, right) {
  const a = parseVersion(left);
  const b = parseVersion(right);
  if (!a || !b) return null;
  for (let index = 0; index < 3; index++) {
    if (a.core[index] !== b.core[index]) return a.core[index] > b.core[index] ? 1 : -1;
  }
  if (!a.prerelease.length || !b.prerelease.length) {
    return a.prerelease.length ? -1 : b.prerelease.length ? 1 : 0;
  }
  for (let index = 0; index < Math.max(a.prerelease.length, b.prerelease.length); index++) {
    const leftPart = a.prerelease[index];
    const rightPart = b.prerelease[index];
    if (leftPart === rightPart) continue;
    if (leftPart === undefined) return -1;
    if (rightPart === undefined) return 1;
    const leftNumeric = /^[0-9]+$/.test(leftPart);
    const rightNumeric = /^[0-9]+$/.test(rightPart);
    if (leftNumeric !== rightNumeric) return leftNumeric ? -1 : 1;
    // Numeric prerelease identifiers can exceed Number's exact range.
    if (leftNumeric && leftPart.length !== rightPart.length) return leftPart.length > rightPart.length ? 1 : -1;
    return leftPart > rightPart ? 1 : -1;
  }
  return 0;
}

export function validateCapabilityRequest(request) {
  if (!requestStructure(request)) return false;
  return [request.required, request.optional ?? []].every(requirements => requirements.every(
    requirement => requirement.minimumVersion === undefined || parseVersion(requirement.minimumVersion) !== null,
  ));
}

// This validates the server's own advertisement, not caller-supplied descriptors.
export function validateServerCapabilityDescriptor(descriptor) {
  return descriptorStructure(descriptor)
    && parseVersion(descriptor.runtime.version) !== null
    && descriptor.capabilities.every(capability => parseVersion(capability.version) !== null);
}
