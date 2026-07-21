#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

default_to="$(git config --local --get amele.patchNotify.to || printf 'melihemik@noirlang.tr')"
subject_prefix="$(git config --local --get amele.patchNotify.subjectPrefix || printf 'PATCH amele-next')"
smtp_server="$(git config --local --get sendemail.smtpserver || true)"
smtp_port="$(git config --local --get sendemail.smtpserverport || true)"
smtp_encryption="$(git config --local --get sendemail.smtpencryption || true)"
smtp_user="$(git config --local --get sendemail.smtpuser || true)"
smtp_from="$(git config --local --get sendemail.from || true)"
smtp_domain="$(git config --local --get sendemail.smtpdomain || true)"

if [[ -d "$repo_root/.git/perl5/lib/perl5" ]]; then
  export PERL5LIB="$repo_root/.git/perl5/lib/perl5${PERL5LIB:+:$PERL5LIB}"
fi

if ! git send-email -h 2>&1 | grep -q 'git send-email'; then
  printf '%s\n' 'git send-email is not available in this environment.' >&2
  exit 1
fi

if [[ -z "${smtp_server:-}" || -z "${smtp_port:-}" || -z "${smtp_encryption:-}" || -z "${smtp_user:-}" || -z "${smtp_from:-}" || -z "${smtp_domain:-}" ]]; then
  printf '%s\n' 'sendemail config is incomplete. Configure sendemail.smtpserver, sendemail.smtpserverport, sendemail.smtpencryption, sendemail.smtpuser, sendemail.from, and sendemail.smtpdomain first.' >&2
  exit 1
fi

read -r -p "Son kaç commit gondermek istiyorsun? [1]: " commit_count
commit_count="${commit_count:-1}"

if ! [[ "$commit_count" =~ ^[0-9]+$ ]] || [[ "$commit_count" -lt 1 ]]; then
  printf '%s\n' 'Geçerli bir pozitif sayı gir.' >&2
  exit 1
fi

if ! git rev-parse --verify "HEAD~$commit_count" >/dev/null 2>&1; then
  printf 'Bu repoda HEAD~%s bulunamiyor. Daha az sayi dene.\n' "$commit_count" >&2
  exit 1
fi

tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/amele-send-email.XXXXXX")"
trap 'rm -rf "$tmp_dir"' EXIT

mapfile -t patch_files < <(git format-patch --quiet --output-directory "$tmp_dir" "HEAD~$commit_count..HEAD")

if [[ "${#patch_files[@]}" -eq 0 ]]; then
  printf '%s\n' 'Gönderilecek patch bulunamadi.' >&2
  exit 1
fi

git send-email \
  --to="$default_to" \
  --suppress-cc=all \
  --subject-prefix="$subject_prefix" \
  --smtp-server="$smtp_server" \
  --smtp-server-port="$smtp_port" \
  --smtp-encryption="$smtp_encryption" \
  --smtp-user="$smtp_user" \
  --smtp-domain="$smtp_domain" \
  --from="$smtp_from" \
  "${patch_files[@]}"
