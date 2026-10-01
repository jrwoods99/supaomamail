#!/bin/sh
# Registers SupaOmaMail as a desktop handler for mailto: links.
#
# The plugin is not an installed application until this file exists: XDG only
# offers handlers that ship a .desktop claiming the scheme. The Exec path is
# this checkout, so a development symlink and a cloned install both work.
set -eu

fail() {
  printf '%s\n' "$1" >&2
  exit 1
}

plugin_dir=${1:-}
claim_default=false
[ -n "$plugin_dir" ] || fail 'usage: register-mailto.sh <plugin-dir> [--claim-default]'
shift
while [ "$#" -gt 0 ]; do
  case "$1" in
    --claim-default) claim_default=true ;;
    *) fail 'usage: register-mailto.sh <plugin-dir> [--claim-default]' ;;
  esac
  shift
done
plugin_dir=$(cd "$plugin_dir" && pwd)
[ -x "$plugin_dir/scripts/mailto.sh" ] || fail 'register-mailto.sh: mailto.sh is missing'
[ -f "$plugin_dir/ui/assets/omamail.svg" ] || fail 'register-mailto.sh: omamail.svg is missing'

data_home=${XDG_DATA_HOME:-${HOME:?}/.local/share}
apps="$data_home/applications"
mkdir -p "$apps"
desktop="$apps/supaomamail.desktop"
# SupaOmaMail installs beside Omamail, so it never takes the mailto default
# on its own: only --claim-default does, which default-mail.sh passes when
# the user asks for it in Settings. Every run refreshes the Exec path.

cat > "$desktop" <<EOF
[Desktop Entry]
Type=Application
Name=SupaOmaMail
Comment=Email client for Omarchy, based on Omamail
Exec=$plugin_dir/scripts/mailto.sh %u
Icon=$plugin_dir/ui/assets/omamail.svg
Terminal=false
StartupNotify=false
MimeType=x-scheme-handler/mailto;
Categories=Office;Network;Email;
EOF

if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database "$apps" >/dev/null 2>&1 || true
fi

if [ "$claim_default" = true ]; then
  if command -v xdg-mime >/dev/null 2>&1; then
    xdg-mime default supaomamail.desktop x-scheme-handler/mailto >/dev/null 2>&1 || true
  fi
  if command -v xdg-settings >/dev/null 2>&1; then
    xdg-settings set default-url-scheme-handler mailto supaomamail.desktop >/dev/null 2>&1 || true
  fi
fi
