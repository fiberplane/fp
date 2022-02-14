#!/bin/sh

set -e

# Get the target OS and architecture
case $(uname -s) in
Darwin) target_os="macos" ;;
*) target_os="linux" ;;
esac

case $(uname -m) in
aarch64) target_arch="aarch64" ;;
*) target_arch="x86_64" ;;
esac

fiberplane_dir="$HOME/.fiberplane"
if [ ! -d "$fiberplane_dir" ]; then
  mkdir -p "$fiberplane_dir"
fi

curl --fail --show-error --location --progress-bar --output "${fiberplane_dir}/fp" "https://fp.dev/fp/latest/${target_os}_${target_arch}/fp"

chmod +x "${fiberplane_dir}/fp"

echo "Fiberplane CLI installed to ${fiberplane_dir}/fp"

# Add to PATH
case $SHELL in
  *zsh) shell_profile="$HOME/.zshrc" ;;
  *bash) shell_profile="$HOME/.bashrc" ;;
  *)
    echo "Unknown shell. Please add ${fiberplane_dir} to your PATH manually."
    shell_profile="" ;;
esac
if [ -n "$shell_profile" ]; then
  echo "" >> "$shell_profile"
  echo "export PATH=\"\$PATH:$fiberplane_dir\"" >> "$shell_profile"
  echo "Fiberplane CLI (fp) added to PATH in $shell_profile"
  source "$shell_profile"


  # Add shell completions
  eval $(fp completions $(basename $SHELL))
fi
