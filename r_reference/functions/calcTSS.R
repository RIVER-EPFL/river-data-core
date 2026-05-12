# Source: cnet-data-portal (MIT, mclement18)
# File: app/utils/calculation_functions.R
# Lines: 198-217
#
# Verbatim copy — do not modify. See generate_fixtures.R for the
# standalone version used to produce golden test values.

calcTSS <- function(df, ...) {
  # Check for the presence of the correct columns
  if (nrow(df) == 1 & all(c('lab_tss_wgt_samp_filt_dried', 'lab_tss_wgt_filt_prefiltr', 'lab_tss_vol_filtered') %in% colnames(df))) {
    wgtDried <- df %>% pull('lab_tss_wgt_samp_filt_dried')
    wgtPrefilt <- df %>% pull('lab_tss_wgt_filt_prefiltr')
    volFiltered <- df %>% pull('lab_tss_vol_filtered')

    # Check values
    values <- c(wgtDried, wgtPrefilt, volFiltered)
    if (length(values) == 3 & !any(is.na(values)) & is.numeric(values)) {
      # Calculate the TSS value
      return(
        1000000 * (wgtDried - wgtPrefilt) / volFiltered
      )
    }
  }

  # If nothing is returned, return 'KEEP OLD'
  'KEEP OLD'
}
