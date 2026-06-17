#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
usage: packaging/build-dist.sh [options]

Build a distributable Gilder tarball containing binaries, man pages, shell
completions, and the systemd user service.

Options:
  --dest <dir>        Output directory. Default: dist
  --profile <name>   Cargo profile to package. Default: release
  --features <list>  Cargo feature list. Default: gtk-renderer,video-renderer
  --no-build         Do not run cargo build; package existing target artifacts
  -h, --help         Show this help text
EOF
}

dest_dir="dist"
profile="release"
features="${GILDER_DIST_FEATURES:-gtk-renderer,video-renderer}"
build=1

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dest)
      [[ $# -ge 2 ]] || { echo "--dest requires a directory" >&2; exit 2; }
      dest_dir="$2"
      shift 2
      ;;
    --profile)
      [[ $# -ge 2 ]] || { echo "--profile requires a value" >&2; exit 2; }
      profile="$2"
      shift 2
      ;;
    --features)
      [[ $# -ge 2 ]] || { echo "--features requires a value" >&2; exit 2; }
      features="$2"
      shift 2
      ;;
    --no-build)
      build=0
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown option: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

case "$profile" in
  release)
    cargo_profile_args=(--release)
    target_profile_dir="release"
    ;;
  debug)
    cargo_profile_args=()
    target_profile_dir="debug"
    ;;
  *)
    cargo_profile_args=(--profile "$profile")
    target_profile_dir="$profile"
    ;;
esac

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)"
arch="$(uname -m)"
system="$(uname -s | tr '[:upper:]' '[:lower:]')"
package_name="gilder-${version}-${system}-${arch}"
stage_dir="${dest_dir}/${package_name}"
archive_path="${dest_dir}/${package_name}.tar.gz"

if [[ "$build" -eq 1 ]]; then
  cargo build "${cargo_profile_args[@]}" --features "$features"
fi

rm -rf "$stage_dir"
mkdir -p \
  "$stage_dir/bin" \
  "$stage_dir/share/man/man1" \
  "$stage_dir/share/bash-completion/completions" \
  "$stage_dir/share/zsh/site-functions" \
  "$stage_dir/lib/systemd/user" \
  "$stage_dir/share/doc/gilder" \
  "$stage_dir/share/doc/gilder/scripts"

for binary in gilderd gilderctl gilder-convert; do
  source_path="target/${target_profile_dir}/${binary}"
  if [[ ! -x "$source_path" ]]; then
    echo "missing built binary: ${source_path}" >&2
    exit 1
  fi
  install -m 0755 "$source_path" "$stage_dir/bin/${binary}"
done

install -m 0644 docs/man/*.1 "$stage_dir/share/man/man1/"
install -m 0644 completions/bash/* "$stage_dir/share/bash-completion/completions/"
install -m 0644 completions/zsh/* "$stage_dir/share/zsh/site-functions/"
install -m 0644 packaging/systemd/gilder.service "$stage_dir/lib/systemd/user/gilder.service"
install -m 0644 README.md docs/packaging.md docs/todo.md docs/video-validation.md "$stage_dir/share/doc/gilder/"
install -m 0755 scripts/video-codec-smoke.sh "$stage_dir/share/doc/gilder/scripts/video-codec-smoke.sh"
install -m 0755 scripts/wayland-video-surface-smoke.sh "$stage_dir/share/doc/gilder/scripts/wayland-video-surface-smoke.sh"
install -m 0755 scripts/performance-snapshot.sh "$stage_dir/share/doc/gilder/scripts/performance-snapshot.sh"

cat > "$stage_dir/MANIFEST.txt" <<EOF
name: gilder
version: ${version}
profile: ${profile}
features: ${features}
system: ${system}
arch: ${arch}
contents:
  bin/gilderd
  bin/gilderctl
  bin/gilder-convert
  share/man/man1/gilderd.1
  share/man/man1/gilderctl.1
  share/man/man1/gilder-convert.1
  share/bash-completion/completions/gilderctl
  share/bash-completion/completions/gilder-convert
  share/zsh/site-functions/_gilderctl
  share/zsh/site-functions/_gilder-convert
  lib/systemd/user/gilder.service
  share/doc/gilder/README.md
  share/doc/gilder/packaging.md
  share/doc/gilder/todo.md
  share/doc/gilder/video-validation.md
  share/doc/gilder/scripts/video-codec-smoke.sh
  share/doc/gilder/scripts/wayland-video-surface-smoke.sh
  share/doc/gilder/scripts/performance-snapshot.sh
EOF

mkdir -p "$dest_dir"
tar -C "$dest_dir" -czf "$archive_path" "$package_name"

echo "staged ${stage_dir}"
echo "archive ${archive_path}"
