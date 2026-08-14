#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 3 ]]; then
  echo "usage: $0 <private-key> <ssh-config> <host-alias>" >&2
  exit 2
fi

private_key="$1"
ssh_config="$2"
host_alias="$3"
expected_fingerprint="SHA256:98FJbexEnVPwJAio08Qv53uEahv4u6V+wSoUQLyKFII"

test -s "$private_key"
test -s "$ssh_config"

public_key="$(ssh-keygen -y -f "$private_key")"
read -r key_type key_body _ <<<"$public_key"
case "$key_type" in
  ssh-* | ecdsa-sha2-*) ;;
  *)
    echo "mutsu shared key did not derive a supported OpenSSH public key" >&2
    exit 1
    ;;
esac
if [[ -z "$key_body" ]]; then
  echo "mutsu shared key did not contain public key material" >&2
  exit 1
fi

fingerprint="$(
  printf '%s\n' "$public_key" |
    ssh-keygen -lf - |
    awk '{print $2}'
)"
if [[ "$fingerprint" != "$expected_fingerprint" ]]; then
  echo "mutsu shared key fingerprint mismatch" >&2
  exit 1
fi

ssh -F "$ssh_config" "$host_alias" bash -s -- \
  "$key_type" "$key_body" "$expected_fingerprint" <<'REMOTE_SCRIPT'
set -euo pipefail

key_type="$1"
key_body="$2"
expected_fingerprint="$3"
authorized_keys_dir="/etc/ssh/authorized_keys"
authorized_keys_file="${authorized_keys_dir}/manitcor"
config_file="/etc/ssh/sshd_config.d/100-mutsu-ci-authorized-keys.conf"

key_tmp="$(mktemp)"
config_tmp="$(mktemp)"
cleanup() {
  rm -f "$key_tmp" "$config_tmp"
}
trap cleanup EXIT

printf '%s %s\n' "$key_type" "$key_body" >"$key_tmp"
fingerprint="$(ssh-keygen -lf "$key_tmp" | awk '{print $2}')"
test "$fingerprint" = "$expected_fingerprint"

printf '%s\n' \
  'AuthorizedKeysFile .ssh/authorized_keys /etc/ssh/authorized_keys/%u' \
  >"$config_tmp"

sudo -n install -d -o root -g wheel -m 755 "$authorized_keys_dir"
sudo -n install -o root -g wheel -m 600 "$key_tmp" "$authorized_keys_file"
sudo -n install -o root -g wheel -m 644 "$config_tmp" "$config_file"
sudo -n /usr/sbin/sshd -t

effective="$(
  sudo -n /usr/sbin/sshd -T \
    -C user=manitcor,host=mutsu,addr=127.0.0.1 |
    awk '$1 == "authorizedkeysfile" {$1=""; sub(/^ /, ""); print}'
)"
case " $effective " in
  *" /etc/ssh/authorized_keys/%u "*) ;;
  *)
    echo "boot-available mutsu authorized-keys path is not effective" >&2
    exit 1
    ;;
esac

sudo -n ssh-keygen -lf "$authorized_keys_file" |
  awk '{print "mutsu boot SSH key installed: " $2}'
REMOTE_SCRIPT
