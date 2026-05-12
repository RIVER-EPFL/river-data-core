# Source: cnet-data-portal (MIT, mclement18)
# File: app/utils/calculation_functions.R
# Lines: 688-789
#
# Verbatim copy — do not modify. See generate_fixtures.R for the
# standalone version used to produce golden test values.

calcCH4 <- function(df, pool, labTemp = 'default', labPa = 'default', ...) {
  # labTemp and labPa values c('default', 'cst', 'db')
  labParams <- list(
    labTemp = labTemp,
    labPa = labPa
  )
  
  # Check for the presence of the correct columns
  allColumns <- sum(
    grepl(
      paste(
        c('WTW_Temp_degC_1',
          'Field_BP$',
          'Field_BP_altitude$',
          'lab_co2_lab_temp',
          'lab_co2_lab_press',
          'lab_co2_ch4_dry'),
        collapse = '|'
      ),
      colnames(df)
    )
  ) == 6
  
  if (nrow(df) == 1 & allColumns) {
    # Define constants to get
    cst_to_get <- c('lab_press_avg_atm', 'lab_temp_avg_degC', 'ch4_in_sa', 'gas_const_r_mol', 'h_ch4_29815k')
    
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
        # Get lab temp from constant
      } else if (labParams[[param]] == 'cst') {
        labParams[[param]] <- constants %>%
          filter(name == cstName) %>%
          pull('value')
      } else if (labParams[[param]] == 'default') {
        # Get db temp
        labParams[[param]] <- df %>% pull(dbName)
        # If its value is NA, use constant
        if (is.na(labParams[[param]])) labParams[[param]] <- constants %>%
            filter(name == cstName) %>%
            pull('value')
      }
    }
    
    # Calculate temp in Kelvin
    labParams$labTemp <- labParams$labTemp + 273.15
    
    # values needed
    ch4_dry <- df %>% select(starts_with('lab_co2_ch4_dry')) %>% pull()
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
    ch4_in_sa <- constants %>% filter(name == 'ch4_in_sa') %>% pull('value')
    gas_const_r_mol <- constants %>% filter(name == 'gas_const_r_mol') %>% pull('value')
    h_ch4_29815k <- constants %>% filter(name == 'h_ch4_29815k') %>% pull('value')
    
    if (!any(is.na(c(ch4_dry, water_temp_k, bp, ch4_in_sa, gas_const_r_mol, h_ch4_29815k, labParams$labTemp, labParams$labPa)))) {
      # Calculate intermediate variables
      h_ch4_t_eq <- h_ch4_29815k * exp(1750 * (1/labParams$labTemp - 1/298.15))
      A <- ch4_dry * (0.957237 * 1013.25) * 101.325 * water_temp_k - bp * (ch4_in_sa * labParams$labTemp * 10^3)
      B <- h_ch4_t_eq * gas_const_r_mol * 10 * water_temp_k + bp
      
      dividend <- A * B
      divisor <- labParams$labTemp * bp * gas_const_r_mol * water_temp_k
      
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
