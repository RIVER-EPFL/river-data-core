# Source: cnet-data-portal (MIT, mclement18)
# File: app/utils/calculation_functions.R
# Lines: 110-145
#
# Verbatim copy — do not modify. See generate_fixtures.R for the
# standalone version used to produce golden test values.

calcCO2corr <- function(df, pool, ...) {
  # Check for the presence of the correct columns
  if (nrow(df) == 1 & ncol(df) == 5 & all(c('WTW_Temp_degC_1', 'Field_BP', 'Field_BP_altitude', 'vaisala_std_curve_id') %in% colnames(df))) {
    rawCO2 <- df %>% pull(1)
    temp <- df %>% pull('WTW_Temp_degC_1')
    fieldPressure <- df %>% pull('Field_BP')
    altPressure <- df %>% pull('Field_BP_altitude')
    stdCurveId <- df %>% pull('vaisala_std_curve_id')

    # Correct values if there is a std curve id
    if (!is.na(stdCurveId) & stdCurveId  > 0) {
      stdCurve <- getRows(pool, 'standard_curves', id == stdCurveId)
      rawCO2 <- rawCO2 * stdCurve$a + stdCurve$b
    }

    # If there is a temp
    if (!is.na(temp)) {
      # And that the fieldPressure is present and within the range
      if (!is.na(fieldPressure) & fieldPressure <= 1050 & fieldPressure >= 700) {
        # Correct the CO2 with the field temp
        return(
          rawCO2 * fieldPressure * 298 / ( 1013 * (273 + temp) )
        )
      } else if (!is.na(altPressure)) {
        # Else if the altPressure is present
        # Correct the CO2 with the pressure calculated from the altitude and temperature
        return(
          rawCO2 * altPressure * 298 / ( 1013 * (273 + temp) )
        )
      }
    }
  }

  # If nothing is returned, return NA
  as.numeric(NA)
}
