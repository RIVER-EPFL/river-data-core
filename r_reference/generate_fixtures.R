#!/usr/bin/env Rscript
#
# Bulk golden value fixture generator for river-data-core toolbox tests.
#
# Generates ~12,500 test cases (500 random + ~15 boundary/edge per function)
# using realistic Swiss Alpine river monitoring ranges.
#
# Requirements: R >= 4.0, jsonlite
# Run: Rscript r_reference/generate_fixtures.R

library(jsonlite)
set.seed(42)

N <- 500  # random cases per function

# =============================================================================
# Standalone pure-math implementations (matching portal calculation_functions.R)
# =============================================================================

calc_mean <- function(values) {
  v <- values[!is.na(values)]
  if (length(v) == 0) return(NA_real_)
  mean(v)
}

calc_sd <- function(values) {
  v <- values[!is.na(values)]
  if (length(v) < 2) return(NA_real_)
  sd(v)
}

calc_minus <- function(a, b) {
  if (is.na(a) || is.na(b)) return(NA_real_)
  a - b
}

calc_equals <- function(primary, fallback) {
  if (is.na(primary)) return(fallback)
  primary
}

calc_ratio <- function(dividend, divisor) {
  if (is.na(dividend) || is.na(divisor) || divisor == 0) return(NA_real_)
  dividend / divisor
}

calc_tss <- function(wgt_dried, wgt_prefilt, vol_filtered) {
  if (any(is.na(c(wgt_dried, wgt_prefilt, vol_filtered)))) return(NA_real_)
  if (vol_filtered == 0) return(NA_real_)
  1000000 * (wgt_dried - wgt_prefilt) / vol_filtered
}

calc_afdm <- function(wgt_dried, wgt_ashed, vol_filtered) {
  if (any(is.na(c(wgt_dried, wgt_ashed, vol_filtered)))) return(NA_real_)
  if (vol_filtered == 0) return(NA_real_)
  1000000 * (wgt_dried - wgt_ashed) / vol_filtered
}

calc_suva <- function(a254, doc_avg_ppb) {
  if (any(is.na(c(a254, doc_avg_ppb))) || doc_avg_ppb == 0) return(NA_real_)
  a254 * 1000 / doc_avg_ppb
}

calc_doc_average <- function(replicates, slope = NA, intercept = NA) {
  if (!is.na(slope) && !is.na(intercept)) replicates <- replicates * slope + intercept
  calc_mean(replicates)
}

calc_doc_sd <- function(replicates, slope = NA, intercept = NA) {
  if (!is.na(slope) && !is.na(intercept)) replicates <- replicates * slope + intercept
  calc_sd(replicates)
}

calc_chla_acid <- function(fluor_before, fluor_after, slope, intercept) {
  if (any(is.na(c(fluor_before, fluor_after, slope, intercept)))) return(NA_real_)
  (fluor_before - fluor_after) * slope + intercept
}

calc_chla_no_acid <- function(fluor, slope, intercept) {
  if (any(is.na(c(fluor, slope, intercept)))) return(NA_real_)
  fluor * slope + intercept
}

calc_rock_surface_area <- function(dims_cm) {
  d_m <- (dims_cm / 100)^1.6075
  pairs <- combn(d_m, 2, prod)
  2 * pi * mean(pairs)^(1/1.6075)
}

calc_per_m2 <- function(sample_value, dims_cm, vol_filtrated, vol_total) {
  area <- calc_rock_surface_area(dims_cm)
  if (area == 0 || vol_filtrated == 0) return(NA_real_)
  sample_value * vol_total / (vol_filtrated * area)
}

calc_benthic_afdm <- function(afdm_g, dims_cm, vol_filtrated, vol_total) {
  calc_per_m2(afdm_g, dims_cm, vol_filtrated, vol_total)
}

calc_barometric_pressure <- function(elevation_m, temp_c) {
  if (any(is.na(c(elevation_m, temp_c)))) return(NA_real_)
  p_kpa <- 101.325 * (1 - 0.0065 * elevation_m / (temp_c + 273.15 + 0.0065 * elevation_m))^5.2561
  p_kpa * 10
}

calc_co2_correction <- function(raw_co2, temp_c, pressure_hpa, std_slope = NA, std_intercept = NA) {
  if (any(is.na(c(raw_co2, temp_c, pressure_hpa)))) return(NA_real_)
  co2 <- raw_co2
  if (!is.na(std_slope) && !is.na(std_intercept)) co2 <- co2 * std_slope + std_intercept
  co2 * pressure_hpa * 298 / (1013 * (273 + temp_c))
}

calc_ch4_dry <- function(ch4_wet, h2o_percent) {
  if (any(is.na(c(ch4_wet, h2o_percent)))) return(NA_real_)
  (h2o_percent * 1.2347 - 0.0016) * ch4_wet / 100 + ch4_wet
}

calc_pco2 <- function(co2_aq_umol, water_temp_c, c_const = 2392.86) {
  if (any(is.na(c(co2_aq_umol, water_temp_c)))) return(NA_real_)
  water_temp_k <- water_temp_c + 273.15
  divisor <- 0.034 * exp(c_const * (1/water_temp_k - 1/298.15))
  if (divisor == 0) return(NA_real_)
  co2_aq_umol / divisor
}

calc_pco2_p1 <- function(co2_aq_umol, water_temp_c, bp_hpa, c_const = 2392.86) {
  if (any(is.na(c(co2_aq_umol, water_temp_c, bp_hpa)))) return(NA_real_)
  water_temp_k <- water_temp_c + 273.15
  dividend <- co2_aq_umol * bp_hpa
  divisor <- 0.034 * exp(c_const * (1/water_temp_k - 1/298.15)) * 1013.25
  if (divisor == 0) return(NA_real_)
  dividend / divisor
}

calc_pco2_p2 <- function(co2_aq_umol, water_temp_c, bp_hpa, c_const = 2392.86) {
  if (any(is.na(c(co2_aq_umol, water_temp_c, bp_hpa)))) return(NA_real_)
  water_temp_k <- water_temp_c + 273.15
  dividend <- co2_aq_umol * 1013.25
  divisor <- 0.034 * exp(c_const * (1/water_temp_k - 1/298.15)) * bp_hpa
  if (divisor == 0) return(NA_real_)
  dividend / divisor
}

calc_co2_headspace <- function(co2_ppm, lab_temp_c, lab_pressure_atm,
                                vol_sa_ml, vol_water_ml,
                                kh_co2 = 0.034, c_const = 2392.86,
                                gas_const_r_atm = 0.08206) {
  if (any(is.na(c(co2_ppm, lab_temp_c, lab_pressure_atm, vol_sa_ml, vol_water_ml)))) return(NA_real_)
  t_lab_k <- lab_temp_c + 273.15
  exponent <- exp(c_const * (1/t_lab_k - 1/298.15))
  dividend <- co2_ppm * lab_pressure_atm * (vol_sa_ml + kh_co2 * exponent * vol_water_ml * gas_const_r_atm * t_lab_k)
  divisor <- gas_const_r_atm * vol_water_ml * t_lab_k
  if (divisor == 0) return(NA_real_)
  dividend / divisor
}

calc_dic <- function(acid_sample_wght, acid_wght, vol_overpressure,
                      sa_added, co2_dry, air_temp_c,
                      h_co2_29815k = 3.3e-4, gas_const_r_mol = 8.314,
                      vial_volume = 12.0, h3po4_added = 0.1) {
  if (any(is.na(c(acid_sample_wght, acid_wght, vol_overpressure, sa_added, co2_dry, air_temp_c)))) return(NA_real_)
  lab_temp_k <- air_temp_c + 273.15
  sample_v <- acid_sample_wght - acid_wght
  hs_v <- vial_volume + vol_overpressure - (sample_v + h3po4_added)
  co2_acid <- co2_dry * (sa_added + hs_v)
  gas_temp <- gas_const_r_mol * lab_temp_k
  exponent <- exp(2392.86 * (1/lab_temp_k - 1/298.15))
  dividend <- co2_acid * (h_co2_29815k * exponent * sample_v * gas_temp + 101.325 * hs_v)
  divisor <- 10^3 * gas_temp * hs_v * sample_v
  if (is.na(divisor) || divisor == 0) return(NA_real_)
  dividend / divisor
}

calc_d13c_dic <- function(acid_sample_wght, acid_wght, vol_overpressure,
                           delta_13co2, air_temp_c,
                           h_co2_29815k = 3.3e-4, gas_const_r_mol = 8.314,
                           vial_volume = 12.0, h3po4_added = 0.1) {
  if (any(is.na(c(acid_sample_wght, acid_wght, vol_overpressure, delta_13co2, air_temp_c)))) return(NA_real_)
  lab_temp_k <- air_temp_c + 273.15
  sample_v <- acid_sample_wght - acid_wght
  hs_v <- vial_volume + vol_overpressure - (sample_v + h3po4_added)
  exponent <- exp(2392.86 * (1/lab_temp_k - 1/298.15))
  H <- h_co2_29815k * exponent * sample_v * gas_const_r_mol
  dividend <- delta_13co2 * 101.325 * hs_v + (lab_temp_k * (delta_13co2 + 0.19) - 373) * H
  divisor <- 101.325 * hs_v + H * lab_temp_k
  if (is.na(divisor) || divisor == 0) return(NA_real_)
  dividend / divisor
}

calc_dissolved_ch4 <- function(ch4_dry_ppb, water_temp_c, bp_hpa,
                                lab_temp_c, lab_pressure_atm,
                                kh_ch4 = 0.0014, ch4_temp_const = 1750.0,
                                ch4_in_sa = 1.9, gas_const_r_mol = 8.314) {
  if (any(is.na(c(ch4_dry_ppb, water_temp_c, bp_hpa, lab_temp_c, lab_pressure_atm)))) return(NA_real_)
  lab_temp_k <- lab_temp_c + 273.15
  water_temp_k <- water_temp_c + 273.15
  h_ch4_t_eq <- kh_ch4 * exp(ch4_temp_const * (1/lab_temp_k - 1/298.15))
  A <- ch4_dry_ppb * (lab_pressure_atm * 1013.25) * 101.325 * water_temp_k - bp_hpa * (ch4_in_sa * lab_temp_k * 10^3)
  B <- h_ch4_t_eq * gas_const_r_mol * 10 * water_temp_k + bp_hpa
  dividend <- A * B
  divisor <- lab_temp_k * bp_hpa * gas_const_r_mol * water_temp_k
  if (divisor == 0) return(NA_real_)
  dividend / divisor
}


# =============================================================================
# Helpers
# =============================================================================

tc <- function(name, inputs, expected, tolerance = 1e-10) {
  list(name = name, inputs = inputs, expected = expected, tolerance = tolerance)
}

maybe_na <- function(val, prob = 0.05) {
  if (runif(1) < prob) NA_real_ else val
}

# =============================================================================
# Bulk case generators
# =============================================================================

gen_common_mean <- function() {
  cases <- list()
  # Boundary
  cases <- c(cases, list(
    tc("single", list(values = c(42.0)), 42.0),
    tc("two_equal", list(values = c(5.0, 5.0)), 5.0),
    tc("all_na", list(values = c(NA, NA)), NA_real_),
    tc("one_na", list(values = c(1.0, NA, 3.0)), 2.0),
    tc("zero", list(values = c(0.0)), 0.0),
    tc("negative", list(values = c(-10.0, 10.0)), 0.0)
  ))
  # Bulk
  for (i in seq_len(N)) {
    n_vals <- sample(2:8, 1)
    vals <- runif(n_vals, -100, 500)
    if (runif(1) < 0.05) vals[sample(length(vals), 1)] <- NA
    cases <- c(cases, list(tc(paste0("rand_", i), list(values = vals), calc_mean(vals))))
  }
  cases
}

gen_common_sd <- function() {
  cases <- list()
  cases <- c(cases, list(
    tc("identical", list(values = c(3.0, 3.0, 3.0)), 0.0),
    tc("two_vals", list(values = c(1.0, 3.0)), sd(c(1, 3))),
    tc("single", list(values = c(5.0)), NA_real_),
    tc("all_na", list(values = c(NA, NA)), NA_real_)
  ))
  for (i in seq_len(N)) {
    n_vals <- sample(2:8, 1)
    vals <- runif(n_vals, -100, 500)
    if (runif(1) < 0.05) vals[sample(length(vals), 1)] <- NA
    cases <- c(cases, list(tc(paste0("rand_", i), list(values = vals), calc_sd(vals))))
  }
  cases
}

gen_common_minus <- function() {
  cases <- list()
  cases <- c(cases, list(
    tc("zero", list(a = 5.0, b = 5.0), 0.0),
    tc("positive", list(a = 10.0, b = 3.0), 7.0),
    tc("negative", list(a = 3.0, b = 10.0), -7.0),
    tc("a_na", list(a = NA, b = 3.0), NA_real_),
    tc("b_na", list(a = 10.0, b = NA), NA_real_)
  ))
  for (i in seq_len(N)) {
    a <- maybe_na(runif(1, -500, 500))
    b <- maybe_na(runif(1, -500, 500))
    cases <- c(cases, list(tc(paste0("rand_", i), list(a = a, b = b), calc_minus(a, b))))
  }
  cases
}

gen_common_equals <- function() {
  cases <- list()
  cases <- c(cases, list(
    tc("primary_valid", list(primary = 42.0, fallback = 99.0), 42.0),
    tc("primary_na", list(primary = NA, fallback = 99.0), 99.0),
    tc("both_na", list(primary = NA, fallback = NA), NA_real_)
  ))
  for (i in seq_len(N)) {
    p <- maybe_na(runif(1, -100, 500), 0.3)
    f <- maybe_na(runif(1, -100, 500), 0.1)
    cases <- c(cases, list(tc(paste0("rand_", i), list(primary = p, fallback = f), calc_equals(p, f))))
  }
  cases
}

gen_common_ratio <- function() {
  cases <- list()
  cases <- c(cases, list(
    tc("normal", list(dividend = 10.0, divisor = 3.0), 10.0 / 3.0),
    tc("zero_div", list(dividend = 10.0, divisor = 0.0), NA_real_),
    tc("neg", list(dividend = -6.0, divisor = 2.0), -3.0),
    tc("both_na", list(dividend = NA, divisor = NA), NA_real_)
  ))
  for (i in seq_len(N)) {
    a <- maybe_na(runif(1, -500, 500))
    b <- maybe_na(runif(1, -500, 500))
    cases <- c(cases, list(tc(paste0("rand_", i), list(dividend = a, divisor = b), calc_ratio(a, b))))
  }
  cases
}

gen_tss <- function() {
  cases <- list()
  cases <- c(cases, list(
    tc("clean", list(wgt_dried = 0.1005, wgt_prefilt = 0.1000, vol_filtered = 500.0),
       calc_tss(0.1005, 0.1000, 500.0)),
    tc("turbid", list(wgt_dried = 0.1150, wgt_prefilt = 0.1000, vol_filtered = 250.0),
       calc_tss(0.1150, 0.1000, 250.0)),
    tc("zero_vol", list(wgt_dried = 0.1025, wgt_prefilt = 0.1000, vol_filtered = 0.0), NA_real_),
    tc("na_wgt", list(wgt_dried = NA, wgt_prefilt = 0.1000, vol_filtered = 500.0), NA_real_)
  ))
  for (i in seq_len(N)) {
    prefilt <- runif(1, 0.05, 0.15)
    dried <- prefilt + runif(1, -0.005, 0.05)
    vol <- maybe_na(runif(1, 50, 1000))
    cases <- c(cases, list(tc(paste0("rand_", i),
      list(wgt_dried = dried, wgt_prefilt = prefilt, vol_filtered = vol),
      calc_tss(dried, prefilt, vol))))
  }
  cases
}

gen_afdm <- function() {
  cases <- list()
  cases <- c(cases, list(
    tc("normal", list(wgt_dried = 0.1025, wgt_ashed = 0.1005, vol_filtered = 500.0),
       calc_afdm(0.1025, 0.1005, 500.0)),
    tc("zero_vol", list(wgt_dried = 0.1025, wgt_ashed = 0.1005, vol_filtered = 0.0), NA_real_),
    tc("na", list(wgt_dried = NA, wgt_ashed = 0.1005, vol_filtered = 500.0), NA_real_)
  ))
  for (i in seq_len(N)) {
    dried <- runif(1, 0.05, 0.25)
    ashed <- dried - runif(1, 0.0, dried * 0.3)
    vol <- maybe_na(runif(1, 50, 1000))
    cases <- c(cases, list(tc(paste0("rand_", i),
      list(wgt_dried = dried, wgt_ashed = ashed, vol_filtered = vol),
      calc_afdm(dried, ashed, vol))))
  }
  cases
}

gen_suva <- function() {
  cases <- list()
  cases <- c(cases, list(
    tc("typical", list(a254 = 0.15, doc_avg_ppb = 2500.0), calc_suva(0.15, 2500.0)),
    tc("zero_doc", list(a254 = 0.15, doc_avg_ppb = 0.0), NA_real_),
    tc("na", list(a254 = NA, doc_avg_ppb = 2500.0), NA_real_)
  ))
  for (i in seq_len(N)) {
    a <- maybe_na(runif(1, 0.01, 0.5))
    d <- maybe_na(runif(1, 100, 10000))
    cases <- c(cases, list(tc(paste0("rand_", i), list(a254 = a, doc_avg_ppb = d), calc_suva(a, d))))
  }
  cases
}

gen_doc_avg <- function() {
  cases <- list()
  cases <- c(cases, list(
    tc("no_curve", list(replicates = c(120.0, 125.0, 118.0), slope = NA, intercept = NA),
       calc_doc_average(c(120, 125, 118))),
    tc("with_curve", list(replicates = c(120.0, 125.0, 118.0), slope = 1.05, intercept = -2.0),
       calc_doc_average(c(120, 125, 118), 1.05, -2.0)),
    tc("all_na", list(replicates = c(NA, NA, NA), slope = NA, intercept = NA), NA_real_)
  ))
  for (i in seq_len(N)) {
    n_rep <- sample(2:5, 1)
    reps <- runif(n_rep, 50, 500)
    if (runif(1) < 0.05) reps[sample(length(reps), 1)] <- NA
    use_curve <- runif(1) < 0.5
    sl <- if (use_curve) runif(1, 0.9, 1.2) else NA
    int <- if (use_curve) runif(1, -5, 5) else NA
    cases <- c(cases, list(tc(paste0("rand_", i),
      list(replicates = reps, slope = sl, intercept = int),
      calc_doc_average(reps, sl, int))))
  }
  cases
}

gen_doc_sd <- function() {
  cases <- list()
  cases <- c(cases, list(
    tc("no_curve", list(replicates = c(120.0, 125.0, 118.0), slope = NA, intercept = NA),
       calc_doc_sd(c(120, 125, 118))),
    tc("single", list(replicates = c(120.0), slope = NA, intercept = NA), NA_real_)
  ))
  for (i in seq_len(N)) {
    n_rep <- sample(2:5, 1)
    reps <- runif(n_rep, 50, 500)
    use_curve <- runif(1) < 0.5
    sl <- if (use_curve) runif(1, 0.9, 1.2) else NA
    int <- if (use_curve) runif(1, -5, 5) else NA
    cases <- c(cases, list(tc(paste0("rand_", i),
      list(replicates = reps, slope = sl, intercept = int),
      calc_doc_sd(reps, sl, int))))
  }
  cases
}

gen_chla_acid <- function() {
  cases <- list()
  cases <- c(cases, list(
    tc("typical", list(fluor_before = 150.0, fluor_after = 80.0, slope = 0.25, intercept = -1.5),
       calc_chla_acid(150, 80, 0.25, -1.5)),
    tc("zero_diff", list(fluor_before = 100.0, fluor_after = 100.0, slope = 0.25, intercept = -1.5),
       calc_chla_acid(100, 100, 0.25, -1.5)),
    tc("na", list(fluor_before = NA, fluor_after = 80.0, slope = 0.25, intercept = -1.5), NA_real_)
  ))
  for (i in seq_len(N)) {
    fb <- maybe_na(runif(1, 50, 300))
    fa <- if (!is.na(fb)) maybe_na(runif(1, 20, fb)) else maybe_na(runif(1, 20, 200))
    sl <- runif(1, 0.1, 0.6)
    int <- runif(1, -3, 2)
    cases <- c(cases, list(tc(paste0("rand_", i),
      list(fluor_before = fb, fluor_after = fa, slope = sl, intercept = int),
      calc_chla_acid(fb, fa, sl, int))))
  }
  cases
}

gen_chla_no_acid <- function() {
  cases <- list()
  cases <- c(cases, list(
    tc("typical", list(fluor = 150.0, slope = 0.30, intercept = -2.0),
       calc_chla_no_acid(150.0, 0.30, -2.0)),
    tc("na_slope", list(fluor = 150.0, slope = NA, intercept = -2.0), NA_real_)
  ))
  for (i in seq_len(N)) {
    fl <- maybe_na(runif(1, 50, 300))
    sl <- maybe_na(runif(1, 0.1, 0.6))
    int <- runif(1, -3, 2)
    cases <- c(cases, list(tc(paste0("rand_", i),
      list(fluor = fl, slope = sl, intercept = int),
      calc_chla_no_acid(fl, sl, int))))
  }
  cases
}

gen_rock_area <- function() {
  cases <- list()
  cases <- c(cases, list(
    tc("sphere_10cm", list(dims_cm = c(10.0, 10.0, 10.0)), calc_rock_surface_area(c(10, 10, 10))),
    tc("flat_rock", list(dims_cm = c(20.0, 15.0, 3.0)), calc_rock_surface_area(c(20, 15, 3)))
  ))
  for (i in seq_len(N)) {
    dims <- runif(3, 3, 50)
    cases <- c(cases, list(tc(paste0("rand_", i), list(dims_cm = dims), calc_rock_surface_area(dims))))
  }
  cases
}

gen_per_m2 <- function() {
  cases <- list()
  for (i in seq_len(N)) {
    sv <- runif(1, 0.001, 0.01)
    dims <- runif(3, 3, 50)
    vf <- runif(1, 10, 200)
    vt <- runif(1, 50, 500)
    cases <- c(cases, list(tc(paste0("rand_", i),
      list(sample_value = sv, dims_cm = dims, vol_filtrated = vf, vol_total = vt),
      calc_per_m2(sv, dims, vf, vt))))
  }
  cases
}

gen_benthic_afdm <- function() {
  cases <- list()
  for (i in seq_len(N)) {
    ag <- runif(1, 0.001, 0.01)
    dims <- runif(3, 3, 50)
    vf <- runif(1, 10, 200)
    vt <- runif(1, 50, 500)
    cases <- c(cases, list(tc(paste0("rand_", i),
      list(afdm_g = ag, dims_cm = dims, vol_filtrated = vf, vol_total = vt),
      calc_benthic_afdm(ag, dims, vf, vt))))
  }
  cases
}

gen_baro_pressure <- function() {
  cases <- list()
  cases <- c(cases, list(
    tc("sea_level_15c", list(elevation_m = 0.0, temp_c = 15.0), calc_barometric_pressure(0, 15), tolerance = 0.5),
    tc("martigny_470m", list(elevation_m = 470.0, temp_c = 15.0), calc_barometric_pressure(470, 15), tolerance = 0.5),
    tc("verbier_1500m", list(elevation_m = 1500.0, temp_c = 8.0), calc_barometric_pressure(1500, 8), tolerance = 0.5),
    tc("mont_blanc_4808m", list(elevation_m = 4808.0, temp_c = -10.0), calc_barometric_pressure(4808, -10), tolerance = 0.5),
    tc("zero_c", list(elevation_m = 1000.0, temp_c = 0.0), calc_barometric_pressure(1000, 0), tolerance = 0.5),
    tc("na", list(elevation_m = NA, temp_c = 15.0), NA_real_)
  ))
  for (i in seq_len(N)) {
    el <- maybe_na(runif(1, 0, 4500))
    tc_ <- runif(1, -15, 35)
    cases <- c(cases, list(tc(paste0("rand_", i),
      list(elevation_m = el, temp_c = tc_),
      calc_barometric_pressure(el, tc_), tolerance = 0.5)))
  }
  cases
}

gen_co2_correction <- function() {
  cases <- list()
  cases <- c(cases, list(
    tc("ref_conditions", list(raw_co2 = 1013.0, temp_c = 25.0, pressure_hpa = 1013.0, std_slope = NA, std_intercept = NA),
       calc_co2_correction(1013, 25, 1013)),
    tc("alpine_no_curve", list(raw_co2 = 500.0, temp_c = 10.0, pressure_hpa = 850.0, std_slope = NA, std_intercept = NA),
       calc_co2_correction(500, 10, 850)),
    tc("na_temp", list(raw_co2 = 500.0, temp_c = NA, pressure_hpa = 850.0, std_slope = NA, std_intercept = NA), NA_real_)
  ))
  for (i in seq_len(N)) {
    co2 <- maybe_na(runif(1, 100, 5000))
    tc_ <- maybe_na(runif(1, 0.5, 25))
    bp <- runif(1, 700, 1050)
    use_curve <- runif(1) < 0.5
    sl <- if (use_curve) runif(1, 0.9, 1.2) else NA
    int <- if (use_curve) runif(1, -10, 10) else NA
    cases <- c(cases, list(tc(paste0("rand_", i),
      list(raw_co2 = co2, temp_c = tc_, pressure_hpa = bp, std_slope = sl, std_intercept = int),
      calc_co2_correction(co2, tc_, bp, sl, int))))
  }
  cases
}

gen_ch4_dry <- function() {
  cases <- list()
  cases <- c(cases, list(
    tc("typical", list(ch4_wet = 2000.0, h2o_percent = 1.5), calc_ch4_dry(2000, 1.5)),
    tc("zero_h2o", list(ch4_wet = 2000.0, h2o_percent = 0.0), calc_ch4_dry(2000, 0)),
    tc("na", list(ch4_wet = NA, h2o_percent = 1.5), NA_real_)
  ))
  for (i in seq_len(N)) {
    ch4 <- maybe_na(runif(1, 500, 50000))
    h2o <- runif(1, 0, 3.5)
    cases <- c(cases, list(tc(paste0("rand_", i),
      list(ch4_wet = ch4, h2o_percent = h2o), calc_ch4_dry(ch4, h2o))))
  }
  cases
}

gen_pco2 <- function() {
  cases <- list()
  C <- 2392.86
  cases <- c(cases, list(
    tc("ref_25c", list(co2_aq = 50.0, water_temp_c = 25.0, c_const = C), calc_pco2(50, 25, C), tolerance = 1e-6),
    tc("cold_2c", list(co2_aq = 100.0, water_temp_c = 2.0, c_const = C), calc_pco2(100, 2, C), tolerance = 1e-6),
    tc("na", list(co2_aq = 50.0, water_temp_c = NA, c_const = C), NA_real_)
  ))
  for (i in seq_len(N)) {
    co2 <- maybe_na(runif(1, 5, 500))
    tc_ <- maybe_na(runif(1, 0.5, 25))
    cases <- c(cases, list(tc(paste0("rand_", i),
      list(co2_aq = co2, water_temp_c = tc_, c_const = C),
      calc_pco2(co2, tc_, C), tolerance = 1e-6)))
  }
  cases
}

gen_pco2_p1 <- function() {
  cases <- list()
  C <- 2392.86
  cases <- c(cases, list(
    tc("sea_level", list(co2_aq = 50.0, water_temp_c = 15.0, bp_hpa = 1013.25, c_const = C),
       calc_pco2_p1(50, 15, 1013.25, C), tolerance = 1e-6),
    tc("na_bp", list(co2_aq = 50.0, water_temp_c = 10.0, bp_hpa = NA, c_const = C), NA_real_)
  ))
  for (i in seq_len(N)) {
    co2 <- maybe_na(runif(1, 5, 500))
    tc_ <- runif(1, 0.5, 25)
    bp <- maybe_na(runif(1, 700, 1050))
    cases <- c(cases, list(tc(paste0("rand_", i),
      list(co2_aq = co2, water_temp_c = tc_, bp_hpa = bp, c_const = C),
      calc_pco2_p1(co2, tc_, bp, C), tolerance = 1e-6)))
  }
  cases
}

gen_pco2_p2 <- function() {
  cases <- list()
  C <- 2392.86
  for (i in seq_len(N)) {
    co2 <- runif(1, 5, 500)
    tc_ <- runif(1, 0.5, 25)
    bp <- runif(1, 700, 1050)
    cases <- c(cases, list(tc(paste0("rand_", i),
      list(co2_aq = co2, water_temp_c = tc_, bp_hpa = bp, c_const = C),
      calc_pco2_p2(co2, tc_, bp, C), tolerance = 1e-6)))
  }
  cases
}

gen_co2_headspace <- function() {
  cases <- list()
  K <- 0.034; C <- 2392.86; R <- 0.08206
  for (i in seq_len(N)) {
    ppm <- runif(1, 100, 10000)
    lt <- runif(1, 15, 30)
    lp <- runif(1, 0.9, 1.05)
    vs <- runif(1, 40, 80)
    vw <- runif(1, 20, 60)
    cases <- c(cases, list(tc(paste0("rand_", i),
      list(co2_ppm = ppm, lab_temp_c = lt, lab_pressure_atm = lp,
           vol_sa_ml = vs, vol_water_ml = vw, kh_co2 = K, c_const = C, gas_const_r_atm = R),
      calc_co2_headspace(ppm, lt, lp, vs, vw, K, C, R), tolerance = 1e-6)))
  }
  cases
}

gen_dic <- function() {
  cases <- list()
  H <- 3.3e-4; G <- 8.314; V <- 12.0; P <- 0.1
  cases <- c(cases, list(
    tc("na", list(acid_sample_wght = NA, acid_wght = 10.0, vol_overpressure = 0.5,
                  sa_added = 0.3, co2_dry = 2000.0, air_temp_c = 22.0,
                  h_co2_29815k = H, gas_const_r_mol = G, vial_volume = V, h3po4_added = P), NA_real_)
  ))
  for (i in seq_len(N)) {
    aw <- runif(1, 8, 12)
    asw <- aw + runif(1, 1, 6)
    vop <- runif(1, 0, 2)
    sa <- runif(1, 0.05, 0.5)
    co2 <- maybe_na(runif(1, 100, 10000))
    at <- runif(1, 15, 30)
    cases <- c(cases, list(tc(paste0("rand_", i),
      list(acid_sample_wght = asw, acid_wght = aw, vol_overpressure = vop,
           sa_added = sa, co2_dry = co2, air_temp_c = at,
           h_co2_29815k = H, gas_const_r_mol = G, vial_volume = V, h3po4_added = P),
      calc_dic(asw, aw, vop, sa, co2, at, H, G, V, P), tolerance = 1e-6)))
  }
  cases
}

gen_d13c_dic <- function() {
  cases <- list()
  H <- 3.3e-4; G <- 8.314; V <- 12.0; P <- 0.1
  cases <- c(cases, list(
    tc("na", list(acid_sample_wght = 12.5, acid_wght = 10.0, vol_overpressure = 0.5,
                  delta_13co2 = NA, air_temp_c = 22.0,
                  h_co2_29815k = H, gas_const_r_mol = G, vial_volume = V, h3po4_added = P), NA_real_)
  ))
  for (i in seq_len(N)) {
    aw <- runif(1, 8, 12)
    asw <- aw + runif(1, 1, 6)
    vop <- runif(1, 0, 2)
    d13 <- maybe_na(runif(1, -25, 5))
    at <- runif(1, 15, 30)
    cases <- c(cases, list(tc(paste0("rand_", i),
      list(acid_sample_wght = asw, acid_wght = aw, vol_overpressure = vop,
           delta_13co2 = d13, air_temp_c = at,
           h_co2_29815k = H, gas_const_r_mol = G, vial_volume = V, h3po4_added = P),
      calc_d13c_dic(asw, aw, vop, d13, at, H, G, V, P), tolerance = 1e-6)))
  }
  cases
}

gen_dissolved_ch4 <- function() {
  cases <- list()
  KH <- 0.0014; TC <- 1750.0; CH4SA <- 1.9; GR <- 8.314
  cases <- c(cases, list(
    tc("na", list(ch4_dry = NA, water_temp_c = 10.0, bp_hpa = 850.0,
                  lab_temp_c = 22.0, lab_pressure_atm = 0.95,
                  kh_ch4 = KH, ch4_temp_const = TC, ch4_in_sa = CH4SA, gas_const_r_mol = GR), NA_real_)
  ))
  for (i in seq_len(N)) {
    ch4 <- maybe_na(runif(1, 500, 50000))
    wt <- runif(1, 0.5, 25)
    bp <- runif(1, 700, 1050)
    lt <- runif(1, 15, 30)
    lp <- runif(1, 0.9, 1.05)
    cases <- c(cases, list(tc(paste0("rand_", i),
      list(ch4_dry = ch4, water_temp_c = wt, bp_hpa = bp,
           lab_temp_c = lt, lab_pressure_atm = lp,
           kh_ch4 = KH, ch4_temp_const = TC, ch4_in_sa = CH4SA, gas_const_r_mol = GR),
      calc_dissolved_ch4(ch4, wt, bp, lt, lp, KH, TC, CH4SA, GR), tolerance = 1e-4)))
  }
  cases
}


# =============================================================================
# Assemble and write
# =============================================================================

cases <- list()
cases$common$mean <- gen_common_mean()
cases$common$std_dev <- gen_common_sd()
cases$common$minus <- gen_common_minus()
cases$common$equals <- gen_common_equals()
cases$common$ratio <- gen_common_ratio()
cases$tss_afdm$tss_mg_l <- gen_tss()
cases$tss_afdm$afdm_mg_l <- gen_afdm()
cases$dom$suva <- gen_suva()
cases$doc$doc_average <- gen_doc_avg()
cases$doc$doc_std_dev <- gen_doc_sd()
cases$chlorophyll$chla_acid <- gen_chla_acid()
cases$chlorophyll$chla_no_acid <- gen_chla_no_acid()
cases$benthic$rock_surface_area_m2 <- gen_rock_area()
cases$benthic$per_m2 <- gen_per_m2()
cases$benthic$benthic_afdm_per_m2 <- gen_benthic_afdm()
cases$field_data$barometric_pressure_from_altitude <- gen_baro_pressure()
cases$field_data$co2_correction <- gen_co2_correction()
cases$pco2$ch4_dry <- gen_ch4_dry()
cases$pco2$pco2_from_co2aq <- gen_pco2()
cases$pco2$pco2_p1 <- gen_pco2_p1()
cases$pco2$pco2_p2 <- gen_pco2_p2()
cases$pco2$dissolved_ch4 <- gen_dissolved_ch4()
cases$co2_air$co2_headspace <- gen_co2_headspace()
cases$dic$dic_concentration <- gen_dic()
cases$dic$d13c_dic <- gen_d13c_dic()

output <- list(
  metadata = list(
    generator = "r_reference/generate_fixtures.R",
    r_version = paste(R.version$major, R.version$minor, sep = "."),
    generated_at = format(Sys.time(), "%Y-%m-%dT%H:%M:%SZ", tz = "UTC"),
    seed = 42,
    cases_per_function = N
  ),
  modules = cases
)

args <- commandArgs(trailingOnly = FALSE)
script_path <- sub("--file=", "", args[grep("--file=", args)])
if (length(script_path) == 0) script_path <- "r_reference/generate_fixtures.R"
script_dir <- dirname(script_path)
output_path <- file.path(script_dir, "..", "tests", "fixtures", "golden_values.json")
dir.create(dirname(output_path), showWarnings = FALSE, recursive = TRUE)

json_text <- toJSON(output, auto_unbox = TRUE, pretty = TRUE, na = "null", digits = 17)
writeLines(json_text, output_path)

total_cases <- sum(sapply(cases, function(m) sum(sapply(m, length))))
cat("Generated", output_path, "\n")
cat("Modules:", paste(names(cases), collapse = ", "), "\n")
cat("Total test cases:", total_cases, "\n")
