# Source: cnet-data-portal (MIT, mclement18)
# File: app/utils/calculation_functions.R
# Lines: 87-105
#
# Verbatim copy — do not modify. See generate_fixtures.R for the
# standalone version used to produce golden test values.

calcAlt2BP <- function(df, pool, ...) {
  # Check for the presence of the correct columns
  if (nrow(df) == 1 & all(c('station', 'WTW_Temp_degC_1') %in% colnames(df))) {
    station <- df %>% pull('station')
    temp <- df %>% pull('WTW_Temp_degC_1')
    # Get elevation
    elev <- getRows(pool, 'stations', name == station, columns = c('order', 'elevation')) %>% arrange(order) %>% pull(elevation)
    
    # If there is an elevation and a temp, calculate the pressure in hPa
    if (!any(is.na(c(elev, temp)))) {
      return(
        round(bigleaf::pressure.from.elevation(elev, temp) * 10)
      )
    }
  }
  
  # If nothing is returned, return NA
  as.numeric(NA)
}
