# R Reference Functions

Verbatim copies of the 26 calculation functions from `cnet-data-portal` (MIT, mclement18).

## Structure

- `functions/` — one `.R` file per function, exact copy from `app/utils/calculation_functions.R`
- `generate_fixtures.R` — standalone R script that produces golden test values

## Regenerating fixtures

```bash
Rscript r_reference/generate_fixtures.R
```

Writes `tests/fixtures/golden_values.json`. Rust tests in `tests/toolbox_golden.rs` compare
against these values.

## Requirements

- R >= 4.0
- `jsonlite` package (`install.packages("jsonlite")`)

## Source

Repository: [cnet-data-portal](https://github.com/mclement18/cnet-data-portal)
File: `app/utils/calculation_functions.R`
License: MIT
