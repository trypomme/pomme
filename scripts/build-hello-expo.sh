#!/usr/bin/env bash
# Build the hello-expo test app. Usage: scripts/build-hello-expo.sh [device|simulator]
exec "$(dirname "$0")/build-app.sh" hello-expo "${1:-device}"
