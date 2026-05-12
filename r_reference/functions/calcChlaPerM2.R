# Source: cnet-data-portal (MIT, mclement18)
# File: app/utils/calculation_functions.R
# Lines: 516-558
#
# Verbatim copy — do not modify. See generate_fixtures.R for the
# standalone version used to produce golden test values.

calcChlaPerM2 <- function(df, ...) {
  # Check for the presence of the correct columns
  allColumns <- sum(
    grepl(
      paste(
        c('lab_chla_sizeA_rep',
          'lab_chla_sizeB_rep',
          'lab_chla_sizeC_rep',
          'lab_chla_tot_vol_rep',
          'lab_chla_vol_filtrated_rep',
          'chla_(no)?acid_ugL_rep'),
        collapse = '|'
      ),
      colnames(df)
    )
  ) == 6
  
  if (nrow(df) == 1 & allColumns) {
    # Get values
    lab_chla_sizeA_rep <- df %>% select(starts_with('lab_chla_sizeA_rep')) %>% pull()
    lab_chla_sizeB_rep <- df %>% select(starts_with('lab_chla_sizeB_rep')) %>% pull()
    lab_chla_sizeC_rep <- df %>% select(starts_with('lab_chla_sizeC_rep')) %>% pull()
    lab_chla_tot_vol_rep <- df %>% select(starts_with('lab_chla_tot_vol_rep')) %>% pull()
    lab_chla_vol_filtrated_rep <- df %>% select(starts_with('lab_chla_vol_filtrated_rep')) %>% pull()
    chla_ugl <- df %>% select(matches('chla_(no)?acid_ugL_rep')) %>% pull()
    
    # If no NAs, calculate Chla per m2
    if (!any(is.na(c(lab_chla_sizeA_rep, lab_chla_sizeB_rep, lab_chla_sizeC_rep, lab_chla_tot_vol_rep, lab_chla_vol_filtrated_rep, chla_ugl)))) {
      return(
        convertToUnitPerM2(
          chla_ugl * 0.005,
          c(lab_chla_sizeA_rep, lab_chla_sizeB_rep, lab_chla_sizeC_rep),
          lab_chla_vol_filtrated_rep,
          lab_chla_tot_vol_rep
        )
      )
    }
  }
  
  
  # If nothing is returned, return 'KEEP OLD'
  'KEEP OLD'
}
