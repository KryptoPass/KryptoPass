#!/usr/bin/env bash
# Extracts the user-visible strings into $podir/<app id>.pot, the KDE way: KDE's
# translation tooling (scripty) runs this file with $XGETTEXT and $podir set, and
# `just i18n-extract` sets them the same way locally (tools/i18n.py).
#
# Only explicit KI18n calls are extracted: i18n(), i18nc(), i18np(), i18ncp() and
# their k/x variants, in C++, QML (also KI18n.i18n()) and Rust.
set -euo pipefail

domain=$(python3 -c 'import sys; sys.path.insert(0, "packaging"); from metadata import load_metadata; print(load_metadata()["app-id"])')

# --force-po writes the template even before any string is marked.
# shellcheck disable=SC2046 # one argument per source file
$XGETTEXT --force-po $(find "src/native" "ui/qml" \( -name '*.cpp' -o -name '*.h' -o -name '*.qml' \) | sort) \
    -o "$podir/$domain.pot"

# The application description lives in Cargo.toml; extract it with the context that
# desktop_integration.cpp translates it with.
snippet=$(mktemp --suffix=.cpp)
trap 'rm -f "$snippet"' EXIT
python3 -c 'import json, sys; sys.path.insert(0, "packaging"); from metadata import load_metadata; print("i18nc(\"@info application description\", " + json.dumps(load_metadata()["description"]) + ");")' > "$snippet"
$XGETTEXT --join-existing --no-location "$snippet" -o "$podir/$domain.pot"

# Same keywords; -L Rust overrides the C++ language that $XGETTEXT selects.
# KI18n placeholders (%1) are the same in Rust, so msgfmt checks them too.
# Besides i18n() calls, the Rust code marks the texts it hands to the interface with
# tr(), trf(), trp() and tr_noop() (crate::i18n); the backend in the sibling
# kryptopass-core repository uses the same names.
core=../kryptopass-core/crates
# shellcheck disable=SC2046
$XGETTEXT --join-existing -L Rust \
    --flag=i18n:1:kde-format --flag=i18nc:2:kde-format \
    --flag=i18np:1:kde-format --flag=i18np:2:kde-format \
    --flag=i18ncp:2:kde-format --flag=i18ncp:3:kde-format \
    -ktr:1 -ktrf:1 -ktr_noop:1 -ktrp:1,2 \
    --flag=tr:1:kde-format --flag=trf:1:kde-format --flag=tr_noop:1:kde-format \
    --flag=trp:1:kde-format --flag=trp:2:kde-format \
    $(find "src" "$core"/*/src -name '*.rs' -not -name '*_tests.rs' | sort) \
    -o "$podir/$domain.pot"
