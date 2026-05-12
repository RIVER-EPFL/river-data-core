# Source: cnet-data-portal (MIT, mclement18)
# File: app/utils/calculation_functions.R
# Lines: 287-368
#
# Verbatim copy — do not modify. See generate_fixtures.R for the
# standalone version used to produce golden test values.

calcDIC <- function(df, pool, labTemp = 'default', labPressure = 'default', ...) {
  # labTemp values c('default', 'cst', 'db')
  
  # Check for the presence of the correct columns
  allColumns <- sum(
    grepl(
      paste(
        c('lab_dic_air_temp',
          'lab_dic_acid_sample_wght',
          'lab_dic_acid_wght',
          'lab_dic_vol_overpressure',
          'lab_dic_SA_added',
          'lab_dic_co2_dry'),
        collapse = '|'
      ),
      colnames(df)
    )
  ) == 6
  
  if (nrow(df) == 1 & allColumns) {
    # Define constants to get
    cst_to_get <- c('h_co2_29815k', 'gas_const_r_mol', 'vial_volume', 'h3po4_added', 'lab_temp_avg_degC', 'lab_press_avg_atm')
    
    # Get constants
    constants <- getRows(pool, 'constants', name %in% cst_to_get, columns = c('name', 'value'))
    
    # Determine which constant to use, from data entry (db) or constant table (cst)
    # The default argument will prevail the 'db' and then fallback to the 'cst'
    # Get lab temp from data
    if (labTemp == 'db') {
      lab_temp <- df %>% pull('lab_dic_air_temp')
      # Get lab temp from constant
    } else if (labTemp == 'cst') {
      lab_temp <- constants %>%
        filter(name == 'lab_temp_avg_degC') %>%
        pull('value')
    } else if (labPressure == 'default') {
      # Get db temp
      lab_temp <- df %>% pull('lab_dic_air_temp')
      # If its value is NA, use constant
      if (is.na(lab_temp)) lab_temp <- constants %>%
          filter(name == 'lab_temp_avg_degC') %>%
          pull('value')
    }
    # Calculate temp in Kelvin
    lab_temp <- lab_temp + 273.15
    
    # values needed
    lab_dic_acid_sample_wght <- df %>% select(starts_with('lab_dic_acid_sample_wght')) %>% pull()
    lab_dic_acid_wght <- df %>% select(starts_with('lab_dic_acid_wght')) %>% pull()
    lab_dic_vol_overpressure <- df %>% select(starts_with('lab_dic_vol_overpressure')) %>% pull()
    lab_dic_SA_added <- df %>% select(starts_with('lab_dic_SA_added')) %>% pull()
    lab_dic_co2_dry <- df %>% select(starts_with('lab_dic_co2_dry')) %>% pull()
    
    # Constant needed
    h_co2_29815k <- constants %>% filter(name == 'h_co2_29815k') %>% pull('value')
    gas_const_r_mol <- constants %>% filter(name == 'gas_const_r_mol') %>% pull('value')
    vial_volume <- constants %>% filter(name == 'vial_volume') %>% pull('value')
    h3po4_added <- constants %>% filter(name == 'h3po4_added') %>% pull('value')
    
    # Calculate intermediate variables
    sampleV <- lab_dic_acid_sample_wght - lab_dic_acid_wght
    hsV <- vial_volume + lab_dic_vol_overpressure - (sampleV + h3po4_added)
    co2_acid <- lab_dic_co2_dry * (lab_dic_SA_added + hsV)
    gas_temp <- gas_const_r_mol * lab_temp
    exponent <- exp(2392.86 * (1/lab_temp - 1/298.15))
    
    dividend <- co2_acid * (h_co2_29815k * exponent * sampleV * gas_temp + 101.325 * hsV)
    divisor <- 10^3 * gas_temp * hsV * sampleV
    
    # Check for presence of both dividend and divisor
    if (!any(is.na(c(dividend, divisor))) & divisor != 0) {
      return(
        # Calculate DIC
        dividend / divisor
      )
    }
  }
  
  # If nothing is returned, return NA
  as.numeric(NA)
}
