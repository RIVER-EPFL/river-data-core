# Source: cnet-data-portal (MIT, mclement18)
# File: app/utils/calculation_functions.R
# Lines: 191-193
#
# Verbatim copy — do not modify. See generate_fixtures.R for the
# standalone version used to produce golden test values.

calcDOCsd <- function(df, pool, ...) {
  calcDOC(df, calcSd, pool)
}
