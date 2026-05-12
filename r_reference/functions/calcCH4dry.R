# Source: cnet-data-portal (MIT, mclement18)
# File: app/utils/calculation_functions.R
# Lines: 654-683
#
# Verbatim copy — do not modify. See generate_fixtures.R for the
# standalone version used to produce golden test values.

calcCH4dry <- function(df, ...) {
  # Check for the presence of the correct columns
  allColumns <- sum(
    grepl(
      paste(
        c('lab_co2(air)?_h2o',
          'lab_co2(air)?_ch4'),
        collapse = '|'
      ),
      colnames(df)
    )
  ) == 2
  
  if (nrow(df) == 1 & allColumns) {
    # Get values
    lab_co2_h2o <- df %>% select(matches('lab_co2(air)?_h2o')) %>% pull()
    lab_co2_ch4 <- df %>% select(matches('lab_co2(air)?_ch4')) %>% pull()
    
    # If no NAs, calculate Chla acidified
    if (!any(is.na(c(lab_co2_h2o, lab_co2_ch4)))) {
      return(
        (lab_co2_h2o * 1.2347 - 0.0016) * lab_co2_ch4 / 100 + lab_co2_ch4
      )
    }
  }
  
  
  # If nothing is returned, return NA
  as.numeric(NA)
}
