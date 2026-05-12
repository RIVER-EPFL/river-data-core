# Source: cnet-data-portal (MIT, mclement18)
# File: app/utils/calculation_functions.R
# Lines: 455-465
#
# Verbatim copy — do not modify. See generate_fixtures.R for the
# standalone version used to produce golden test values.

convertToUnitPerM2 <- function(s, d, vf, vt) {
  # Convert sample value (s) to a sample unit/m2
  # Using rock dimensions (d) format: c(length , width, depth)
  # Sample volume (vt) and volume filtrated (vf)
  
  # Calculate area
  area <- 2 * pi * mean(combn((d / 100)^1.6075, 2, prod))^(1/1.6075)
  
  # Convert to unit/m2
  s * vt / (vf * area)
}
