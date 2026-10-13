#!/usr/bin/env python3
"""Generate `locales/<code>.json` from `locales/translations.tsv`.

The TSV is a single source of truth for every shipped translation:

    key <TAB> es <TAB> fr <TAB> de ...

`locales/en.json` is the English source of truth for the *keys*; any key missing
from a translation column falls back to the English text (and at runtime, any
key missing from a locale's JSON falls back to English too).

Run:  python3 scripts/gen-locales.py
"""

import csv
import json
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
LOCALES = ROOT / "locales"
SOURCE = LOCALES / "translations.tsv"
ENGLISH = LOCALES / "en.json"


def main() -> int:
    english = json.loads(ENGLISH.read_text(encoding="utf-8"))
    keys = list(english.keys())

    with SOURCE.open(encoding="utf-8", newline="") as handle:
        rows = list(csv.reader(handle, delimiter="\t"))

    if not rows:
        print("translations.tsv is empty", file=sys.stderr)
        return 1

    header, *body = rows
    languages = header[1:]
    table = {row[0]: row[1:] for row in body if row}

    written = []
    for index, code in enumerate(languages):
        catalog = {}
        for key in keys:
            values = table.get(key, [])
            translated = values[index].strip() if index < len(values) else ""
            catalog[key] = translated or english[key]
        path = LOCALES / f"{code}.json"
        path.write_text(
            json.dumps(catalog, ensure_ascii=False, indent=2) + "\n",
            encoding="utf-8",
        )
        written.append(code)

    missing = [key for key in keys if key not in table]
    if missing:
        print(f"warning: {len(missing)} key(s) not in the TSV: {missing[:5]}…")

    print(f"wrote {len(written)} locales: {', '.join(written)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
