#!/usr/bin/env bash

PKG="${E3_SUPPORT_IMAGE_REPOSITORY:-ghcr.io/theinterfold/e3-support}:next"

docker run -it \
  -v "$(pwd)/app:/app/app" \
  -v "$(pwd)/host:/app/host" \
  -v "$(pwd)/methods:/app/methods" \
  -v "$(pwd)/types:/app/types" \
  -v "$(pwd)/program:/app/program" \
  -v "$(pwd)/scripts:/app/scripts" \
  -v "$(pwd)/.interfold/generated/contracts:/app/contracts" \
  -v "$(pwd)/.interfold/generated/tests:/app/tests" \
  -v "$(pwd)/Cargo.toml:/app/Cargo.toml" \
  -v "$(pwd)/Cargo.lock:/app/Cargo.lock" \
  "$PKG"
