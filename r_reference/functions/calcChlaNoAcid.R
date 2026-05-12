# Source: cnet-data-portal (MIT, mclement18)
# File: app/utils/calculation_functions.R
# Lines: 610-649
#
# Verbatim copy — do not modify. See generate_fixtures.R for the
# standalone version used to produce golden test values.

calcChlaNoAcid <- function(df, pool, ...) {
  # Check for the presence of the correct columns
  allColumns <- sum(
    grepl(
      paste(
        c('lab_chla_fluor_1_rep',
          'chla_noacid_std_curve_id'),
        collapse = '|'
      ),
      colnames(df)
    )
  ) == 2

  if (nrow(df) == 1 & allColumns) {
    # Get values
    lab_chla_fluor_1_rep <- df %>% select(starts_with('lab_chla_fluor_1_rep')) %>% pull()

    # Get std curve values for correction
    stdCurveId <- df %>% pull('chla_noacid_std_curve_id')
    if (!is.na(stdCurveId) & stdCurveId > 0) {
      stdCurve <- getRows(pool, 'standard_curves', id == stdCurveId, columns = c('a', 'b'))
      chla_non_acidified_slope <- stdCurve %>% pull('a')
      chla_non_acidified_intercept <- stdCurve %>% pull('b')
    } else {
      chla_non_acidified_slope <- NA
      chla_non_acidified_intercept <- NA
    }

    # If no NAs, calculate Chla non acidified
    if (!any(is.na(c(lab_chla_fluor_1_rep, chla_non_acidified_slope, chla_non_acidified_intercept)))) {
      return(
        lab_chla_fluor_1_rep * chla_non_acidified_slope + chla_non_acidified_intercept
      )
    }
  }


  # If nothing is returned, return 'KEEP OLD'
  'KEEP OLD'
}
