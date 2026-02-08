#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

usage() {
    echo "Usage: ./release.sh <patch|minor|major|VERSION>"
    echo ""
    echo "Examples:"
    echo "  ./release.sh patch          # 0.2.7 -> 0.2.8"
    echo "  ./release.sh minor          # 0.2.7 -> 0.3.0"
    echo "  ./release.sh major          # 0.2.7 -> 1.0.0"
    echo "  ./release.sh 0.3.0          # set exact version"
    exit 1
}

[[ $# -eq 1 ]] || usage

current_version=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')
echo "Current version: $current_version"

bump_arg="$1"
IFS='.' read -r major minor patch <<< "$current_version"

case "$bump_arg" in
    patch) new_version="$major.$minor.$((patch + 1))" ;;
    minor) new_version="$major.$((minor + 1)).0" ;;
    major) new_version="$((major + 1)).0.0" ;;
    [0-9]*)
        if [[ ! "$bump_arg" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
            echo "Error: invalid version format '$bump_arg'. Expected X.Y.Z"
            exit 1
        fi
        new_version="$bump_arg"
        ;;
    *) usage ;;
esac

if [[ "$new_version" == "$current_version" ]]; then
    echo "Error: new version is the same as current ($current_version)"
    exit 1
fi

echo "New version: $new_version"
echo ""

if ! git diff --quiet || ! git diff --cached --quiet; then
    echo "Error: working tree is dirty. Commit or stash changes first."
    exit 1
fi

branch=$(git rev-parse --abbrev-ref HEAD)
if [[ "$branch" != "master" && "$branch" != "main" ]]; then
    echo "Warning: releasing from branch '$branch', not master/main"
    read -rp "Continue? [y/N] " confirm
    [[ "$confirm" =~ ^[Yy]$ ]] || exit 1
fi

echo "==> Bumping version in Cargo.toml"
sed -i "0,/^version = \"$current_version\"/s//version = \"$new_version\"/" Cargo.toml

echo "==> Running checks"
cargo fmt --check
cargo clippy -- -D warnings
cargo test

echo "==> Building release (CPU)"
touch build.rs
cargo build --release
mkdir -p release
cp target/release/muesli "release/muesli-linux-x86_64-cpu"

echo "==> Building release (Vulkan)"
cargo build --release --features vulkan
cp target/release/muesli "release/muesli-linux-x86_64-vulkan"

echo "==> Generating checksums"
cd release
for f in muesli-*; do
    sha256sum "$f" > "$f.sha256"
done
cd ..

echo "==> Committing version bump"
git add Cargo.toml
git commit -m "v$new_version"

echo "==> Tagging v$new_version"
git tag "v$new_version"

echo "==> Pushing to origin"
git push origin "$branch" --tags

echo "==> Creating GitHub release"
gh release create "v$new_version" \
    --title "v$new_version" \
    --generate-notes \
    release/muesli-linux-x86_64-cpu \
    release/muesli-linux-x86_64-cpu.sha256 \
    release/muesli-linux-x86_64-vulkan \
    release/muesli-linux-x86_64-vulkan.sha256

echo "==> Installing locally"
pkill -f "muesli daemon" 2>/dev/null || true
sleep 1
cp release/muesli-linux-x86_64-cpu ~/.local/bin/muesli
echo ""
echo "Installed: $(muesli --version)"

echo "==> Cleaning up"
rm -rf release

echo ""
echo "Released v$new_version"
echo "https://github.com/itsameandrea/muesli/releases/tag/v$new_version"
