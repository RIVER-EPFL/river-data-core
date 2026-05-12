# Source: cnet-data-portal (MIT, mclement18)
# File: app/utils/calculation_functions.R
# Lines: 14-31
#
# Verbatim copy — do not modify. See generate_fixtures.R for the
# standalone version used to produce golden test values.

calcMinus <- function(df, ...) {
  # Check that the df has only 2 columns 1 row
  if (ncol(df) == 2 & nrow(df) == 1) {
    # Get the two first clumns of the df
    col1 <- df %>% pull(1)
    col2 <- df %>% pull(2)

    # Check that they are number non NA
    values <- c(col1, col2)
    if (length(values) == 2 & !any(is.na(values)) & is.numeric(values)) {
      # Return the difference
      return(col1 - col2)
    }
  }

  # If nothing is returned, return NA
  as.numeric(NA)
}
