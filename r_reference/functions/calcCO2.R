# Source: cnet-data-portal (MIT, mclement18)
# File: app/utils/calculation_functions.R
# Lines: 794-883
#
# Verbatim copy — do not modify. See generate_fixtures.R for the
# standalone version used to produce golden test values.

calcCO2 <- function(df, pool, labTemp = 'default', labPa = 'default', ...) {
  # labTemp and labPa values c('default', 'cst', 'db')
  labParams <- list(
    labTemp = labTemp,
    labPa = labPa
  )

  # Check for the presence of the correct columns
  allColumns <- sum(
    grepl(
      paste(
        c('lab_co2_lab_temp',
          'lab_co2_lab_press',
          'lab_co2_co2ppm'),
        collapse = '|'
      ),
      colnames(df)
    )
  ) == 3

  if (nrow(df) == 1 & allColumns) {
    # Define constants to get
    cst_to_get <- c('lab_press_avg_atm', 'lab_temp_avg_degC', 'vol_sa', 'vol_water', 'c_const', 'gas_const_r_atm')

    # Get constants
    constants <- getRows(pool, 'constants', name %in% cst_to_get, columns = c('name', 'value'))

    # Determine which constant to use, from data entry (db) or constant table (cst)
    # The default argument will prevail the 'db' and then fallback to the 'cst'
    for (param in names(labParams)) {
      if (param == 'labTemp') {
        cstName <- 'lab_temp_avg_degC'
        dbName <- 'lab_co2_lab_temp'
      } else {
        cstName <- 'lab_press_avg_atm'
        dbName <- 'lab_co2_lab_press'
      }
      # Get lab temp from data
      if (labParams[[param]] == 'db') {
        labParams[[param]] <- df %>% pull(dbName)
        # Convert hPa to atm
        if (param == 'labPa') labParams[[param]] <- labParams[[param]] / 1013.25
        # Get lab temp from constant
      } else if (labParams[[param]] == 'cst') {
        labParams[[param]] <- constants %>%
          filter(name == cstName) %>%
          pull('value')
      } else if (labParams[[param]] == 'default') {
        # Get db temp
        labParams[[param]] <- df %>% pull(dbName)
        # Convert hPa to atm
        if (param == 'labPa') labParams[[param]] <- labParams[[param]] / 1013.25
        # If its value is NA, use constant
        if (is.na(labParams[[param]])) labParams[[param]] <- constants %>%
            filter(name == cstName) %>%
            pull('value')
      }
    }

    # Calculate temp in Kelvin
    labParams$labTemp <- labParams$labTemp + 273.15

    # values needed
    co2 <- df %>% select(starts_with('lab_co2_co2ppm')) %>% pull()

    # Constant needed
    vol_sa <- constants %>% filter(name == 'vol_sa') %>% pull('value')
    vol_water <- constants %>% filter(name == 'vol_water') %>% pull('value')
    c_const <- constants %>% filter(name == 'c_const') %>% pull('value')
    gas_const_r_atm <- constants %>% filter(name == 'gas_const_r_atm') %>% pull('value')

    if (!any(is.na(c(co2, vol_sa, vol_water, c_const, gas_const_r_atm, labParams$labTemp, labParams$labPa)))) {
      # Calculate intermediate variables
      exponent <- exp(c_const * (1/labParams$labTemp - 1/298.15))
      dividend <- co2 * labParams$labPa * (vol_sa + 0.034 * exponent * vol_water * gas_const_r_atm * labParams$labTemp)
      divisor <- gas_const_r_atm * vol_water * labParams$labTemp

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
