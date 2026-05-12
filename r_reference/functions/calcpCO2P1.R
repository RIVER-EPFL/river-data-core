# Source: cnet-data-portal (MIT, mclement18)
# File: app/utils/calculation_functions.R
# Lines: 937-993
#
# Verbatim copy — do not modify. See generate_fixtures.R for the
# standalone version used to produce golden test values.

calcpCO2P1 <- function(df, pool, ...) {
  # Check for the presence of the correct columns
  allColumns <- sum(
    grepl(
      paste(
        c('WTW_Temp_degC_1',
          'Field_BP$',
          'Field_BP_altitude$',
          'CO2_HS_Um'),
        collapse = '|'
      ),
      colnames(df)
    )
  ) == 4
  
  if (nrow(df) == 1 & allColumns) {
    # Define constants to get
    cst_to_get <- c('c_const')
    
    # Get constants
    constants <- getRows(pool, 'constants', name %in% cst_to_get, columns = c('name', 'value'))
    
    # values needed
    co2 <- df %>% select(starts_with('CO2_HS_Um')) %>% pull()
    water_temp_k <- 273.15 + df %>% pull('WTW_Temp_degC_1')
    fieldPressure <- df %>% pull('Field_BP')
    altPressure <- df %>% pull('Field_BP_altitude')
    # If the fieldPressure is present and within the range
    if (!is.na(fieldPressure) & fieldPressure <= 1050 & fieldPressure >= 700) {
      # Use filed pressure
      bp <- fieldPressure
    } else {
      # Else use altPressure
      bp <- altPressure
    }
    
    # Constant needed
    c_const <- constants %>% filter(name == 'c_const') %>% pull('value')
    
    if (!any(is.na(c(co2, water_temp_k, bp, c_const)))) {
      # Calculate intermediate variables
      dividend <- co2 * bp
      divisor <- 0.034 * exp(c_const * (1/water_temp_k - 1/298.15)) * 1013.25
      
      # Check for presence of both dividend and divisor
      if (divisor != 0) {
        return(
          # Calculate CH4
          dividend / divisor
        )
      }
    }
  }
  
  # If nothing is returned, return NA
  as.numeric(NA)
}
