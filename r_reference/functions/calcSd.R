# Source: cnet-data-portal (MIT, mclement18)
# File: app/utils/calculation_functions.R
# Lines: 50-61
#
# Verbatim copy — do not modify. See generate_fixtures.R for the
# standalone version used to produce golden test values.

calcSd <- function(df, ...) {
  # Check that df has only one row
  if (nrow(df) == 1) {
    # Calculate and return stdev
    stdev <- df %>% tidyr::pivot_longer(everything()) %>% pull(value) %>% sd(na.rm = TRUE)
    if (is.na(stdev)) stdev <- 'KEEP OLD'
    return(stdev)
  }
  
  # If nothing is returned, return NA
  as.numeric(NA)
}
