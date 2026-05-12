# river-data-core

Shared library for the [river-data](https://github.com/RIVER-EPFL) platform: sync protocol, control plane types, and data toolbox.

- [river-data-api](https://github.com/RIVER-EPFL/river-data-api), [river-data-ui](https://github.com/RIVER-EPFL/river-data-ui)
- [river-data-vaisala](https://github.com/RIVER-EPFL/river-data-vaisala), [river-data-rshiny](https://github.com/RIVER-EPFL/river-data-rshiny)

## Features

- **`client`** — sync service HTTP client (reqwest)
- **`server`** — sync control plane handlers (axum, sea-orm)
- **`toolbox`** — RIVER lab data toolset

## Toolbox

Rust implementations of calculation functions originally written in R. Each function is tested against the original R output using 12,500+ bulk random cases to ensure numerical equivalency.

### Running tests

```bash
cargo test --features toolbox                        # all tests (unit + golden)
cargo test --features toolbox --test toolbox_golden   # golden value tests only
cargo test --features toolbox --lib                   # unit tests only
```

### Regenerating R fixtures

```bash
Rscript r_reference/generate_fixtures.R
```

Produces `tests/fixtures/golden_values.json`. Deterministic output via `set.seed(42)`. Requires R >= 4.0 and `jsonlite`.

### Verifying R reference integrity

```bash
Rscript r_reference/verify_integrity.R <path_to_calculation_functions.R>
```

Proves each file in `r_reference/functions/` is a byte-exact copy of the portal source.

### Adding a new function

To port an R function to Rust, add the original R code as a new file in `r_reference/functions/` with source attribution, add test cases to `r_reference/generate_fixtures.R`, and submit a PR to [@evanjt](https://github.com/evanjt).
