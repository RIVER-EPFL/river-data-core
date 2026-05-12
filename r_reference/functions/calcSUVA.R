# Source: cnet-data-portal (MIT, mclement18)
# File: app/utils/calculation_functions.R
# Lines: 245-261
#
# Verbatim copy — do not modify. See generate_fixtures.R for the
# standalone version used to produce golden test values.

calcSUVA <- function(df, ...) {
  # Check for the presence of the correct columns
  if (nrow(df) == 1 & all(c('a254', 'DOC_avg_ppb') %in% colnames(df))) {
    a254 <- df %>% pull('a254')
    DOC_avg <- df %>% pull('DOC_avg_ppb')
    
    # Check for presence of DOC_avg and a254
    if (!any(is.na(c(a254, DOC_avg)))) {
      return(
        a254 * 1000 / DOC_avg
      )
    }
  }
  
  # If nothing is returned, return NA
  as.numeric(NA)
}
