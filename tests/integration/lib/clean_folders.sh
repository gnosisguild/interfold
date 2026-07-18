#!/usr/bin/env bash
clean_folders() {
    local SCRIPT_DIR=$1

    # Delete output artifacts
    rm -rf "$SCRIPT_DIR/output/"*

    # Reset per-run node state without deleting the source-aligned Noir
    # artifacts staged by prebuild.sh. Removing the whole .interfold directory
    # makes `interfold noir setup` download the released circuit bundle, which
    # can have an older ABI than the Rust witness code under test.
    rm -rf \
        "$SCRIPT_DIR/.interfold/config" \
        "$SCRIPT_DIR/.interfold/data" \
        "$SCRIPT_DIR/.interfold/noir/work"
}

clean_folders $1
