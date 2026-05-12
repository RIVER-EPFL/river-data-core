# Source: cnet-data-portal (MIT, mclement18)
# File: app/utils/calculation_functions.R
# Lines: 564-605
#
# Verbatim copy — do not modify. See generate_fixtures.R for the
# standalone version used to produce golden test values.

calcChlaAcid <- function(df, pool, ...) {
  # Check for the presence of the correct columns
  allColumns <- sum(
    grepl(
      paste(
        c('lab_chla_fluor_1_rep',
          'lab_chla_fluor_2_rep',
          'chla_acid_std_curve_id'),
        collapse = '|'
      ),
      colnames(df)
    )
  ) == 3
  
  if (nrow(df) == 1 & allColumns) {
    # Get values
    lab_chla_fluor_1_rep <- df %>% select(starts_with('lab_chla_fluor_1_rep')) %>% pull()
    lab_chla_fluor_2_rep <- df %>% select(starts_with('lab_chla_fluor_2_rep')) %>% pull()
    
    # Get std curve values for correction
    stdCurveId <- df %>% pull('chla_acid_std_curve_id')
    if (!is.na(stdCurveId) & stdCurveId > 0) {
      stdCurve <- getRows(pool, 'standard_curves', id == stdCurveId, columns = c('a', 'b'))
      chla_acidified_slope <- stdCurve %>% pull('a')
      chla_acidified_intercept <- stdCurve %>% pull('b')
    } else {
      chla_acidified_slope <- NA
      chla_acidified_intercept <- NA
    }
    
    # If no NAs, calculate Chla acidified
    if (!any(is.na(c(lab_chla_fluor_1_rep, lab_chla_fluor_2_rep, chla_acidified_slope, chla_acidified_intercept)))) {
      return(
        (lab_chla_fluor_1_rep - lab_chla_fluor_2_rep) * chla_acidified_slope + chla_acidified_intercept
      )
    }
  }
  
  
  # If nothing is returned, return 'KEEP OLD'
  'KEEP OLD'
}
