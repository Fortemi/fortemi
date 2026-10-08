# Release image signing grant for the CI AppRole (issue #1158).
#
# Attach to the AppRole whose VAULT_CI_ROLE_ID/VAULT_CI_SECRET_ID the
# sign-release-images job uses. cosign's hashivault provider signs via
# transit/sign/<key>/sha2-256 and reads the public key from transit/keys/<key>.
# No key creation, rotation, export, config or deletion rights are granted.

path "transit/sign/fortemi-release-cosign/*" {
  capabilities = ["update"]
}

path "transit/keys/fortemi-release-cosign" {
  capabilities = ["read"]
}
