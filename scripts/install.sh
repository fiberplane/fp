#!/bin/sh

set -e

# Get the target OS and architecture
case $(uname -s) in
Darwin) target_os="apple-darwin" ;;
Linux) target_os="unknown-linux-gnu" ;;
*) echo "Unsupported OS: $(uname -s)"; exit 1 ;;
esac

case $(uname -m) in
arm64) target_arch="aarch64" ;;
x86_64) target_arch="x86_64" ;;
*) echo "Unsupported architecture: $(uname -m)"; exit 1 ;;
esac

# Setup any variables

## FP_DIR changes where the binary is installed and optionally the completions
if [ -z "${FP_DIR}" ]; then
  FP_DIR="$HOME/.fiberplane"
fi

## FP_INSTALL_COMPLETIONS enables installing completions
if [ -z "${FP_INSTALL_COMPLETIONS}" ]; then
  FP_INSTALL_COMPLETIONS="true"
fi

## FP_UPDATE_RC enables updating the shell rc file (with PATH update and completions)
if [ -z "${FP_UPDATE_RC}" ]; then
  FP_UPDATE_RC="true"
fi

if [ ! -d "$FP_DIR" ]; then
  mkdir -p "$FP_DIR"
fi

echo "Installing Fiberplane CLI to $FP_DIR/fp ..."

binary_url="https://fp.dev/fp/latest/${target_arch}-${target_os}/fp"
curl --fail --show-error --location --progress-bar --output "${FP_DIR}/fp" "${binary_url}"

chmod +x "${FP_DIR}/fp"

shell=$(basename "$SHELL")
case $shell in
  zsh) shell_profile="$HOME/.zshrc" ;;
  bash) shell_profile="$HOME/.bashrc" ;;
  *) ;;
esac
shell_completions="${FP_DIR}/${shell}_completions"

if [ "$FP_INSTALL_COMPLETIONS" = "true" ]; then
  # Regenerate shell completions
  if [ -n "$shell_completions" ]; then
    eval "${FP_DIR}/fp completions ${shell} > $shell_completions"
  fi
fi

if [ "$FP_UPDATE_RC" = "true" ]; then
  if command -v fp > /dev/null 2>&1; then
    echo "Fiberplane CLI is already available in your PATH, skipping updating the shell rc file"
  else
    if [ -n "$shell_profile" ]; then
      # Save a copy of the current shell profile
      cp $shell_profile "$shell_profile.bak" 2>/dev/null || true

      echo "" >> "$shell_profile"
      echo "# Fiberplane CLI (fp)" >> "$shell_profile"
      echo "export PATH=\"$FP_DIR:\$PATH\"" >> "$shell_profile"

      if [ "$FP_INSTALL_COMPLETIONS" = "true" ]; then
        echo "source $shell_completions" >> "$shell_profile"
      fi

      #source "$shell_profile"

      echo "Fiberplane CLI (fp) successfully installed. Run 'fp help' to see available commands."
      exit 0
    fi
  fi
else
  echo "Fiberplane CLI installed to ${FP_DIR}/fp"
  echo ""
  echo "Add ${FP_DIR} to your PATH:"
  echo "  export PATH=\"$FP_DIR:\$PATH\""

  if [ "$FP_INSTALL_COMPLETIONS" = "true" ]; then
    echo "Source $shell_completions in your shell's rc file:"
    echo "  source $shell_completions"
  fi
fi
