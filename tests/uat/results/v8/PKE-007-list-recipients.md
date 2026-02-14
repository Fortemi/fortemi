# UAT Test Report: PKE-007 - List Recipients

**Test ID:** PKE-007  
**Test Date:** 2026-02-14  
**Tester:** Claude (automated)  
**Status:** ✅ PASS

## Test Objective
Verify that `pke_list_recipients` correctly extracts and returns recipient addresses from encrypted ciphertext without decryption.

## Test Execution

### Step 1: Generate Keypair
**Tool:** `mcp__fortemi__pke_generate_keypair`  
**Input:**
- passphrase: "test-listrecip-2026"
- label: "uat-listrecip"

**Result:** ✅ Success
- Generated address: `mm:fNZsm6Yxqn5s494oYYyF3eBNCzBAHx836`
- Public key: `rpVPYOsrY8Rzb3n6uXkGH4h7IryOvIWCEAyI7uWhD14=`

### Step 2: Encrypt Test Data
**Tool:** `mcp__fortemi__pke_encrypt`  
**Input:**
- plaintext: `VGVzdCBjb250ZW50` (base64 of "Test content")
- recipient_keys: `["rpVPYOsrY8Rzb3n6uXkGH4h7IryOvIWCEAyI7uWhD14="]`

**Result:** ✅ Success
- Ciphertext size: 382 bytes
- Recipients embedded in header: `["mm:fNZsm6Yxqn5s494oYYyF3eBNCzBAHx836"]`

### Step 3: List Recipients
**Tool:** `mcp__fortemi__pke_list_recipients`  
**Input:**
- ciphertext: (382-byte MMPKE01 formatted ciphertext from step 2)

**Result:** ✅ Success
```json
{
  "recipients": [
    "mm:fNZsm6Yxqn5s494oYYyF3eBNCzBAHx836"
  ]
}
```

## Pass Criteria Verification

✅ **Criteria Met:** Returns recipients array containing the address from step 1

**Expected address:** `mm:fNZsm6Yxqn5s494oYYyF3eBNCzBAHx836`  
**Returned recipients:** `["mm:fNZsm6Yxqn5s494oYYyF3eBNCzBAHx836"]`  
**Verification:** Address found in recipients array - exact match

## Observations

1. **Header parsing works correctly** - Tool successfully extracts recipient addresses from MMPKE01 header without requiring decryption
2. **Address format validated** - Returned address matches wallet-style format (mm:...)
3. **No decryption required** - Operation is read-only on header metadata
4. **Single recipient case** - Test validates array contains exactly one recipient as expected

## Conclusion

**Status:** ✅ PASS

The `pke_list_recipients` tool correctly extracts recipient addresses from encrypted ciphertext. All pass criteria met:
- Recipients array returned
- Contains expected address from encryption step
- No errors encountered
- Tool operates without requiring private key or decryption

The PKE recipient listing functionality is working as designed.

---
**Test Completed:** 2026-02-14T16:36:15Z
