#!/usr/bin/env bash
# generate-site.sh — Copies content into site/src/ and generates SUMMARY.md
# Run from repo root: bash scripts/generate-site.sh
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SITE_SRC="$REPO_ROOT/site/src"

# --- Clean generated dirs (preserve hand-written files) ---
rm -rf "$SITE_SRC/devlog" "$SITE_SRC/reference/usd"
mkdir -p "$SITE_SRC/devlog" "$SITE_SRC/reference/usd"

# --- Copy devlog entries (preserve month dirs) ---
for month_dir in "$REPO_ROOT"/devlog/20*/; do
    month=$(basename "$month_dir")
    mkdir -p "$SITE_SRC/devlog/$month"
    cp "$month_dir"/*.md "$SITE_SRC/devlog/$month/" 2>/dev/null || true
done

# --- Copy UI mockup assets (for devlog image references) ---
if [ -d "$REPO_ROOT/assets/stitch_bif_ui" ]; then
    mkdir -p "$SITE_SRC/assets"
    cp -r "$REPO_ROOT/assets/stitch_bif_ui" "$SITE_SRC/assets/"
fi
if [ -d "$REPO_ROOT/assets/stitch_bif_ui_01" ]; then
    mkdir -p "$SITE_SRC/assets"
    cp -r "$REPO_ROOT/assets/stitch_bif_ui_01" "$SITE_SRC/assets/"
fi

# --- Copy USD reference docs ---
cp "$REPO_ROOT"/docs/usd/*.md "$SITE_SRC/reference/usd/"

# --- Copy changelog ---
cp "$REPO_ROOT/CHANGELOG.md" "$SITE_SRC/reference/changelog.md"

# --- Generate SUMMARY.md ---
SUMMARY="$SITE_SRC/SUMMARY.md"

cat > "$SUMMARY" << 'HEADER'
# Summary

[Introduction](introduction.md)

# Manual

- [Getting Started](reference/getting-started.md)
- [Architecture](reference/architecture.md)
- [USD Reference]()
HEADER

# USD docs (stable, hardcoded order)
cat >> "$SUMMARY" << 'USD'
  - [Core Concepts](reference/usd/concepts.md)
  - [Composition](reference/usd/composition.md)
  - [Geometry Schemas](reference/usd/schemas-geom.md)
  - [Shading Schemas](reference/usd/schemas-shade.md)
  - [Light Schemas](reference/usd/schemas-lux.md)
  - [SDF Foundations](reference/usd/sdf-foundations.md)
  - [Datatypes](reference/usd/datatypes.md)
  - [Preview Surface](reference/usd/preview-surface.md)
  - [Toolset](reference/usd/toolset.md)
  - [FAQ](reference/usd/faq.md)
USD

echo "- [Changelog](reference/changelog.md)" >> "$SUMMARY"
echo "" >> "$SUMMARY"
echo "# Dev Diary" >> "$SUMMARY"
echo "" >> "$SUMMARY"

# --- Generate devlog entries grouped by month, newest first ---

# Collect month dirs, sort reverse
months=()
for month_dir in "$SITE_SRC"/devlog/20*/; do
    [ -d "$month_dir" ] && months+=("$(basename "$month_dir")")
done
IFS=$'\n' sorted_months=($(printf '%s\n' "${months[@]}" | sort -r)); unset IFS

for month in "${sorted_months[@]}"; do
    # Format month name (e.g., "2026-03" -> "March 2026")
    month_names=("" "January" "February" "March" "April" "May" "June" "July" "August" "September" "October" "November" "December")
    year="${month:0:4}"
    mon_num="${month:5:2}"
    # Strip leading zero for array index
    mon_idx=$((10#$mon_num))
    month_label="${month_names[$mon_idx]} $year"

    echo "- [$month_label]()" >> "$SUMMARY"

    # Collect all .md files in this month, sort reverse by filename
    files=()
    for f in "$SITE_SRC/devlog/$month"/*.md; do
        [ -f "$f" ] && files+=("$f")
    done
    IFS=$'\n' sorted_files=($(printf '%s\n' "${files[@]}" | sort -r)); unset IFS

    for filepath in "${sorted_files[@]}"; do
        filename=$(basename "$filepath")
        relpath="devlog/$month/$filename"

        # Extract title from first "# " line
        title=$(grep -m1 '^# ' "$filepath" | sed 's/^# //' || echo "$filename")

        # For DEVLOG files, extract date from filename for cleaner label
        if [[ "$filename" =~ ^DEVLOG_([0-9]{4}-[0-9]{2}-[0-9]{2}) ]]; then
            date="${BASH_REMATCH[1]}"
            # If title is just "Development Log - DATE", shorten it
            if [[ "$title" == "Development Log"* ]]; then
                title="$date"
            else
                title="$date — $title"
            fi
        elif [[ "$filename" =~ ^CODE_REVIEW ]]; then
            # Keep code review title as-is, prefix with type
            if [[ "$title" == "Code Review"* ]] || [[ "$title" == "code review"* ]]; then
                : # title is fine
            else
                title="Code Review: $title"
            fi
        fi

        echo "  - [$title]($relpath)" >> "$SUMMARY"
    done
done

echo ""
echo "Site content generated:"
echo "  SUMMARY.md entries: $(grep -c '^\s*-' "$SUMMARY") items"
echo "  Devlog months: ${#sorted_months[@]}"
echo "  Devlog files: $(find "$SITE_SRC/devlog" -name '*.md' | wc -l)"
echo "  USD docs: $(ls "$SITE_SRC/reference/usd/"*.md 2>/dev/null | wc -l)"
