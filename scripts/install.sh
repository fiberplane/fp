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

binary_url="https://fp.dev/fp/latest/${target_os}_${target_arch}/fp"
curl --fail --show-error --location --progress-bar --output "${fiberplane_dir}/fp" "${binary_url}"

chmod +x "${fiberplane_dir}/fp"

shell=$(basename $SHELL)
case $shell in
  zsh) shell_profile="$HOME/.zshrc" ;;
  bash) shell_profile="$HOME/.bashrc" ;;
  *) ;;
esac
shell_completions="${fiberplane_dir}/${shell}_completions"

# Regenerate shell completions
if [ -n "$shell_completions" ]; then
  eval "${fiberplane_dir}/fp completions ${shell} > $shell_completions"
fi

# Add to PATH if it wasn't installed before
if $(fp --version > /dev/null); then
  echo "Successfully updated Fiberplane CLI"
  echo ""
  echo "Run fp --help to get started"
else
  if [ -n "$shell_profile" ]; then
    # Save a copy of the current shell profile
    cp $shell_profile "$shell_profile.bak" 2>/dev/null || true

    echo "" >> "$shell_profile"
    echo "# Fiberplane CLI (fp)" >> "$shell_profile"
    echo "export PATH=\"$fiberplane_dir:\$PATH\"" >> "$shell_profile"
    echo "source $shell_completions" >> "$shell_profile"

    echo "Fiberplane CLI (fp) successfully installed"
  else
    echo "Fiberplane CLI installed to ${fiberplane_dir}/fp"
    echo ""
    echo "Manually add ${fiberplane_dir} to your PATH:"
    echo "  export PATH=\"$fiberplane_dir:\$PATH\""
    echo "  source $shell_completions"
  fi
fi
