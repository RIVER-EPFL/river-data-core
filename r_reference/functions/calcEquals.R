# Source: cnet-data-portal (MIT, mclement18)
# File: app/utils/calculation_functions.R
# Lines: 64-82
#
# Verbatim copy — do not modify. See generate_fixtures.R for the
# standalone version used to produce golden test values.

calcEquals <- function(df, ...) {
  # Check that df has only one row and one column
  if (nrow(df) == 1 & ncol(df) == 2) {
    # Get the two first clumns of the df
    col1 <- df %>% pull(1)
    col2 <- df %>% pull(2)
    
    # If col1 is NA, set it to col2
    if (is.na(col1)) {
      # Return the difference
      return(col2)
    } else {
      return(col1)
    }
  }
  
  # If nothing is returned, return NA
  as.numeric(NA)
}
