UAT Test PKE-002: Generate Second Keypair
==========================================

Test ID: PKE-002
Status: PASS
Execution Time: 2026-02-14T16:28:43Z

Test Parameters:
- Passphrase: "secondary-passphrase-2026"
- Label: "uat-secondary"

Results:
--------
✓ Keypair generated successfully
✓ Returned different address from PKE-001
✓ All required fields present

Address: mm:mJBzUcb17dUqteGvkmFNXYAV6qx9jAnfW
Public Key: 9QzA6sq+r9mwSpkgt87qTE/VljLHIi4tx6zGOsIj41U=
Encrypted Private Key: TU1QS0VLRVneAAAAeyJ2ZXJzaW9uIjoxLCJrZGYiOiJhcmdvbjJpZCIsImtkZl9wYXJhbXMiOnsibWVtb3J5X2tpYiI6NjU1MzYsIml0ZXJhdGlvbnMiOjMsInBhcmFsbGVsaXNtIjo0fSwic2FsdCI6Ik1PR1pnWXJ4dXRqbHVwY1J2ZFVwc0diWVU2VDhlZ0NHV3VRMWx1TGthSU09Iiwibm9uY2UiOiJLcVNKSFFQUzBnNmQxVFpkIiwiY3JlYXRlZF9hdCI6IjIwMjYtMDItMTRUMTY6Mjg6NDMuOTA4NTE2NjM1WiJ9HnzzCvFFk9vIVkyeAa9fuZNkrakUFCILwpRihdy8TzErLaHfVaPBLoT0jz0V6k6O
Label: uat-secondary
Output Dir: null

Verification:
- Address format: Valid (mm: prefix, 33 characters)
- Public key: Base64 encoded (44 characters)
- Encrypted private key: Base64 encoded MMPKEKE format (224 characters)
- Label: Correctly set to "uat-secondary"

Pass Criteria Met:
✓ Returns different address than PKE-001
✓ Returns public_key field
✓ Returns encrypted_private_key field
✓ Address starts with "mm:"
✓ All fields properly formatted

Conclusion: PASS
