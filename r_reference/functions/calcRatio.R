# Source: cnet-data-portal (MIT, mclement18)
# File: app/utils/calculation_functions.R
# Lines: 266-282
#
# Verbatim copy — do not modify. See generate_fixtures.R for the
# standalone version used to produce golden test values.

calcRatio <- function(df, ...) {
  # Check for the presence of the correct columns
  if (nrow(df) == 1 & ncol(df) == 2) {
    dividend <- df %>% pull(1)
    divisor <- df %>% pull(2)
    
    # Check for presence of both dividend and divisor
    if (!any(is.na(c(dividend, divisor))) & divisor != 0) {
      return(
        dividend / divisor
      )
    }
  }
  
  # If nothing is returned, return NA
  as.numeric(NA)
}
