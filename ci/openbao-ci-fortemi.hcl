path "kv_internal/data/ci/fortemi/*" {
  capabilities = ["read"]
}

path "kv_internal/metadata/ci/fortemi/*" {
  capabilities = ["read", "list"]
}

path "kv_internal/data/ci/shared/cloudflare-api" {
  capabilities = ["read"]
}

path "kv_internal/metadata/ci/shared/cloudflare-api" {
  capabilities = ["read"]
}

path "kv_internal/data/ci/shared/docs-deploy" {
  capabilities = ["read"]
}

path "kv_internal/metadata/ci/shared/docs-deploy" {
  capabilities = ["read"]
}

path "kv_internal/data/ci/shared/ghcr-token" {
  capabilities = ["read"]
}

path "kv_internal/metadata/ci/shared/ghcr-token" {
  capabilities = ["read"]
}

path "kv_internal/data/ci/shared/mutsu-ssh-key" {
  capabilities = ["read"]
}

path "kv_internal/metadata/ci/shared/mutsu-ssh-key" {
  capabilities = ["read"]
}
