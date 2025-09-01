#!/bin/bash

# Sourced from: https://github.com/cecton/cargo-fixup/tree/main
# MIT License

# Copyright (c) 2025 Cecile Tonglet

# Permission is hereby granted, free of charge, to any person obtaining a copy
# of this software and associated documentation files (the "Software"), to deal
# in the Software without restriction, including without limitation the rights
# to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
# copies of the Software, and to permit persons to whom the Software is
# furnished to do so, subject to the following conditions:

# The above copyright notice and this permission notice shall be included in all
# copies or substantial portions of the Software.

# THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
# IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
# FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
# AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
# LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
# OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
# SOFTWARE.

set -eu

# --- Constants and Paths ---
SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")/../" && pwd)
PATCHES_DIR="$SCRIPT_DIR/patches"
TARGET_PATCHED_DIR="$SCRIPT_DIR/target/patched-crates"

# --- No patching needed - run original args ---
if [ -z "${CARGO_PKG_NAME:-}" ] || [ -z "${CARGO_MANIFEST_DIR:-}" ]; then
  exec "$@"
fi

ORIGINAL_DIR_NAME=$(basename "$CARGO_MANIFEST_DIR")
PATCH_DIR="$PATCHES_DIR/$ORIGINAL_DIR_NAME"

# --- Check for matching patch directory ---
if [ -d "$PATCH_DIR" ]; then
  # Start logging commands from now on, they are related to patching
  LOG_FILE="/tmp/cargo-rustc-patch-$(date --utc +"%Y-%m-%dT%H:%M:%SZ").log"
  exec 3>&1 4>&2
  exec >>"$LOG_FILE" 2>&1

  PATCHED_SRC="$TARGET_PATCHED_DIR/$ORIGINAL_DIR_NAME"

  echo "Applying patches to $CARGO_PKG_NAME..."

  mkdir -p "$TARGET_PATCHED_DIR"
  rm -rf -- "$PATCHED_SRC"
  cp -RL -- "$CARGO_MANIFEST_DIR" "$PATCHED_SRC"

  for PATCH_FILE in "$PATCH_DIR"/*; do
    [ -f "$PATCH_FILE" ] || continue
    if [ -x "$PATCH_FILE" ]; then
      echo "Executing: $PATCH_FILE"
      (cd "$PATCHED_SRC" && "$PATCH_FILE")
    elif [ "${PATCH_FILE##*.}" = "patch" ]; then
      echo "Applying patch: $PATCH_FILE"
      patch -s -p1 -d "$PATCHED_SRC" < "$PATCH_FILE"
    else
      echo "Not executable nor patch file: $PATCH_FILE"
    fi
  done

  # Update arguments to use patched source directory
  new_args=()
  for arg in "$@"; do
    new_args+=("${arg//$CARGO_MANIFEST_DIR/$PATCHED_SRC}")
  done

  exec "${new_args[@]}"
else
  exec "$@"
fi

