#!/usr/bin/env bash
# Install the CLI.
set -euo pipefail

PREFIX="${PREFIX:-$HOME/.local}"
readonly VERSION=1.2.3

usage() {
  echo "usage: $0 [--prefix DIR]" >&2
  exit 1
}

for arg in "$@"; do
  case "$arg" in
    --prefix=*) PREFIX="${arg#*=}" ;;
    -h|--help) usage ;;
    *) echo "unknown: $arg"; usage ;;
  esac
done

if [[ ! -d "$PREFIX/bin" ]]; then
  mkdir -p "$PREFIX/bin"
fi
count=$(ls "$PREFIX/bin" | wc -l)
while read -r line; do
  printf '%s\n' "$line"
done < <(grep -v '^#' config.txt)
cat <<EOF > "$PREFIX/README"
Installed version $VERSION ($count files)
EOF
export PATH="$PREFIX/bin:$PATH"
