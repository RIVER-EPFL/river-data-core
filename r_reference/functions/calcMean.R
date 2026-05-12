# Source: cnet-data-portal (MIT, mclement18)
# File: app/utils/calculation_functions.R
# Lines: 35-46
#
# Verbatim copy — do not modify. See generate_fixtures.R for the
# standalone version used to produce golden test values.

calcMean <- function(df, ...) {
  # Check that df has only one row
  if (nrow(df) == 1) {
    # Calculate and return mean
    avg <- df %>% tidyr::pivot_longer(everything()) %>% pull(value) %>% mean(na.rm = TRUE)
    if (is.na(avg)) avg <- 'KEEP OLD'
    return(avg)
  }
  
  # If nothing is returned, return NA
  as.numeric(NA)
}
