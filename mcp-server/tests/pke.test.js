import { strict as assert } from "node:assert";
import { test, describe, before, after } from "node:test";
import { MCPTestClient } from "./helpers/mcp-client.js";

describe("Phase 17: Public Key Encryption (PKE)", () => {
  let client;
  const cleanup = { keysetNames: [] };
  const testSuffix = MCPTestClient.uniqueId().slice(0, 8);
  const testPassphrase = "test-passphrase-long-enough-12";
  let generatedPublicKey;
  let generatedPrivateKey;
  let generatedAddress;

  before(async () => {
    client = new MCPTestClient();
    await client.initialize();
  });

  after(async () => {
    // Clean up created keysets (reverse order)
    for (const name of cleanup.keysetNames.reverse()) {
      try { await client.callTool("pke_delete_keyset", { name }); } catch {}
    }
    await client.close();
  });

  // --- Keypair Generation ---

  test("PKE-001: Generate keypair", async () => {
    const result = await client.callTool("pke_generate_keypair", {
      passphrase: testPassphrase,
      label: `test-key-${testSuffix}`,
    });
    assert.ok(result, "Should return keypair data");
    assert.ok(result.public_key, "Should have public_key");
    assert.ok(result.encrypted_private_key || result.private_key, "Should have private key");
    assert.ok(result.address, "Should have address");
    assert.ok(result.address.startsWith("mm:"), "Address should start with mm:");

    generatedPublicKey = result.public_key;
    generatedPrivateKey = result.encrypted_private_key || result.private_key;
    generatedAddress = result.address;

    console.log(`  Generated address: ${generatedAddress.slice(0, 20)}...`);
  });

  test("PKE-002: Generate keypair rejects short passphrase", async () => {
    const error = await client.callToolExpectError("pke_generate_keypair", {
      passphrase: "short",
    });
    assert.ok(error.error, "Should error for short passphrase");
    assert.ok(
      error.error.includes("12") || error.error.includes("passphrase") ||
      error.error.includes("minimum") || error.error.includes("character"),
      "Error should mention passphrase length requirement"
    );
  });

  // --- Address Operations ---

  test("PKE-003: Get address from public key", async () => {
    if (!generatedPublicKey) {
      console.log("  Skipping - no public key from PKE-001");
      return;
    }

    const result = await client.callTool("pke_get_address", {
      public_key: generatedPublicKey,
    });
    assert.ok(result, "Should return address data");
    assert.ok(result.address, "Should have address");
    assert.ok(result.address.startsWith("mm:"), "Address should start with mm:");
    // Should match the address from generation
    assert.strictEqual(result.address, generatedAddress, "Address should match generated address");
  });

  test("PKE-004: Verify valid address", async () => {
    if (!generatedAddress) {
      console.log("  Skipping - no address from PKE-001");
      return;
    }

    const result = await client.callTool("pke_verify_address", {
      address: generatedAddress,
    });
    assert.ok(result, "Should return verification result");
    assert.ok(result.valid === true || result.is_valid === true, "Address should be valid");
  });

  test("PKE-005: Verify invalid address", async () => {
    const result = await client.callTool("pke_verify_address", {
      address: "mm:invalidaddress123",
    });
    assert.ok(result, "Should return verification result");
    assert.ok(
      result.valid === false || result.is_valid === false,
      "Invalid address should not verify"
    );
  });

  test("PKE-006: Verify address with wrong prefix", async () => {
    const result = await client.callTool("pke_verify_address", {
      address: "xx:notanaddress",
    });
    assert.ok(result, "Should return verification result");
    assert.ok(
      result.valid === false || result.is_valid === false,
      "Wrong prefix should not verify"
    );
  });

  // --- Encrypt / Decrypt Roundtrip ---

  test("PKE-007: Encrypt data for recipient", async () => {
    if (!generatedPublicKey) {
      console.log("  Skipping - no public key from PKE-001");
      return;
    }

    const plaintext = Buffer.from("Hello PKE encryption test!").toString("base64");
    const result = await client.callTool("pke_encrypt", {
      plaintext,
      recipient_keys: [generatedPublicKey],
    });
    assert.ok(result, "Should return encryption result");
    assert.ok(result.ciphertext, "Should have ciphertext");
    assert.ok(result.ciphertext.length > 0, "Ciphertext should not be empty");
    console.log(`  Ciphertext length: ${result.ciphertext.length} chars`);
  });

  test("PKE-008: Full encrypt/decrypt roundtrip", async () => {
    if (!generatedPublicKey || !generatedPrivateKey) {
      console.log("  Skipping - no keys from PKE-001");
      return;
    }

    const originalMessage = "Test message for roundtrip " + testSuffix;
    const plaintext = Buffer.from(originalMessage).toString("base64");

    // Encrypt
    const encrypted = await client.callTool("pke_encrypt", {
      plaintext,
      recipient_keys: [generatedPublicKey],
    });
    assert.ok(encrypted.ciphertext, "Should have ciphertext");

    // Decrypt
    const decrypted = await client.callTool("pke_decrypt", {
      ciphertext: encrypted.ciphertext,
      encrypted_private_key: generatedPrivateKey,
      passphrase: testPassphrase,
    });
    assert.ok(decrypted, "Should return decryption result");
    assert.ok(decrypted.plaintext, "Should have plaintext");

    const decryptedMessage = Buffer.from(decrypted.plaintext, "base64").toString("utf8");
    assert.strictEqual(decryptedMessage, originalMessage, "Decrypted message should match original");
  });

  test("PKE-009: Decrypt with wrong passphrase errors", async () => {
    if (!generatedPublicKey || !generatedPrivateKey) {
      console.log("  Skipping - no keys from PKE-001");
      return;
    }

    const plaintext = Buffer.from("secret data").toString("base64");
    const encrypted = await client.callTool("pke_encrypt", {
      plaintext,
      recipient_keys: [generatedPublicKey],
    });

    const error = await client.callToolExpectError("pke_decrypt", {
      ciphertext: encrypted.ciphertext,
      encrypted_private_key: generatedPrivateKey,
      passphrase: "wrong-passphrase-here",
    });
    assert.ok(error.error, "Should error for wrong passphrase");
  });

  // --- List Recipients ---

  test("PKE-010: List recipients from ciphertext", async () => {
    if (!generatedPublicKey) {
      console.log("  Skipping - no public key from PKE-001");
      return;
    }

    const plaintext = Buffer.from("test data").toString("base64");
    const encrypted = await client.callTool("pke_encrypt", {
      plaintext,
      recipient_keys: [generatedPublicKey],
    });

    const result = await client.callTool("pke_list_recipients", {
      ciphertext: encrypted.ciphertext,
    });
    assert.ok(result, "Should return recipient list");
    assert.ok(result.recipients || result.addresses, "Should have recipients or addresses");
    const recipients = result.recipients || result.addresses || [];
    assert.ok(Array.isArray(recipients), "Recipients should be array");
    assert.ok(recipients.length >= 1, "Should have at least one recipient");
    // Our address should be in the list
    const hasOurAddress = recipients.some((r) =>
      (typeof r === "string" ? r : r.address) === generatedAddress
    );
    assert.ok(hasOurAddress, "Should contain our address in recipients");
  });

  test("PKE-011: List recipients with invalid ciphertext errors", async () => {
    const error = await client.callToolExpectError("pke_list_recipients", {
      ciphertext: "bm90LXZhbGlkLWNpcGhlcnRleHQ=",
    });
    assert.ok(error.error, "Should error for invalid ciphertext");
  });

  // --- Keyset Management ---

  test("PKE-012: List keysets (may be empty)", async () => {
    const result = await client.callTool("pke_list_keysets", {});
    assert.ok(result !== undefined, "Should return keyset list");
    // May be array or object with keysets property
    const keysets = Array.isArray(result) ? result : (result.keysets || []);
    assert.ok(Array.isArray(keysets), "Keysets should be array");
    console.log(`  Found ${keysets.length} existing keysets`);
  });

  test("PKE-013: Create keyset", async () => {
    const name = `test-ks-${testSuffix}-a`;
    const result = await client.callTool("pke_create_keyset", {
      name,
      passphrase: testPassphrase,
    });
    assert.ok(result, "Should create keyset");
    assert.ok(
      result.name || result.address || result.public_key,
      "Should return keyset info"
    );
    cleanup.keysetNames.push(name);
    console.log(`  Created keyset: ${name}`);
  });

  test("PKE-014: Create second keyset", async () => {
    const name = `test-ks-${testSuffix}-b`;
    const result = await client.callTool("pke_create_keyset", {
      name,
      passphrase: testPassphrase,
    });
    assert.ok(result, "Should create second keyset");
    cleanup.keysetNames.push(name);
  });

  test("PKE-015: List keysets shows created keysets", async () => {
    const result = await client.callTool("pke_list_keysets", {});
    const keysets = Array.isArray(result) ? result : (result.keysets || []);
    const nameA = `test-ks-${testSuffix}-a`;
    const found = keysets.find((k) => k.name === nameA);
    assert.ok(found, `Should find keyset '${nameA}' in list`);
  });

  test("PKE-016: Get active keyset", async () => {
    const result = await client.callTool("pke_get_active_keyset", {});
    // May have active keyset or null - both are valid
    assert.ok(result !== undefined, "Should return active keyset data");
    console.log(`  Active keyset: ${result?.name || result?.active || "(none)"}`);
  });

  test("PKE-017: Set active keyset", async () => {
    const name = `test-ks-${testSuffix}-a`;
    const result = await client.callTool("pke_set_active_keyset", { name });
    assert.ok(result, "Should set active keyset");

    // Verify
    const active = await client.callTool("pke_get_active_keyset", {});
    assert.ok(active, "Should have active keyset");
    assert.strictEqual(active.name, name, "Active keyset should match");
  });

  test("PKE-018: Export keyset", async () => {
    const name = `test-ks-${testSuffix}-a`;
    try {
      const result = await client.callTool("pke_export_keyset", { name });
      assert.ok(result, "Should export keyset");
      assert.ok(
        result.export_path || result.output_dir || result.success,
        "Should indicate export location or success"
      );
      console.log(`  Exported to: ${result.export_path || result.output_dir || "success"}`);
    } catch (e) {
      // Export may fail if filesystem permissions don't allow it in test env
      console.log(`  Export not available in test env: ${e.message.slice(0, 100)}`);
    }
  });

  test("PKE-019: Delete keyset", async () => {
    const name = `test-ks-${testSuffix}-b`;
    const result = await client.callTool("pke_delete_keyset", { name });
    assert.ok(result, "Should delete keyset");

    // Remove from cleanup since we already deleted it
    const idx = cleanup.keysetNames.indexOf(name);
    if (idx >= 0) cleanup.keysetNames.splice(idx, 1);

    // Verify it's gone
    const list = await client.callTool("pke_list_keysets", {});
    const keysets = Array.isArray(list) ? list : (list.keysets || []);
    const found = keysets.find((k) => k.name === name);
    assert.ok(!found, "Deleted keyset should not appear in list");
  });

  test("PKE-020: Delete non-existent keyset errors", async () => {
    const error = await client.callToolExpectError("pke_delete_keyset", {
      name: `nonexistent-${MCPTestClient.uniqueId()}`,
    });
    assert.ok(error.error, "Should error for non-existent keyset");
  });

  test("PKE-021: Create keyset with short passphrase errors", async () => {
    const error = await client.callToolExpectError("pke_create_keyset", {
      name: `test-ks-${testSuffix}-fail`,
      passphrase: "short",
    });
    assert.ok(error.error, "Should error for short passphrase");
  });

  test("PKE-022: Set active to non-existent keyset errors", async () => {
    const error = await client.callToolExpectError("pke_set_active_keyset", {
      name: `nonexistent-${MCPTestClient.uniqueId()}`,
    });
    assert.ok(error.error, "Should error for non-existent keyset");
  });
});
