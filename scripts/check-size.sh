#!/usr/bin/env bash

set -e

git diff -w origin/$1...HEAD -- . ':!*.lock' ':!*lock.yaml' ':!*lock.json' | \
  grep "^[+-]" | \
  grep -v "^[+-][+-][+-]" | \
  grep -v "^[+-]@@" | \
  grep -v "^[+-]\s*$" | \
  grep -v "^[+-]\s*\(//\|#\|\*\|/\*\|\*/\)" | \
  wc -l
