#!/usr/bin/env bash
set -euo pipefail

repo="${GITHUB_ACTION_REPOSITORY:-anasm266/apibump}"
version="${INPUT_VERSION:-v0.1.0}"
runner_os="${RUNNER_OS:-Linux}"
runner_arch="${RUNNER_ARCH:-X64}"

case "$runner_os-$runner_arch" in
  Linux-X64)
    target="x86_64-unknown-linux-gnu"
    extension="tar.gz"
    binary="apibump"
    ;;
  macOS-X64)
    target="x86_64-apple-darwin"
    extension="tar.gz"
    binary="apibump"
    ;;
  macOS-ARM64)
    target="aarch64-apple-darwin"
    extension="tar.gz"
    binary="apibump"
    ;;
  Windows-X64)
    target="x86_64-pc-windows-msvc"
    extension="zip"
    binary="apibump.exe"
    ;;
  *)
    echo "Unsupported runner platform: $runner_os-$runner_arch" >&2
    exit 2
    ;;
esac

asset="apibump-${version}-${target}.${extension}"
url="https://github.com/${repo}/releases/download/${version}/${asset}"
download_dir="${RUNNER_TEMP:-/tmp}/apibump-download"
install_dir="${RUNNER_TEMP:-/tmp}/apibump-bin"

mkdir -p "$download_dir" "$install_dir"
archive="${download_dir}/${asset}"

echo "Downloading $url"
curl --fail --location --show-error --silent --output "$archive" "$url"

if [[ "$extension" == "zip" ]]; then
  python - "$archive" "$install_dir" <<'PY'
import sys
import zipfile

archive, install_dir = sys.argv[1:3]
with zipfile.ZipFile(archive) as zip_file:
    zip_file.extractall(install_dir)
PY
else
  tar -xzf "$archive" -C "$install_dir"
fi

chmod +x "$install_dir/$binary" 2>/dev/null || true
"$install_dir/$binary" --version

echo "$install_dir" >> "$GITHUB_PATH"

