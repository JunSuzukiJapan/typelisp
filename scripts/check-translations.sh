#!/usr/bin/env bash
# Reports translations that have fallen behind the Japanese documents they
# were made from. The Japanese text is the original: README_JP.md for
# README.md, docs/ja/<path> for docs/<language>/<path> in every other
# language directory under docs/ (docs/dev/ is not translated), and each
# editor's editor/<editor>/README_JP.md for its README.md (English) and
# README_<language>.md (every other language docs/ has).
#
# Every translation starts with a line naming its source and the commit it was
# translated from:
#
#   <!-- translated-from: docs/ja/tutorial/intro.md @ <full commit hash> -->
#
# A translation is stale when its source differs between that commit and the
# working tree. After bringing a translation up to date, put the current commit
# in its first line.
#
# Usage:
#   scripts/check-translations.sh
#
# One line per problem — MISSING, ORPHAN, NOHEADER, WRONGSOURCE, BADCOMMIT or
# STALE, followed by the file — and the script fails if there is any.
set -uo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

problems=0
checked=0

report() {
    echo "$1 $2"
    problems=$((problems + 1))
}

# check <translation> <source>
check() {
    local translation="$1" source="$2" header expected_source commit
    checked=$((checked + 1))
    header="$(head -n 1 "$translation")"
    if ! [[ "$header" =~ ^\<!--\ translated-from:\ ([^ ]+)\ @\ ([0-9a-f]{40})\ --\>$ ]]; then
        report NOHEADER "$translation"
        return
    fi
    expected_source="${BASH_REMATCH[1]}"
    commit="${BASH_REMATCH[2]}"
    if [ "$expected_source" != "$source" ]; then
        report WRONGSOURCE "$translation (names $expected_source, should be $source)"
        return
    fi
    if ! git cat-file -e "$commit^{commit}" 2>/dev/null; then
        report BADCOMMIT "$translation ($commit)"
        return
    fi
    if ! git diff --quiet "$commit" -- "$source"; then
        report STALE "$translation ($source changed since ${commit:0:7})"
    fi
}

if [ -f README.md ]; then
    check README.md README_JP.md
else
    report MISSING README.md
fi

for dir in docs/*/; do
    lang="$(basename "$dir")"
    case "$lang" in ja|dev) continue ;; esac

    while IFS= read -r source; do
        translation="docs/$lang/${source#docs/ja/}"
        if [ -f "$translation" ]; then
            check "$translation" "$source"
        else
            report MISSING "$translation"
        fi
    done < <(find docs/ja -name '*.md' | sort)

    while IFS= read -r translation; do
        source="docs/ja/${translation#docs/$lang/}"
        [ -f "$source" ] || report ORPHAN "$translation"
    done < <(find "docs/$lang" -name '*.md' | sort)
done

for source in editor/*/README_JP.md; do
    [ -f "$source" ] || continue
    editor_dir="$(dirname "$source")"
    if [ -f "$editor_dir/README.md" ]; then
        check "$editor_dir/README.md" "$source"
    else
        report MISSING "$editor_dir/README.md"
    fi
    for dir in docs/*/; do
        lang="$(basename "$dir")"
        case "$lang" in ja|dev|en) continue ;; esac
        translation="$editor_dir/README_$lang.md"
        if [ -f "$translation" ]; then
            check "$translation" "$source"
        else
            report MISSING "$translation"
        fi
    done
    while IFS= read -r translation; do
        lang="${translation#"$editor_dir/README_"}"
        lang="${lang%.md}"
        case "$lang" in JP) continue ;; esac
        if [ "$lang" = en ] || [ "$lang" = ja ] || [ "$lang" = dev ] || [ ! -d "docs/$lang" ]; then
            report ORPHAN "$translation"
        fi
    done < <(find "$editor_dir" -maxdepth 1 -name 'README_*.md' | sort)
done

if [ "$problems" -gt 0 ]; then
    echo "$problems problem(s) in $checked translation(s)"
    exit 1
fi
echo "all $checked translations are up to date"
