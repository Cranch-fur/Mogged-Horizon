#!/bin/sh
# Packs the mod into target/mogged-<version>.hnmod (needs Python 3.11+).
# Horizon's launcher compiles it into the game: put the .hnmod in <instance>/horizon-native-mods/ and press Build.
exec python3 "$(dirname "$0")/tools/package.py" "$@"
