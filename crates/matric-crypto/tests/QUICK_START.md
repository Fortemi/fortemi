# PKE Test Suite - Quick Start Guide

## Run All Tests

```bash
cargo test -p matric-crypto --test pke_integration_test
```

Expected output:
```
running 55 tests
test result: ok. 55 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
finished in 2.46s
```

## Run by Category

### Cryptographic Correctness (9 tests)
```bash
cargo test -p matric-crypto --test pke_integration_test test_encrypt
cargo test -p matric-crypto --test pke_integration_test test_keypair
cargo test -p matric-crypto --test pke_integration_test test_multi_recipient
```

### Address Format (9 tests)
```bash
cargo test -p matric-crypto --test pke_integration_test test_address
```

### Error Handling (12 tests)
```bash
cargo test -p matric-crypto --test pke_integration_test test_reject
cargo test -p matric-crypto --test pke_integration_test test_wrong
cargo test -p matric-crypto --test pke_integration_test test_load
```

### Format Compliance (6 tests)
```bash
cargo test -p matric-crypto --test pke_integration_test test_format
cargo test -p matric-crypto --test pke_integration_test test_header
cargo test -p matric-crypto --test pke_integration_test test_can_decrypt
```

### Key Persistence (4 tests)
```bash
cargo test -p matric-crypto --test pke_integration_test test_save
cargo test -p matric-crypto --test pke_integration_test test_public_key
cargo test -p matric-crypto --test pke_integration_test test_private_key
```

### Edge Cases (8 tests)
```bash
cargo test -p matric-crypto --test pke_integration_test test_encrypt_empty
cargo test -p matric-crypto --test pke_integration_test test_encrypt_large
cargo test -p matric-crypto --test pke_integration_test test_encrypt_binary
```

### Security Properties (7 tests)
```bash
cargo test -p matric-crypto --test pke_integration_test test_ciphertext
cargo test -p matric-crypto --test pke_integration_test test_recipient_isolation
cargo test -p matric-crypto --test pke_integration_test test_metadata
```

## Run Single Test

```bash
cargo test -p matric-crypto --test pke_integration_test test_multi_recipient_encryption
```

## Run with Verbose Output

```bash
cargo test -p matric-crypto --test pke_integration_test -- --nocapture
```

## Run All matric-crypto Tests

```bash
cargo test -p matric-crypto
```

Expected: 173 tests passing (108 unit + 55 integration + 10 other)

## Test Files Location

```
crates/matric-crypto/tests/
├── pke_integration_test.rs          # Main test suite (55 tests)
├── PKE_TEST_DOCUMENTATION.md        # Detailed documentation
├── TEST_SUMMARY.md                  # Implementation summary
└── QUICK_START.md                   # This guide
```

## Coverage Summary

| Category | Tests | Status |
|----------|-------|--------|
| Cryptographic Correctness | 9 | ✅ |
| Address Format | 9 | ✅ |
| Error Handling | 12 | ✅ |
| Format Compliance | 6 | ✅ |
| Key Persistence | 4 | ✅ |
| Edge Cases | 8 | ✅ |
| Security Properties | 7 | ✅ |
| **Total** | **55** | ✅ |

## Key Tests to Watch

### Critical Security Tests
- `test_multi_recipient_non_recipients_cannot_decrypt` - Access control
- `test_ciphertext_authentication_integrity` - Tampering detection
- `test_reject_tampered_ciphertext` - Data integrity
- `test_recipient_isolation` - Privacy guarantee

### Critical Functionality Tests
- `test_encrypt_decrypt_roundtrip` - Basic encryption
- `test_multi_recipient_encryption` - Multi-recipient feature
- `test_address_checksum_detects_single_character_corruption` - Error detection
- `test_keypair_generation_produces_valid_keys` - Key generation

## Troubleshooting

### Test Failures

If tests fail, check:

1. **Build errors**: Run `cargo build -p matric-crypto` first
2. **Dependency issues**: Run `cargo update -p matric-crypto`
3. **Platform issues**: Some tests use tempfile, ensure /tmp is writable

### Performance Issues

If tests are slow:

1. **Run in release mode**: `cargo test -p matric-crypto --release`
2. **Parallel execution**: Tests run in parallel by default
3. **Skip slow tests**: Use `--skip` flag to skip specific tests

## Common Test Patterns

### Testing Encryption
```rust
let recipient = Keypair::generate();
let message = b"Test message";
let encrypted = encrypt_pke(message, &[recipient.public.clone()], None).unwrap();
let (decrypted, _) = decrypt_pke(&encrypted, &recipient.private).unwrap();
assert_eq!(message.as_slice(), decrypted.as_slice());
```

### Testing Error Handling
```rust
let result = some_operation_that_should_fail();
assert!(result.is_err());
assert!(result.unwrap_err().to_string().contains("expected error text"));
```

### Testing Key Persistence
```rust
let dir = tempdir().unwrap();
let path = dir.path().join("test.key.enc");
save_private_key(&keypair.private, &path, "password").unwrap();
let loaded = load_private_key(&path, "password").unwrap();
assert_eq!(keypair.private.as_bytes(), loaded.as_bytes());
```

## Documentation

- **Detailed Guide**: See `PKE_TEST_DOCUMENTATION.md`
- **Implementation Summary**: See `TEST_SUMMARY.md`
- **Code**: See `pke_integration_test.rs`

## Support

For questions or issues:
1. Review test documentation
2. Check implementation in `crates/matric-crypto/src/pke/`
3. Refer to Issue #339 for requirements

---

**Quick Start Version**: 1.0
**Last Updated**: 2026-02-02
