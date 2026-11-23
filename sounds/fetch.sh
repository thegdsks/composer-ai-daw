#!/bin/sh
# Fetches the packs listed in sounds/packs.tsv into sounds/packs/<id>/ (gitignored), checks
# every sha256 and fails closed on a mismatch. Needs curl, shasum and bsdtar (reads 7z and tar.xz).
# usage: sounds/fetch.sh [id ...]   (no ids means every row)
set -eu
here=$(cd "$(dirname "$0")" && pwd)
packs="$here/packs"
mkdir -p "$packs/.dl"
sum() { shasum -a 256 "$1" | cut -d' ' -f1; }
tab=$(printf '\t')
while IFS="$tab" read -r id url asha ssha; do
  case "$id" in ''|'#'*) continue ;; esac
  if [ "$#" -gt 0 ]; then
    want=no
    for a in "$@"; do [ "$a" = "$id" ] && want=yes; done
    [ "$want" = yes ] || continue
  fi
  out="$packs/$id"
  if [ -f "$out/bank.sf2" ] && [ "$(sum "$out/bank.sf2")" = "$ssha" ]; then
    echo "ok      $id"
    continue
  fi
  arc="$packs/.dl/$(basename "$url")"
  [ -f "$arc" ] || curl -fsSL --max-time 300 -o "$arc" "$url"
  if [ "$(sum "$arc")" != "$asha" ]; then
    echo "FAIL    $id archive checksum mismatch" >&2
    rm -f "$arc"
    exit 1
  fi
  tmp=$(mktemp -d "$packs/.x.XXXXXX")
  bsdtar -xf "$arc" -C "$tmp"
  sf2=$(find "$tmp" -type f -name '*.sf2' | head -n 1)
  if [ -z "$sf2" ] || [ "$(sum "$sf2")" != "$ssha" ]; then
    echo "FAIL    $id sf2 checksum mismatch" >&2
    rm -rf "$tmp"
    exit 1
  fi
  mkdir -p "$out"
  cp "$sf2" "$out/bank.sf2"
  for n in LICENSE.txt README.txt readme.txt; do
    f=$(find "$tmp" -type f -name "$n" | head -n 1)
    [ -n "$f" ] && cp "$f" "$out/$n"
  done
  rm -rf "$tmp"
  echo "fetched $id"
done < "$here/packs.tsv"
# Index of the wired packs for the browser sound list.
if command -v node >/dev/null 2>&1; then node "$here/index.mjs"; fi
# The browser app serves the packs through this link (gitignored).
mkdir -p "$here/../web/app/public"
ln -sfn ../../../sounds/packs "$here/../web/app/public/sounds-packs"
