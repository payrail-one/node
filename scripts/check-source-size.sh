#!/usr/bin/env bash
set -euo pipefail

readonly max_lines=700
readonly project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
failed=0

while IFS= read -r -d '' source_file; do
  line_count="$(wc -l < "${source_file}")"
  if (( line_count > max_lines )); then
    relative_path="${source_file#"${project_root}/"}"
    echo "${relative_path}: ${line_count} lines (maximum ${max_lines})" >&2
    failed=1
  fi
done < <(
  find "${project_root}" -type f \
    \( -name '*.rs' -o -name '*.md' -o -name '*.toml' -o -name '*.sh' \) \
    -not -path "${project_root}/target/*" \
    -not -path "${project_root}/web/node_modules/*" \
    -not -path "${project_root}/web/apps/*/dist/*" \
    -print0
)

exit "${failed}"
