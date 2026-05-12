# Source: cnet-data-portal (MIT, mclement18)
# File: app/utils/calculation_functions.R
# Lines: 221-240
#
# Verbatim copy — do not modify. See generate_fixtures.R for the
# standalone version used to produce golden test values.

calcAFDM <- function(df, ...) {
  # Check for the presence of the correct columns
  if (nrow(df) == 1 & all(c('lab_tss_wgt_samp_filt_dried', 'lab_tss_wgt_samp_filt_ashed', 'lab_tss_vol_filtered') %in% colnames(df))) {
    wgtDried <- df %>% pull('lab_tss_wgt_samp_filt_dried')
    wgtAshed <- df %>% pull('lab_tss_wgt_samp_filt_ashed')
    volFiltered <- df %>% pull('lab_tss_vol_filtered')
    
    # Check values
    values <- c(wgtDried, wgtAshed, volFiltered)
    if (length(values) == 3 & !any(is.na(values)) & is.numeric(values)) {
      # Calculate the TSS value
      return(
        1000000 * (wgtDried - wgtAshed) / volFiltered
      )
    }
  }
  
  # If nothing is returned, return 'KEEP OLD'
  'KEEP OLD'
}
