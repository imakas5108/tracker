# CSV export

The native **Export CSV…** panel offers two profiles, a decimal separator,
precision and an optional provenance section. Both use the same command and
motion pipeline as MCP. Formatting and atomic file writes run off the main actor.

## Excel CSV

The native panel defaults to Excel CSV, the current regional decimal separator,
full precision and eight columns:

`t (s)`, `x (unit)`, `y (unit)`, `vx (unit/s)`, `vy (unit/s)`, `v (unit/s)`,
`ax (unit/s²)`, `ay (unit/s²)`.

Decimal commas use semicolon-delimited fields; decimal points use commas. A UTF-8
BOM preserves Unicode units, a `sep=` line states the delimiter, and rows use
CRLF. Missing derivatives are blank. Enable provenance to append source type,
score, missing reason, derivative method/window, axes and exact source stamps.
Choose six, nine or twelve decimal places for a shorter explicitly rounded
export. Project observations and calculated rows retain full precision.

This is plain text, so column widths, number formats, frozen headers, charts and
styled Excel tables are not stored. It is not an `.xlsx` workbook. Excel can
apply its own numeric precision and display rules; retain the project and
Standard CSV for reproducible numerical work. The `sep=` directive is intended
for Excel and is not a scientific data row.

## Standard CSV

MCP and direct command callers default to Standard CSV for compatibility:
comma delimiter, decimal points, full precision, LF, no BOM/directive and the
original 18-column order with provenance, followed by `tracking_method` for
newly recorded scores (blank for manual or unknown legacy observations).
All text fields are quoted when they
contain the delimiter, quotes or line breaks. Nonfinite numerical values fail
clearly rather than writing invalid measurements. Rounding and omission of
provenance are explicit options; decimal commas are rejected for this profile.

Native save dialogs retain their overwrite confirmation. MCP uses explicit
absolute paths and refuses an existing destination unless `overwrite=true`.
See [MCP options](mcp.md) for the additive command arguments.

## Executed verification — 15 November 2026

- The supplied Excel screenshot was confirmed through accessibility: the original
  export occupied **A1:A73**, with comma-separated records in one column.
- Tests cover Indonesian, German and US separator choices, Unicode/BOM/directive,
  full precision, explicit rounding, quoting, blank derivatives and rejected
  combinations. The complete release suite passed: 35 XCTest cases and 17 Swift
  Testing definitions (including parameterized cases), with no fixture skips.
- The packaged MCP bridge advertises the new options. The motion acceptance
  script passed 17 checks, including Standard CSV/MCP numerical equality,
  decimal-comma rounded CSV, optional provenance, invalid-option rejection and
  unchanged scientific rows after export.
- Opened the exact app-generated
  `build/csv-verification/ball-toss-excel-6dp.csv` in Microsoft Excel on the local
  Indonesian configuration. Accessibility reported **A1:H73**, header `t (s)`,
  and numeric Sum **1273,63975** / Average **2,332673535** when the used content
  was selected. This verifies multiple columns and numeric recognition on this
  installation; other Excel versions/regional settings are not device-tested.
- Evidence: `build/csv-verification/excel-generated-ax.txt` and
  `excel-numeric-ax.txt`; tests: `/tmp/tracker-final-tests2.log`,
  `/tmp/tracker-final-motion-mcp.log`. The original user workbook was not saved
  or edited. The earlier ambiguous chooser navigation was rejected by automatic
  approval review; opening the exact generated test file succeeded.

The native accessory was opened through Open Computer Use. Its profile, regional
separator, full-precision choice and provenance switch were exposed to
accessibility. A profile-switch mismatch found during this check was fixed by
updating the preferences together, and a regression check covers both profile
transitions. Final native verification showed Standard CSV with decimal points
and provenance on; the dialog was cancelled without writing. Evidence:
`build/csv-verification/native-standard-final-ax.txt`. Screen
capture remains unavailable, so pixel-level layout, contrast and truncation are
not fully verified by these accessibility checks.

Microsoft documents [regional delimiter/import behavior](https://support.microsoft.com/en-us/excel/get-started/import-or-export-text-txt-or-csv-files)
and [UTF-8 BOM handling](https://support.microsoft.com/en-gb/excel/opening-csv-utf-8-files-correctly-in-excel).
The specific `sep=` result above is a local observation.
