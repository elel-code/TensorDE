#!/usr/bin/env bash
set -euo pipefail

readonly max_lines="${MAX_FILE_LINES:-800}"
status=0

while IFS= read -r -d '' file; do
    lines="$(wc -l < "${file}")"
    if (( lines > max_lines )); then
        printf 'file exceeds %s-line limit: %s (%s lines)\n' "${max_lines}" "${file}" "${lines}" >&2
        status=1
    fi
done < <(find src scripts -type f \( -name '*.rs' -o -name '*.sh' \) -print0)

exit "${status}"

