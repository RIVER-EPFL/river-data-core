# Source: cnet-data-portal (MIT, mclement18)
# File: app/utils/calculation_functions.R
# Lines: 149-180
#
# Verbatim copy — do not modify. See generate_fixtures.R for the
# standalone version used to produce golden test values.

calcDOC <- function(df, func, pool) {
  # Check for the presence of the correct columns
  allColumns <- sum(
    grepl(
      paste(
        c('DOC_rep',
          'doc_std_curve_id'),
        collapse = '|'
      ),
      colnames(df)
    )
  ) == 4
  
  if (nrow(df) == 1 & ncol(df) == 4 & allColumns) {
    # Split df
    stdCurveId <- df %>% pull('doc_std_curve_id')
    reps <- df %>% select(-doc_std_curve_id)
    
    # Correct values if there is a std curve id
    if (!is.na(stdCurveId) & stdCurveId  > 0) {
      stdCurve <- getRows(pool, 'standard_curves', id == stdCurveId)
      reps %<>% mutate(
        across(everything(), ~.x * stdCurve$a + stdCurve$b)
      )
    }
    
    # Run calculation function (either mean or sd)
    return(
      func(reps)
    )
  }
}
