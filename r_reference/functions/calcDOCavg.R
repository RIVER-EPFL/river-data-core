# Source: cnet-data-portal (MIT, mclement18)
# File: app/utils/calculation_functions.R
# Lines: 185-187
#
# Verbatim copy — do not modify. See generate_fixtures.R for the
# standalone version used to produce golden test values.

calcDOCavg <- function(df, pool, ...) {
  calcDOC(df, calcMean, pool)
}
