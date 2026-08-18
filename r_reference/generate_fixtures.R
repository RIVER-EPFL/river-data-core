#!/usr/bin/env Rscript
#
# Golden value fixture generator for river-data-core toolbox tests.
#
# Sources the verbatim portal functions from r_reference/functions/*.R
# (verified against the portal source by verify_integrity.R) and drives
# them through 1-row data frames plus a mocked `getRows`/pool, exactly as
# the portal does. Constants default to the portal production dump values.
#
# The portal functions need dplyr/tidyr/magrittr/bigleaf; this script
# provides minimal shims covering the call patterns they use, so only
# base R + jsonlite are required.
#
# Run: Rscript r_reference/generate_fixtures.R

library(jsonlite)
set.seed(42)

N <- 200  # random cases per scalar function
N_MAP <- 50  # random cases per composite (map) function

# =============================================================================
# Shims for the portal functions' dplyr/tidyr/magrittr/bigleaf usage
# =============================================================================

`%>%` <- function(lhs, rhs) {
  rhs_expr <- substitute(rhs)
  if (is.call(rhs_expr)) {
    new_call <- as.call(c(rhs_expr[[1]], quote(.LHS.), as.list(rhs_expr)[-1]))
  } else {
    new_call <- as.call(list(rhs_expr, quote(.LHS.)))
  }
  env <- new.env(parent = parent.frame())
  assign(".LHS.", lhs, envir = env)
  eval(new_call, env)
}

`%<>%` <- function(lhs, rhs) {
  lhs_expr <- substitute(lhs)
  rhs_expr <- substitute(rhs)
  new_call <- as.call(c(rhs_expr[[1]], lhs_expr, as.list(rhs_expr)[-1]))
  result <- eval(new_call, parent.frame())
  assign(deparse(lhs_expr), result, envir = parent.frame())
  invisible(result)
}

pull <- function(df, var) {
  if (missing(var)) return(df[[ncol(df)]])
  v <- substitute(var)
  if (is.numeric(v)) return(df[[v]])
  if (is.character(v)) return(df[[v]])
  resolved <- tryCatch(eval(v, parent.frame()), error = function(e) NULL)
  if (is.character(resolved) || is.numeric(resolved)) return(df[[resolved]])
  df[[deparse(v)]]
}

select <- function(df, expr) {
  e <- substitute(expr)
  if (is.call(e) && identical(e[[1]], as.name("-"))) {
    return(df[, setdiff(names(df), deparse(e[[2]])), drop = FALSE])
  }
  helpers <- list(
    starts_with = function(p) names(df)[startsWith(names(df), p)],
    matches = function(p) names(df)[grepl(p, names(df))]
  )
  cols <- eval(e, helpers, enclos = parent.frame())
  df[, cols, drop = FALSE]
}

filter <- function(df, cond) {
  keep <- eval(substitute(cond), df, enclos = parent.frame())
  df[keep, , drop = FALSE]
}

arrange <- function(df, col) {
  nm <- deparse(substitute(col))
  df[order(df[[nm]]), , drop = FALSE]
}

mutate <- function(df, expr) {
  e <- substitute(expr)
  stopifnot(is.call(e), identical(e[[1]], as.name("across")))
  fml <- eval(e[[3]], parent.frame())
  fml_env <- environment(fml)
  for (nm in names(df)) {
    df[[nm]] <- eval(fml[[2]], list(.x = df[[nm]]), enclos = fml_env)
  }
  df
}

pivot_longer <- function(df, cols) {
  data.frame(
    name = names(df),
    value = as.numeric(unlist(df[1, ], use.names = FALSE)),
    stringsAsFactors = FALSE
  )
}

pressure.from.elevation <- function(elev, Tair) {
  # bigleaf::pressure.from.elevation without VPD:
  # pressure0 / exp(g * elev / (Rd * Tair_K)), kPa
  101.325 / exp(9.81 * elev / (287.0586 * (Tair + 273.15)))
}

`::` <- function(pkg, name) {
  pkg <- as.character(substitute(pkg))
  name <- as.character(substitute(name))
  if (pkg == "tidyr" && name == "pivot_longer") return(pivot_longer)
  if (pkg == "bigleaf" && name == "pressure.from.elevation") return(pressure.from.elevation)
  getExportedValue(pkg, name)
}

# =============================================================================
# Mock database (pool + getRows)
# =============================================================================

MOCK <- new.env()
pool <- NULL

getRows <- function(pool, table, ..., columns = NULL) {
  df <- MOCK[[table]]
  conds <- as.list(substitute(list(...)))[-1]
  for (cond in conds) {
    keep <- eval(cond, df, enclos = parent.frame())
    df <- df[keep, , drop = FALSE]
  }
  if (!is.null(columns)) df <- df[, columns, drop = FALSE]
  df
}

# Portal production constants (cnet_db_prod.sql dump)
PORTAL_CONSTANTS <- c(
  gas_const_r_atm = 0.0820574,
  h_co2_29815k = 0.034733,
  c_const = 2400,
  vol_sa = 0.03,
  vol_water = 0.03,
  lab_press_avg_atm = 0.957237,
  lab_temp_avg_degC = 22.5,
  h_ch4_29815k = 0.00213,
  ch4_in_sa = 0.000002,
  gas_const_r_mol = 8.31446,
  vial_volume = 12.168,
  h3po4_added = 0.3
)

set_constants <- function(overrides = c()) {
  vals <- PORTAL_CONSTANTS
  vals[names(overrides)] <- overrides
  MOCK$constants <- data.frame(
    name = names(vals),
    value = as.numeric(vals),
    stringsAsFactors = FALSE
  )
}

set_std_curve <- function(a, b) {
  MOCK$standard_curves <- data.frame(id = 1, a = a, b = b)
}

set_station <- function(elevation) {
  MOCK$stations <- data.frame(
    name = "S1", order = 1, elevation = elevation,
    stringsAsFactors = FALSE
  )
}

set_constants()
set_std_curve(1, 0)
set_station(500)

# =============================================================================
# Source the verbatim portal functions
# =============================================================================

args_all <- commandArgs(trailingOnly = FALSE)
script_path <- sub("--file=", "", args_all[grep("--file=", args_all)])
if (length(script_path) == 0) script_path <- "r_reference/generate_fixtures.R"
script_dir <- dirname(script_path)

for (f in list.files(file.path(script_dir, "functions"), pattern = "\\.R$", full.names = TRUE)) {
  source(f)
}

# =============================================================================
# Helpers
# =============================================================================

df1 <- function(...) data.frame(..., stringsAsFactors = FALSE, check.names = FALSE)

# Portal functions signal "do not update" with the string 'KEEP OLD';
# fixtures encode both that and NA as null.
num <- function(x) {
  if (length(x) != 1 || is.character(x) || is.na(x)) return(NA_real_)
  as.numeric(x)
}

tc <- function(name, inputs, expected, tolerance = 1e-9) {
  list(name = name, inputs = inputs, expected = num(expected), tolerance = tolerance)
}

tcm <- function(name, inputs, expected_map, tolerance = 1e-9) {
  list(name = name, inputs = inputs, expected_map = lapply(expected_map, num), tolerance = tolerance)
}

maybe_na <- function(val, prob = 0.05) {
  if (runif(1) < prob) NA_real_ else val
}

rep_df <- function(values, prefix) {
  df <- as.data.frame(as.list(values))
  names(df) <- paste0(prefix, seq_along(values))
  df
}

# =============================================================================
# Portal-call wrappers keyed by fixture input shape
# =============================================================================

r_mean <- function(values) calcMean(rep_df(values, "v"))
r_sd <- function(values) calcSd(rep_df(values, "v"))
r_minus <- function(a, b) calcMinus(df1(col1 = a, col2 = b))
r_equals <- function(primary, fallback) calcEquals(df1(col1 = primary, col2 = fallback))
r_ratio <- function(dividend, divisor) calcRatio(df1(col1 = dividend, col2 = divisor))

r_tss <- function(wgt_dried, wgt_prefilt, vol_filtered) {
  calcTSS(df1(
    lab_tss_wgt_samp_filt_dried = wgt_dried,
    lab_tss_wgt_filt_prefiltr = wgt_prefilt,
    lab_tss_vol_filtered = vol_filtered
  ))
}

r_afdm <- function(wgt_dried, wgt_ashed, vol_filtered) {
  calcAFDM(df1(
    lab_tss_wgt_samp_filt_dried = wgt_dried,
    lab_tss_wgt_samp_filt_ashed = wgt_ashed,
    lab_tss_vol_filtered = vol_filtered
  ))
}

r_suva <- function(a254, doc_avg_ppb) calcSUVA(df1(a254 = a254, DOC_avg_ppb = doc_avg_ppb))

r_doc <- function(fn, replicates, slope, intercept) {
  stopifnot(length(replicates) == 3)
  if (!is.na(slope) && !is.na(intercept)) {
    set_std_curve(slope, intercept)
    curve_id <- 1
  } else {
    curve_id <- NA_real_
  }
  df <- df1(
    DOC_rep_1 = replicates[1], DOC_rep_2 = replicates[2], DOC_rep_3 = replicates[3],
    doc_std_curve_id = curve_id
  )
  fn(df, pool)
}

r_chla_acid <- function(fluor_before, fluor_after, slope, intercept) {
  if (!is.na(slope) && !is.na(intercept)) {
    set_std_curve(slope, intercept)
    curve_id <- 1
  } else {
    curve_id <- NA_real_
  }
  calcChlaAcid(df1(
    lab_chla_fluor_1_rep_1 = fluor_before,
    lab_chla_fluor_2_rep_1 = fluor_after,
    chla_acid_std_curve_id = curve_id
  ), pool)
}

r_chla_no_acid <- function(fluor, slope, intercept) {
  if (!is.na(slope) && !is.na(intercept)) {
    set_std_curve(slope, intercept)
    curve_id <- 1
  } else {
    curve_id <- NA_real_
  }
  calcChlaNoAcid(df1(
    lab_chla_fluor_1_rep_1 = fluor,
    chla_noacid_std_curve_id = curve_id
  ), pool)
}

r_rock_area <- function(dims_cm) {
  1 / convertToUnitPerM2(1, dims_cm, 1, 1)
}

r_per_m2 <- function(sample_value, dims_cm, vol_filtrated, vol_total) {
  convertToUnitPerM2(sample_value, dims_cm, vol_filtrated, vol_total)
}

r_benthic_afdm <- function(afdm_g, dims_cm, vol_filtrated, vol_total) {
  calcBenthicAFDM(df1(
    lab_chla_sizeA_rep_1 = dims_cm[1],
    lab_chla_sizeB_rep_1 = dims_cm[2],
    lab_chla_sizeC_rep_1 = dims_cm[3],
    lab_chla_tot_vol_rep_1 = vol_total,
    lab_chla_vol_filtrated_rep_1 = vol_filtrated,
    afdm_g_filter_rep_1 = afdm_g
  ))
}

r_benthic_chla <- function(chla_ug_l, dims_cm, vol_filtrated, vol_total) {
  calcChlaPerM2(df1(
    lab_chla_sizeA_rep_1 = dims_cm[1],
    lab_chla_sizeB_rep_1 = dims_cm[2],
    lab_chla_sizeC_rep_1 = dims_cm[3],
    lab_chla_tot_vol_rep_1 = vol_total,
    lab_chla_vol_filtrated_rep_1 = vol_filtrated,
    chla_acid_ugL_rep_1 = chla_ug_l
  ))
}

r_baro <- function(elevation_m, temp_c) {
  set_station(elevation_m)
  calcAlt2BP(df1(station = "S1", WTW_Temp_degC_1 = temp_c), pool)
}

r_co2_correction <- function(raw_co2, temp_c, pressure_hpa, std_slope, std_intercept) {
  if (!is.na(std_slope) && !is.na(std_intercept)) {
    set_std_curve(std_slope, std_intercept)
    curve_id <- 1
  } else {
    curve_id <- NA_real_
  }
  calcCO2corr(df1(
    Vaisala_CO2_avg = raw_co2,
    WTW_Temp_degC_1 = temp_c,
    Field_BP = NA_real_,
    Field_BP_altitude = pressure_hpa,
    vaisala_std_curve_id = curve_id
  ), pool)
}

# Pressure selection rule as written in calcCO2corr/calcpCO2P1/calcpCO2P2/calcCH4
r_select_pressure <- function(field_pressure, altitude_pressure) {
  if (!is.na(field_pressure) & field_pressure <= 1050 & field_pressure >= 700) {
    field_pressure
  } else {
    altitude_pressure
  }
}

r_ch4_dry <- function(ch4_wet, h2o_percent) {
  calcCH4dry(df1(lab_co2_h2o = h2o_percent, lab_co2_ch4 = ch4_wet))
}

r_pco2 <- function(co2_aq, water_temp_c, c_const) {
  set_constants(c(c_const = c_const))
  calcpCO2(df1(WTW_Temp_degC_1 = water_temp_c, CO2_HS_Um = co2_aq), pool)
}

r_pco2_p1 <- function(co2_aq, water_temp_c, bp_hpa, c_const) {
  set_constants(c(c_const = c_const))
  calcpCO2P1(df1(
    WTW_Temp_degC_1 = water_temp_c,
    Field_BP = NA_real_,
    Field_BP_altitude = bp_hpa,
    CO2_HS_Um = co2_aq
  ), pool)
}

r_pco2_p2 <- function(co2_aq, water_temp_c, bp_hpa, c_const) {
  set_constants(c(c_const = c_const))
  calcpCO2P2(df1(
    WTW_Temp_degC_1 = water_temp_c,
    Field_BP = NA_real_,
    Field_BP_altitude = bp_hpa,
    CO2_HS_Um = co2_aq
  ), pool)
}

r_dissolved_ch4 <- function(ch4_dry, water_temp_c, bp_hpa, lab_temp_c,
                            h_ch4_29815k, ch4_in_sa, gas_const_r_mol) {
  set_constants(c(
    h_ch4_29815k = h_ch4_29815k,
    ch4_in_sa = ch4_in_sa,
    gas_const_r_mol = gas_const_r_mol
  ))
  calcCH4(df1(
    WTW_Temp_degC_1 = water_temp_c,
    Field_BP = NA_real_,
    Field_BP_altitude = bp_hpa,
    lab_co2_lab_temp = lab_temp_c,
    lab_co2_lab_press = NA_real_,
    lab_co2_ch4_dry = ch4_dry
  ), pool)
}

r_co2_headspace <- function(co2_ppm, lab_temp_c, lab_pressure_atm,
                            vol_sa, vol_water, c_const, gas_const_r_atm) {
  set_constants(c(
    vol_sa = vol_sa,
    vol_water = vol_water,
    c_const = c_const,
    gas_const_r_atm = gas_const_r_atm
  ))
  calcCO2(df1(
    lab_co2_lab_temp = lab_temp_c,
    lab_co2_lab_press = lab_pressure_atm * 1013.25,
    lab_co2_co2ppm = co2_ppm
  ), pool)
}

r_dic <- function(acid_sample_wght, acid_wght, vol_overpressure, sa_added,
                  co2_dry, air_temp_c,
                  h_co2_29815k, gas_const_r_mol, vial_volume, h3po4_added) {
  set_constants(c(
    h_co2_29815k = h_co2_29815k,
    gas_const_r_mol = gas_const_r_mol,
    vial_volume = vial_volume,
    h3po4_added = h3po4_added
  ))
  calcDIC(df1(
    lab_dic_air_temp = air_temp_c,
    lab_dic_acid_sample_wght = acid_sample_wght,
    lab_dic_acid_wght = acid_wght,
    lab_dic_vol_overpressure = vol_overpressure,
    lab_dic_SA_added = sa_added,
    lab_dic_co2_dry = co2_dry
  ), pool)
}

r_d13c_dic <- function(acid_sample_wght, acid_wght, vol_overpressure,
                       delta_13co2, air_temp_c,
                       h_co2_29815k, gas_const_r_mol, vial_volume, h3po4_added) {
  set_constants(c(
    h_co2_29815k = h_co2_29815k,
    gas_const_r_mol = gas_const_r_mol,
    vial_volume = vial_volume,
    h3po4_added = h3po4_added
  ))
  calcd13DIC(df1(
    lab_dic_air_temp = air_temp_c,
    lab_dic_acid_sample_wght = acid_sample_wght,
    lab_dic_acid_wght = acid_wght,
    lab_dic_vol_overpressure = vol_overpressure,
    lab_dic_delta_13co2 = delta_13co2
  ), pool)
}

# =============================================================================
# Case generators
# =============================================================================

gen_common_mean <- function() {
  cases <- list(
    tc("single", list(values = c(42.0)), r_mean(42.0)),
    tc("two_equal", list(values = c(5.0, 5.0)), r_mean(c(5, 5))),
    tc("all_na", list(values = c(NA, NA)), r_mean(c(NA_real_, NA_real_))),
    tc("one_na", list(values = c(1.0, NA, 3.0)), r_mean(c(1, NA, 3))),
    tc("negative", list(values = c(-10.0, 10.0)), r_mean(c(-10, 10)))
  )
  for (i in seq_len(N)) {
    vals <- runif(sample(2:8, 1), -100, 500)
    if (runif(1) < 0.1) vals[sample(length(vals), 1)] <- NA
    cases <- c(cases, list(tc(paste0("rand_", i), list(values = vals), r_mean(vals))))
  }
  cases
}

gen_common_sd <- function() {
  cases <- list(
    tc("identical", list(values = c(3.0, 3.0, 3.0)), r_sd(c(3, 3, 3))),
    tc("two_vals", list(values = c(1.0, 3.0)), r_sd(c(1, 3))),
    tc("single", list(values = c(5.0)), r_sd(5.0)),
    tc("one_na_of_two", list(values = c(5.0, NA)), r_sd(c(5, NA)))
  )
  for (i in seq_len(N)) {
    vals <- runif(sample(2:8, 1), -100, 500)
    if (runif(1) < 0.1) vals[sample(length(vals), 1)] <- NA
    cases <- c(cases, list(tc(paste0("rand_", i), list(values = vals), r_sd(vals))))
  }
  cases
}

gen_common_minus <- function() {
  cases <- list(
    tc("zero", list(a = 5.0, b = 5.0), r_minus(5, 5)),
    tc("a_na", list(a = NA, b = 3.0), r_minus(NA_real_, 3))
  )
  for (i in seq_len(N)) {
    a <- maybe_na(runif(1, -500, 500))
    b <- maybe_na(runif(1, -500, 500))
    cases <- c(cases, list(tc(paste0("rand_", i), list(a = a, b = b), r_minus(a, b))))
  }
  cases
}

gen_common_equals <- function() {
  cases <- list(
    tc("primary_valid", list(primary = 42.0, fallback = 99.0), r_equals(42, 99)),
    tc("primary_na", list(primary = NA, fallback = 99.0), r_equals(NA_real_, 99)),
    tc("both_na", list(primary = NA, fallback = NA), r_equals(NA_real_, NA_real_))
  )
  for (i in seq_len(N)) {
    p <- maybe_na(runif(1, -100, 500), 0.3)
    f <- maybe_na(runif(1, -100, 500), 0.1)
    cases <- c(cases, list(tc(paste0("rand_", i), list(primary = p, fallback = f), r_equals(p, f))))
  }
  cases
}

gen_common_ratio <- function() {
  cases <- list(
    tc("normal", list(dividend = 10.0, divisor = 3.0), r_ratio(10, 3)),
    tc("zero_div", list(dividend = 10.0, divisor = 0.0), r_ratio(10, 0)),
    tc("na", list(dividend = NA, divisor = 2.0), r_ratio(NA_real_, 2))
  )
  for (i in seq_len(N)) {
    a <- maybe_na(runif(1, -500, 500))
    b <- maybe_na(runif(1, -500, 500))
    cases <- c(cases, list(tc(paste0("rand_", i), list(dividend = a, divisor = b), r_ratio(a, b))))
  }
  cases
}

gen_tss <- function() {
  cases <- list(
    tc("clean", list(wgt_dried = 0.1005, wgt_prefilt = 0.1, vol_filtered = 500.0),
       r_tss(0.1005, 0.1, 500)),
    tc("na_wgt", list(wgt_dried = NA, wgt_prefilt = 0.1, vol_filtered = 500.0),
       r_tss(NA_real_, 0.1, 500))
  )
  for (i in seq_len(N)) {
    prefilt <- runif(1, 0.05, 0.15)
    dried <- prefilt + runif(1, -0.005, 0.05)
    vol <- maybe_na(runif(1, 50, 1000))
    cases <- c(cases, list(tc(paste0("rand_", i),
      list(wgt_dried = dried, wgt_prefilt = prefilt, vol_filtered = vol),
      r_tss(dried, prefilt, vol))))
  }
  cases
}

gen_afdm <- function() {
  cases <- list(
    tc("normal", list(wgt_dried = 0.1025, wgt_ashed = 0.1005, vol_filtered = 500.0),
       r_afdm(0.1025, 0.1005, 500)),
    tc("na", list(wgt_dried = NA, wgt_ashed = 0.1005, vol_filtered = 500.0),
       r_afdm(NA_real_, 0.1005, 500))
  )
  for (i in seq_len(N)) {
    dried <- runif(1, 0.05, 0.25)
    ashed <- dried - runif(1, 0, dried * 0.3)
    vol <- maybe_na(runif(1, 50, 1000))
    cases <- c(cases, list(tc(paste0("rand_", i),
      list(wgt_dried = dried, wgt_ashed = ashed, vol_filtered = vol),
      r_afdm(dried, ashed, vol))))
  }
  cases
}

gen_suva <- function() {
  cases <- list(
    tc("typical", list(a254 = 0.15, doc_avg_ppb = 2500.0), r_suva(0.15, 2500)),
    tc("na", list(a254 = NA, doc_avg_ppb = 2500.0), r_suva(NA_real_, 2500))
  )
  for (i in seq_len(N)) {
    a <- maybe_na(runif(1, 0.01, 0.5))
    d <- maybe_na(runif(1, 100, 10000))
    cases <- c(cases, list(tc(paste0("rand_", i), list(a254 = a, doc_avg_ppb = d), r_suva(a, d))))
  }
  cases
}

gen_doc <- function(fn) {
  cases <- list(
    tc("no_curve", list(replicates = c(120.0, 125.0, 118.0), slope = NA, intercept = NA),
       r_doc(fn, c(120, 125, 118), NA_real_, NA_real_)),
    tc("with_curve", list(replicates = c(120.0, 125.0, 118.0), slope = 1.05, intercept = -2.0),
       r_doc(fn, c(120, 125, 118), 1.05, -2)),
    tc("all_na", list(replicates = c(NA, NA, NA), slope = NA, intercept = NA),
       r_doc(fn, c(NA_real_, NA_real_, NA_real_), NA_real_, NA_real_)),
    tc("one_na", list(replicates = c(120.0, NA, 118.0), slope = NA, intercept = NA),
       r_doc(fn, c(120, NA, 118), NA_real_, NA_real_))
  )
  for (i in seq_len(N)) {
    reps <- runif(3, 50, 500)
    if (runif(1) < 0.1) reps[sample(3, 1)] <- NA
    use_curve <- runif(1) < 0.5
    sl <- if (use_curve) runif(1, 0.9, 1.2) else NA_real_
    int <- if (use_curve) runif(1, -5, 5) else NA_real_
    cases <- c(cases, list(tc(paste0("rand_", i),
      list(replicates = reps, slope = sl, intercept = int),
      r_doc(fn, reps, sl, int))))
  }
  cases
}

gen_chla_acid <- function() {
  cases <- list(
    tc("typical", list(fluor_before = 150.0, fluor_after = 80.0, slope = 0.25, intercept = -1.5),
       r_chla_acid(150, 80, 0.25, -1.5)),
    tc("no_curve", list(fluor_before = 150.0, fluor_after = 80.0, slope = NA, intercept = NA),
       r_chla_acid(150, 80, NA_real_, NA_real_))
  )
  for (i in seq_len(N)) {
    fb <- maybe_na(runif(1, 50, 300))
    fa <- maybe_na(runif(1, 20, 200))
    sl <- runif(1, 0.1, 0.6)
    int <- runif(1, -3, 2)
    cases <- c(cases, list(tc(paste0("rand_", i),
      list(fluor_before = fb, fluor_after = fa, slope = sl, intercept = int),
      r_chla_acid(fb, fa, sl, int))))
  }
  cases
}

gen_chla_no_acid <- function() {
  cases <- list(
    tc("typical", list(fluor = 150.0, slope = 0.30, intercept = -2.0),
       r_chla_no_acid(150, 0.3, -2)),
    tc("no_curve", list(fluor = 150.0, slope = NA, intercept = NA),
       r_chla_no_acid(150, NA_real_, NA_real_))
  )
  for (i in seq_len(N)) {
    fl <- maybe_na(runif(1, 50, 300))
    sl <- runif(1, 0.1, 0.6)
    int <- runif(1, -3, 2)
    cases <- c(cases, list(tc(paste0("rand_", i),
      list(fluor = fl, slope = sl, intercept = int),
      r_chla_no_acid(fl, sl, int))))
  }
  cases
}

gen_rock_area <- function() {
  cases <- list(
    tc("sphere_10cm", list(dims_cm = c(10.0, 10.0, 10.0)), r_rock_area(c(10, 10, 10))),
    tc("flat_rock", list(dims_cm = c(20.0, 15.0, 3.0)), r_rock_area(c(20, 15, 3))),
    tc("two_dims", list(dims_cm = c(12.0, 8.0)), r_rock_area(c(12, 8)))
  )
  for (i in seq_len(N)) {
    dims <- runif(3, 3, 50)
    cases <- c(cases, list(tc(paste0("rand_", i), list(dims_cm = dims), r_rock_area(dims))))
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
      r_per_m2(sv, dims, vf, vt))))
  }
  cases
}

gen_benthic_afdm <- function() {
  cases <- list(
    tc("na_afdm", list(afdm_g = NA, dims_cm = c(10, 8, 6), vol_filtrated = 50.0, vol_total = 100.0),
       r_benthic_afdm(NA_real_, c(10, 8, 6), 50, 100))
  )
  for (i in seq_len(N)) {
    ag <- runif(1, 0.001, 0.01)
    dims <- runif(3, 3, 50)
    vf <- runif(1, 10, 200)
    vt <- runif(1, 50, 500)
    cases <- c(cases, list(tc(paste0("rand_", i),
      list(afdm_g = ag, dims_cm = dims, vol_filtrated = vf, vol_total = vt),
      r_benthic_afdm(ag, dims, vf, vt))))
  }
  cases
}

gen_benthic_chla <- function() {
  cases <- list(
    tc("na_chla", list(chla_ug_l = NA, dims_cm = c(10, 8, 6), vol_filtrated = 50.0, vol_total = 100.0),
       r_benthic_chla(NA_real_, c(10, 8, 6), 50, 100))
  )
  for (i in seq_len(N)) {
    ch <- runif(1, 1, 100)
    dims <- runif(3, 3, 50)
    vf <- runif(1, 10, 200)
    vt <- runif(1, 50, 500)
    cases <- c(cases, list(tc(paste0("rand_", i),
      list(chla_ug_l = ch, dims_cm = dims, vol_filtrated = vf, vol_total = vt),
      r_benthic_chla(ch, dims, vf, vt))))
  }
  cases
}

gen_baro <- function() {
  cases <- list(
    tc("sea_level_15c", list(elevation_m = 0.0, temp_c = 15.0), r_baro(0, 15)),
    tc("martigny_470m", list(elevation_m = 470.0, temp_c = 15.0), r_baro(470, 15)),
    tc("verbier_1500m", list(elevation_m = 1500.0, temp_c = 8.0), r_baro(1500, 8)),
    tc("mont_blanc_4808m", list(elevation_m = 4808.0, temp_c = -10.0), r_baro(4808, -10)),
    tc("na_temp", list(elevation_m = 1000.0, temp_c = NA), r_baro(1000, NA_real_))
  )
  for (i in seq_len(N)) {
    el <- runif(1, 0, 4500)
    t <- runif(1, -15, 35)
    cases <- c(cases, list(tc(paste0("rand_", i),
      list(elevation_m = el, temp_c = t), r_baro(el, t))))
  }
  cases
}

gen_co2_correction <- function() {
  cases <- list(
    tc("ref_conditions", list(raw_co2 = 1013.0, temp_c = 25.0, pressure_hpa = 1013.0,
                              std_slope = NA, std_intercept = NA),
       r_co2_correction(1013, 25, 1013, NA_real_, NA_real_)),
    tc("na_temp", list(raw_co2 = 500.0, temp_c = NA, pressure_hpa = 850.0,
                       std_slope = NA, std_intercept = NA),
       r_co2_correction(500, NA_real_, 850, NA_real_, NA_real_))
  )
  for (i in seq_len(N)) {
    co2 <- runif(1, 100, 5000)
    t <- maybe_na(runif(1, 0.5, 25))
    bp <- runif(1, 700, 1050)
    use_curve <- runif(1) < 0.5
    sl <- if (use_curve) runif(1, 0.9, 1.2) else NA_real_
    int <- if (use_curve) runif(1, -10, 10) else NA_real_
    cases <- c(cases, list(tc(paste0("rand_", i),
      list(raw_co2 = co2, temp_c = t, pressure_hpa = bp, std_slope = sl, std_intercept = int),
      r_co2_correction(co2, t, bp, sl, int))))
  }
  cases
}

gen_reach_depths <- function() {
  cases <- list()
  for (i in seq_len(N_MAP)) {
    depths <- runif(sample(1:6, 1), 2, 80)
    if (runif(1) < 0.1) depths[sample(length(depths), 1)] <- NA
    cases <- c(cases, list(tcm(paste0("rand_", i),
      list(depths = depths),
      list(avg = r_mean(depths), sd = r_sd(depths)))))
  }
  cases
}

gen_select_pressure <- function() {
  cases <- list(
    tc("field_in_range", list(field_pressure = 950.0, altitude_pressure = 800.0),
       r_select_pressure(950, 800)),
    tc("field_too_low", list(field_pressure = 600.0, altitude_pressure = 800.0),
       r_select_pressure(600, 800)),
    tc("field_too_high", list(field_pressure = 1100.0, altitude_pressure = 800.0),
       r_select_pressure(1100, 800)),
    tc("field_na", list(field_pressure = NA, altitude_pressure = 800.0),
       r_select_pressure(NA_real_, 800)),
    tc("both_na", list(field_pressure = NA, altitude_pressure = NA),
       r_select_pressure(NA_real_, NA_real_)),
    tc("boundary_700", list(field_pressure = 700.0, altitude_pressure = 800.0),
       r_select_pressure(700, 800)),
    tc("boundary_1050", list(field_pressure = 1050.0, altitude_pressure = 800.0),
       r_select_pressure(1050, 800))
  )
  for (i in seq_len(N_MAP)) {
    fp <- maybe_na(runif(1, 500, 1200), 0.2)
    ap <- maybe_na(runif(1, 700, 1050), 0.1)
    cases <- c(cases, list(tc(paste0("rand_", i),
      list(field_pressure = fp, altitude_pressure = ap), r_select_pressure(fp, ap))))
  }
  cases
}

gen_ch4_dry <- function() {
  cases <- list(
    tc("typical", list(ch4_wet = 2000.0, h2o_percent = 1.5), r_ch4_dry(2000, 1.5)),
    tc("zero_h2o", list(ch4_wet = 2000.0, h2o_percent = 0.0), r_ch4_dry(2000, 0)),
    tc("na", list(ch4_wet = NA, h2o_percent = 1.5), r_ch4_dry(NA_real_, 1.5))
  )
  for (i in seq_len(N)) {
    ch4 <- maybe_na(runif(1, 1, 500))
    h2o <- runif(1, 0, 3.5)
    cases <- c(cases, list(tc(paste0("rand_", i),
      list(ch4_wet = ch4, h2o_percent = h2o), r_ch4_dry(ch4, h2o))))
  }
  cases
}

gen_pco2 <- function() {
  C <- PORTAL_CONSTANTS[["c_const"]]
  cases <- list(
    tc("ref_25c", list(co2_aq = 50.0, water_temp_c = 25.0, c_const = C), r_pco2(50, 25, C)),
    tc("na_temp", list(co2_aq = 50.0, water_temp_c = NA, c_const = C), r_pco2(50, NA_real_, C))
  )
  for (i in seq_len(N)) {
    co2 <- maybe_na(runif(1, 5, 500))
    t <- runif(1, 0.5, 25)
    cases <- c(cases, list(tc(paste0("rand_", i),
      list(co2_aq = co2, water_temp_c = t, c_const = C), r_pco2(co2, t, C))))
  }
  cases
}

gen_pco2_p1 <- function() {
  C <- PORTAL_CONSTANTS[["c_const"]]
  cases <- list(
    tc("sea_level", list(co2_aq = 50.0, water_temp_c = 15.0, bp_hpa = 1013.25, c_const = C),
       r_pco2_p1(50, 15, 1013.25, C)),
    tc("na_bp", list(co2_aq = 50.0, water_temp_c = 10.0, bp_hpa = NA, c_const = C),
       r_pco2_p1(50, 10, NA_real_, C))
  )
  for (i in seq_len(N)) {
    co2 <- maybe_na(runif(1, 5, 500))
    t <- runif(1, 0.5, 25)
    bp <- runif(1, 700, 1050)
    cases <- c(cases, list(tc(paste0("rand_", i),
      list(co2_aq = co2, water_temp_c = t, bp_hpa = bp, c_const = C),
      r_pco2_p1(co2, t, bp, C))))
  }
  cases
}

gen_pco2_p2 <- function() {
  C <- PORTAL_CONSTANTS[["c_const"]]
  cases <- list()
  for (i in seq_len(N)) {
    co2 <- runif(1, 5, 500)
    t <- runif(1, 0.5, 25)
    bp <- runif(1, 700, 1050)
    cases <- c(cases, list(tc(paste0("rand_", i),
      list(co2_aq = co2, water_temp_c = t, bp_hpa = bp, c_const = C),
      r_pco2_p2(co2, t, bp, C))))
  }
  cases
}

gen_dissolved_ch4 <- function() {
  KH <- PORTAL_CONSTANTS[["h_ch4_29815k"]]
  SA <- PORTAL_CONSTANTS[["ch4_in_sa"]]
  GR <- PORTAL_CONSTANTS[["gas_const_r_mol"]]
  cases <- list(
    tc("na_ch4", list(ch4_dry = NA, water_temp_c = 10.0, bp_hpa = 850.0, lab_temp_c = 22.0,
                      h_ch4_29815k = KH, ch4_in_sa = SA, gas_const_r_mol = GR),
       r_dissolved_ch4(NA_real_, 10, 850, 22, KH, SA, GR))
  )
  for (i in seq_len(N)) {
    ch4 <- maybe_na(runif(1, 1, 500))
    wt <- runif(1, 0.5, 25)
    bp <- runif(1, 700, 1050)
    lt <- runif(1, 15, 30)
    cases <- c(cases, list(tc(paste0("rand_", i),
      list(ch4_dry = ch4, water_temp_c = wt, bp_hpa = bp, lab_temp_c = lt,
           h_ch4_29815k = KH, ch4_in_sa = SA, gas_const_r_mol = GR),
      r_dissolved_ch4(ch4, wt, bp, lt, KH, SA, GR))))
  }
  cases
}

gen_co2_headspace <- function() {
  C <- PORTAL_CONSTANTS[["c_const"]]
  R <- PORTAL_CONSTANTS[["gas_const_r_atm"]]
  cases <- list(
    tc("portal_volumes", list(co2_ppm = 3000.0, lab_temp_c = 22.5, lab_pressure_atm = 0.957237,
                              vol_sa_ml = 0.03, vol_water_ml = 0.03, c_const = C, gas_const_r_atm = R),
       r_co2_headspace(3000, 22.5, 0.957237, 0.03, 0.03, C, R))
  )
  for (i in seq_len(N)) {
    ppm <- runif(1, 100, 10000)
    lt <- runif(1, 15, 30)
    lp <- runif(1, 0.9, 1.05)
    vs <- runif(1, 0.02, 0.08)
    vw <- runif(1, 0.02, 0.06)
    cases <- c(cases, list(tc(paste0("rand_", i),
      list(co2_ppm = ppm, lab_temp_c = lt, lab_pressure_atm = lp,
           vol_sa_ml = vs, vol_water_ml = vw, c_const = C, gas_const_r_atm = R),
      r_co2_headspace(ppm, lt, lp, vs, vw, C, R))))
  }
  cases
}

gen_dic <- function() {
  H <- PORTAL_CONSTANTS[["h_co2_29815k"]]
  G <- PORTAL_CONSTANTS[["gas_const_r_mol"]]
  V <- PORTAL_CONSTANTS[["vial_volume"]]
  P <- PORTAL_CONSTANTS[["h3po4_added"]]
  cases <- list(
    tc("lab_temp_from_constant",
       list(acid_sample_wght = 11.5, acid_wght = 9.5, vol_overpressure = 0.5, sa_added = 0.3,
            co2_dry = 2000.0, air_temp_c = PORTAL_CONSTANTS[["lab_temp_avg_degC"]],
            h_co2_29815k = H, gas_const_r_mol = G, vial_volume = V, h3po4_added = P),
       r_dic(11.5, 9.5, 0.5, 0.3, 2000, NA_real_, H, G, V, P)),
    tc("na_co2", list(acid_sample_wght = 11.5, acid_wght = 9.5, vol_overpressure = 0.5,
                      sa_added = 0.3, co2_dry = NA, air_temp_c = 22.0,
                      h_co2_29815k = H, gas_const_r_mol = G, vial_volume = V, h3po4_added = P),
       r_dic(11.5, 9.5, 0.5, 0.3, NA_real_, 22, H, G, V, P))
  )
  for (i in seq_len(N)) {
    aw <- runif(1, 8, 10)
    asw <- aw + runif(1, 0.5, 3)
    vop <- runif(1, 0, 2)
    sa <- runif(1, 0.05, 0.5)
    co2 <- maybe_na(runif(1, 100, 10000))
    at <- runif(1, 15, 30)
    cases <- c(cases, list(tc(paste0("rand_", i),
      list(acid_sample_wght = asw, acid_wght = aw, vol_overpressure = vop,
           sa_added = sa, co2_dry = co2, air_temp_c = at,
           h_co2_29815k = H, gas_const_r_mol = G, vial_volume = V, h3po4_added = P),
      r_dic(asw, aw, vop, sa, co2, at, H, G, V, P))))
  }
  cases
}

gen_d13c_dic <- function() {
  H <- PORTAL_CONSTANTS[["h_co2_29815k"]]
  G <- PORTAL_CONSTANTS[["gas_const_r_mol"]]
  V <- PORTAL_CONSTANTS[["vial_volume"]]
  P <- PORTAL_CONSTANTS[["h3po4_added"]]
  cases <- list(
    tc("na_d13", list(acid_sample_wght = 11.5, acid_wght = 9.5, vol_overpressure = 0.5,
                      delta_13co2 = NA, air_temp_c = 22.0,
                      h_co2_29815k = H, gas_const_r_mol = G, vial_volume = V, h3po4_added = P),
       r_d13c_dic(11.5, 9.5, 0.5, NA_real_, 22, H, G, V, P))
  )
  for (i in seq_len(N)) {
    aw <- runif(1, 8, 10)
    asw <- aw + runif(1, 0.5, 3)
    vop <- runif(1, 0, 2)
    d13 <- maybe_na(runif(1, -25, 5))
    at <- runif(1, 15, 30)
    cases <- c(cases, list(tc(paste0("rand_", i),
      list(acid_sample_wght = asw, acid_wght = aw, vol_overpressure = vop,
           delta_13co2 = d13, air_temp_c = at,
           h_co2_29815k = H, gas_const_r_mol = G, vial_volume = V, h3po4_added = P),
      r_d13c_dic(asw, aw, vop, d13, at, H, G, V, P))))
  }
  cases
}

gen_dic_replicates <- function() {
  H <- PORTAL_CONSTANTS[["h_co2_29815k"]]
  G <- PORTAL_CONSTANTS[["gas_const_r_mol"]]
  V <- PORTAL_CONSTANTS[["vial_volume"]]
  P <- PORTAL_CONSTANTS[["h3po4_added"]]
  cases <- list()
  for (i in seq_len(N_MAP)) {
    rep_inputs <- lapply(1:2, function(j) {
      aw <- runif(1, 8, 10)
      list(
        acid_sample_wght = aw + runif(1, 0.5, 3),
        acid_wght = aw,
        vol_overpressure = runif(1, 0, 2),
        sa_added = runif(1, 0.05, 0.5),
        co2_dry = maybe_na(runif(1, 100, 10000), 0.1),
        delta_13co2 = maybe_na(runif(1, -25, 5), 0.1)
      )
    })
    at <- runif(1, 15, 30)
    vals <- lapply(rep_inputs, function(r) {
      list(
        dic = r_dic(r$acid_sample_wght, r$acid_wght, r$vol_overpressure,
                    r$sa_added, r$co2_dry, at, H, G, V, P),
        d13c = r_d13c_dic(r$acid_sample_wght, r$acid_wght, r$vol_overpressure,
                          r$delta_13co2, at, H, G, V, P)
      )
    })
    dic_pair <- c(num(vals[[1]]$dic), num(vals[[2]]$dic))
    d13_pair <- c(num(vals[[1]]$d13c), num(vals[[2]]$d13c))
    cases <- c(cases, list(tcm(paste0("rand_", i),
      list(a = rep_inputs[[1]], b = rep_inputs[[2]], air_temp_c = at,
           h_co2_29815k = H, gas_const_r_mol = G, vial_volume = V, h3po4_added = P),
      list(
        dic_a = vals[[1]]$dic, dic_b = vals[[2]]$dic,
        dic_avg = r_mean(dic_pair), dic_std = r_sd(dic_pair),
        d13c_a = vals[[1]]$d13c, d13c_b = vals[[2]]$d13c,
        d13c_avg = r_mean(d13_pair), d13c_std = r_sd(d13_pair)
      ))))
  }
  cases
}

gen_pco2_replicates <- function() {
  C <- PORTAL_CONSTANTS[["c_const"]]
  R <- PORTAL_CONSTANTS[["gas_const_r_atm"]]
  KH <- PORTAL_CONSTANTS[["h_ch4_29815k"]]
  SA <- PORTAL_CONSTANTS[["ch4_in_sa"]]
  GR <- PORTAL_CONSTANTS[["gas_const_r_mol"]]
  cases <- list()
  for (i in seq_len(N_MAP)) {
    rep_inputs <- lapply(1:2, function(j) {
      list(
        co2_ppm = maybe_na(runif(1, 100, 10000), 0.1),
        h2o_percent = runif(1, 0, 3.5),
        ch4_ppm = maybe_na(runif(1, 1, 500), 0.1)
      )
    })
    wt <- runif(1, 0.5, 25)
    bp <- runif(1, 700, 1050)
    lt <- runif(1, 15, 30)
    lp <- runif(1, 0.9, 1.05)
    vs <- runif(1, 0.02, 0.08)
    vw <- runif(1, 0.02, 0.06)
    vals <- lapply(rep_inputs, function(r) {
      co2_hs <- num(r_co2_headspace(r$co2_ppm, lt, lp, vs, vw, C, R))
      ch4d <- num(r_ch4_dry(r$ch4_ppm, r$h2o_percent))
      list(
        co2_hs = co2_hs,
        pco2 = r_pco2(co2_hs, wt, C),
        p1 = r_pco2_p1(co2_hs, wt, bp, C),
        p2 = r_pco2_p2(co2_hs, wt, bp, C),
        ch4_dry = ch4d,
        ch4_diss = r_dissolved_ch4(ch4d, wt, bp, lt, KH, SA, GR)
      )
    })
    pair <- function(field) c(num(vals[[1]][[field]]), num(vals[[2]][[field]]))
    expected <- list(
      co2_hs_a = vals[[1]]$co2_hs, co2_hs_b = vals[[2]]$co2_hs,
      co2_hs_avg = r_mean(pair("co2_hs")), co2_hs_sd = r_sd(pair("co2_hs")),
      pco2_avg = r_mean(pair("pco2")), pco2_sd = r_sd(pair("pco2")),
      pco2_p1_avg = r_mean(pair("p1")), pco2_p1_sd = r_sd(pair("p1")),
      pco2_p2_avg = r_mean(pair("p2")), pco2_p2_sd = r_sd(pair("p2")),
      ch4_dry_avg = r_mean(pair("ch4_dry")), ch4_dry_sd = r_sd(pair("ch4_dry")),
      ch4_dissolved_avg = r_mean(pair("ch4_diss")), ch4_dissolved_sd = r_sd(pair("ch4_diss"))
    )
    cases <- c(cases, list(tcm(paste0("rand_", i),
      list(a = rep_inputs[[1]], b = rep_inputs[[2]],
           water_temp_c = wt, bp_hpa = bp, lab_temp_c = lt, lab_pressure_atm = lp,
           vol_sa_ml = vs, vol_water_ml = vw,
           c_const = C, gas_const_r_atm = R,
           h_ch4_29815k = KH, ch4_in_sa = SA, gas_const_r_mol = GR),
      expected)))
  }
  cases
}

gen_nutrients <- function() {
  cases <- list()
  species_pool <- c("P", "NH4", "TDP", "TDN")
  for (i in seq_len(N_MAP)) {
    species <- list()
    n_extra <- sample(0:2, 1)
    for (s in sample(species_pool, n_extra)) {
      reps <- runif(3, 1, 200)
      if (runif(1) < 0.15) reps[sample(3, 1)] <- NA
      species[[s]] <- reps
    }
    nox <- runif(3, 20, 300)
    no2 <- runif(3, 0.5, 15)
    if (runif(1) < 0.2) nox[sample(3, 1)] <- NA
    if (runif(1) < 0.2) no2[sample(3, 1)] <- NA
    species[["NOx"]] <- nox
    species[["NO2"]] <- no2

    no3 <- mapply(function(a, b) num(r_minus(a, b)), nox, no2)
    expected <- list()
    for (s in names(species)) {
      expected[[paste0(s, "_avg")]] <- r_mean(species[[s]])
      expected[[paste0(s, "_sd")]] <- r_sd(species[[s]])
    }
    expected[["NO3_avg"]] <- r_mean(no3)
    expected[["NO3_sd"]] <- r_sd(no3)

    cases <- c(cases, list(tcm(paste0("rand_", i), list(species = species), expected)))
  }
  cases
}

gen_chla_benthic <- function() {
  cases <- list()
  for (i in seq_len(N_MAP)) {
    acid_sl <- runif(1, 0.1, 0.6)
    acid_int <- runif(1, -3, 2)
    noacid_sl <- runif(1, 0.1, 0.6)
    noacid_int <- runif(1, -3, 2)
    n_reps <- sample(1:5, 1)
    reps <- lapply(seq_len(n_reps), function(j) {
      vt <- runif(1, 50, 500)
      list(
        fluor_before = runif(1, 50, 300),
        fluor_after = maybe_na(runif(1, 20, 200), 0.2),
        vol_total_ml = vt,
        vol_after_ml = runif(1, 5, vt * 0.8),
        diameters_cm = runif(3, 3, 50),
        afdm_g_filter = maybe_na(runif(1, 0.001, 0.01), 0.2)
      )
    })
    vals <- lapply(reps, function(r) {
      vf <- r$vol_total_ml - r$vol_after_ml
      acid <- if (is.na(r$fluor_after)) NA_real_ else
        num(r_chla_acid(r$fluor_before, r$fluor_after, acid_sl, acid_int))
      noacid <- num(r_chla_no_acid(r$fluor_before, noacid_sl, noacid_int))
      list(
        acid = acid,
        noacid = noacid,
        acid_m2 = if (is.na(acid)) NA_real_ else
          num(r_benthic_chla(acid, r$diameters_cm, vf, r$vol_total_ml)),
        noacid_m2 = num(r_benthic_chla(noacid, r$diameters_cm, vf, r$vol_total_ml)),
        afdm_m2 = if (is.na(r$afdm_g_filter)) NA_real_ else
          num(r_benthic_afdm(r$afdm_g_filter, r$diameters_cm, vf, r$vol_total_ml))
      )
    })
    col <- function(field) sapply(vals, function(v) v[[field]])
    has_acid <- any(!is.na(col("acid")))
    has_afdm <- any(!is.na(col("afdm_m2")))
    expected <- list(
      chla_noacid_ug_l_avg = r_mean(col("noacid")),
      chla_noacid_ug_l_sd = r_sd(col("noacid")),
      chla_noacid_ug_m2_avg = r_mean(col("noacid_m2")),
      chla_noacid_ug_m2_sd = r_sd(col("noacid_m2"))
    )
    if (has_acid) {
      expected$chla_acid_ug_l_avg <- r_mean(col("acid"))
      expected$chla_acid_ug_l_sd <- r_sd(col("acid"))
      expected$chla_acid_ug_m2_avg <- r_mean(col("acid_m2"))
      expected$chla_acid_ug_m2_sd <- r_sd(col("acid_m2"))
    }
    if (has_afdm) {
      expected$afdm_g_m2_avg <- r_mean(col("afdm_m2"))
      expected$afdm_g_m2_sd <- r_sd(col("afdm_m2"))
    }
    cases <- c(cases, list(tcm(paste0("rand_", i),
      list(acid_slope = acid_sl, acid_intercept = acid_int,
           noacid_slope = noacid_sl, noacid_intercept = noacid_int,
           replicates = reps),
      expected)))
  }
  cases
}

# calcChlaNoAcid is the portal's raw * stdCurve$a + stdCurve$b applied verbatim,
# so it drives the standalone apply_standard_curve fixtures.
gen_apply_standard_curve <- function() {
  cases <- list(
    tc("identity", list(raw = 100.0, slope = 1.0, intercept = 0.0),
       r_chla_no_acid(100, 1, 0)),
    tc("typical", list(raw = 100.0, slope = 1.05, intercept = -2.3),
       r_chla_no_acid(100, 1.05, -2.3)),
    tc("na_raw", list(raw = NA, slope = 1.05, intercept = -2.3),
       r_chla_no_acid(NA_real_, 1.05, -2.3))
  )
  for (i in seq_len(N)) {
    raw <- maybe_na(runif(1, -50, 500))
    sl <- runif(1, 0.8, 1.3)
    int <- runif(1, -10, 10)
    cases <- c(cases, list(tc(paste0("rand_", i),
      list(raw = raw, slope = sl, intercept = int),
      r_chla_no_acid(raw, sl, int))))
  }
  cases
}

gen_absorbance_ratio <- function() {
  cases <- list(
    tc("e2_e3", list(numerator = 0.2, denominator = 0.05), r_ratio(0.2, 0.05)),
    tc("zero_denominator", list(numerator = 0.2, denominator = 0.0), r_ratio(0.2, 0)),
    tc("na", list(numerator = NA, denominator = 0.05), r_ratio(NA_real_, 0.05))
  )
  for (i in seq_len(N)) {
    a <- maybe_na(runif(1, 0.001, 2))
    b <- maybe_na(runif(1, 0.001, 2))
    cases <- c(cases, list(tc(paste0("rand_", i),
      list(numerator = a, denominator = b), r_ratio(a, b))))
  }
  cases
}

gen_nutrient_from_replicates <- function() {
  cases <- list(
    tcm("single", list(replicates = c(42.0)),
        list(avg = r_mean(42.0), sd = r_sd(42.0)))
  )
  for (i in seq_len(N_MAP)) {
    reps <- runif(sample(2:5, 1), 1, 300)
    if (runif(1) < 0.15) reps[sample(length(reps), 1)] <- NA
    cases <- c(cases, list(tcm(paste0("rand_", i),
      list(replicates = reps),
      list(avg = r_mean(reps), sd = r_sd(reps)))))
  }
  cases
}

gen_nitrate <- function() {
  cases <- list(
    tc("typical", list(nox = 50.0, no2 = 3.0), r_minus(50, 3)),
    tc("na_nox", list(nox = NA, no2 = 3.0), r_minus(NA_real_, 3))
  )
  for (i in seq_len(N)) {
    nox <- maybe_na(runif(1, 20, 300))
    no2 <- maybe_na(runif(1, 0.5, 15))
    cases <- c(cases, list(tc(paste0("rand_", i),
      list(nox = nox, no2 = no2), r_minus(nox, no2))))
  }
  cases
}

gen_pco2_full_pipeline <- function() {
  C <- PORTAL_CONSTANTS[["c_const"]]
  R <- PORTAL_CONSTANTS[["gas_const_r_atm"]]
  KH <- PORTAL_CONSTANTS[["h_ch4_29815k"]]
  SA <- PORTAL_CONSTANTS[["ch4_in_sa"]]
  GR <- PORTAL_CONSTANTS[["gas_const_r_mol"]]
  cases <- list()
  for (i in seq_len(N_MAP)) {
    co2_ppm <- maybe_na(runif(1, 100, 10000), 0.1)
    h2o <- runif(1, 0, 3.5)
    ch4 <- maybe_na(runif(1, 1, 500), 0.1)
    wt <- runif(1, 0.5, 25)
    bp <- runif(1, 700, 1050)
    lt <- runif(1, 15, 30)
    lp <- runif(1, 0.9, 1.05)
    vs <- runif(1, 0.02, 0.08)
    vw <- runif(1, 0.02, 0.06)
    co2_hs <- num(r_co2_headspace(co2_ppm, lt, lp, vs, vw, C, R))
    ch4d <- num(r_ch4_dry(ch4, h2o))
    cases <- c(cases, list(tcm(paste0("rand_", i),
      list(co2_ppm = co2_ppm, h2o_percent = h2o, ch4_ppm = ch4,
           water_temp_c = wt, bp_hpa = bp, lab_temp_c = lt, lab_pressure_atm = lp,
           vol_sa_ml = vs, vol_water_ml = vw,
           c_const = C, gas_const_r_atm = R,
           h_ch4_29815k = KH, ch4_in_sa = SA, gas_const_r_mol = GR),
      list(
        co2_hs = co2_hs,
        pco2 = r_pco2(co2_hs, wt, C),
        pco2_p1 = r_pco2_p1(co2_hs, wt, bp, C),
        pco2_p2 = r_pco2_p2(co2_hs, wt, bp, C),
        ch4_dry = ch4d,
        ch4_dissolved = r_dissolved_ch4(ch4d, wt, bp, lt, KH, SA, GR)
      ))))
  }
  cases
}

gen_dic_combined <- function() {
  H <- PORTAL_CONSTANTS[["h_co2_29815k"]]
  G <- PORTAL_CONSTANTS[["gas_const_r_mol"]]
  V <- PORTAL_CONSTANTS[["vial_volume"]]
  P <- PORTAL_CONSTANTS[["h3po4_added"]]
  cases <- list()
  for (i in seq_len(N_MAP)) {
    aw <- runif(1, 8, 10)
    asw <- aw + runif(1, 0.5, 3)
    vop <- runif(1, 0, 2)
    sa <- runif(1, 0.05, 0.5)
    co2 <- maybe_na(runif(1, 100, 10000), 0.1)
    d13 <- maybe_na(runif(1, -25, 5), 0.1)
    at <- runif(1, 15, 30)
    cases <- c(cases, list(tcm(paste0("rand_", i),
      list(acid_sample_wght = asw, acid_wght = aw, vol_overpressure = vop,
           sa_added = sa, co2_dry = co2, delta_13co2 = d13, air_temp_c = at,
           h_co2_29815k = H, gas_const_r_mol = G, vial_volume = V, h3po4_added = P),
      list(
        dic = r_dic(asw, aw, vop, sa, co2, at, H, G, V, P),
        d13c = r_d13c_dic(asw, aw, vop, d13, at, H, G, V, P)
      ))))
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
cases$doc$doc_average <- gen_doc(calcDOCavg)
cases$doc$doc_std_dev <- gen_doc(calcDOCsd)
cases$chlorophyll$chla_acid <- gen_chla_acid()
cases$chlorophyll$chla_no_acid <- gen_chla_no_acid()
cases$chlorophyll$chla_benthic_replicates <- gen_chla_benthic()
cases$benthic$rock_surface_area_m2 <- gen_rock_area()
cases$benthic$per_m2 <- gen_per_m2()
cases$benthic$benthic_afdm_per_m2 <- gen_benthic_afdm()
cases$benthic$benthic_chla_per_m2 <- gen_benthic_chla()
cases$field_data$barometric_pressure_from_altitude <- gen_baro()
cases$field_data$co2_correction <- gen_co2_correction()
cases$field_data$reach_depth_stats <- gen_reach_depths()
cases$field_data$select_pressure <- gen_select_pressure()
cases$pco2$ch4_dry <- gen_ch4_dry()
cases$pco2$pco2_from_co2aq <- gen_pco2()
cases$pco2$pco2_p1 <- gen_pco2_p1()
cases$pco2$pco2_p2 <- gen_pco2_p2()
cases$pco2$dissolved_ch4 <- gen_dissolved_ch4()
cases$pco2$pco2_replicates <- gen_pco2_replicates()
cases$co2_air$co2_headspace <- gen_co2_headspace()
cases$dic$dic_concentration <- gen_dic()
cases$dic$d13c_dic <- gen_d13c_dic()
cases$dic$dic_replicates <- gen_dic_replicates()
cases$nutrients$multi_nutrient_replicates <- gen_nutrients()
# The additions below run after every original generator so the RNG stream,
# and therefore every original case, is unchanged on regeneration
cases$common$apply_standard_curve <- gen_apply_standard_curve()
cases$dom$absorbance_ratio <- gen_absorbance_ratio()
cases$nutrients$nutrient_from_replicates <- gen_nutrient_from_replicates()
cases$nutrients$nitrate_from_nox_no2 <- gen_nitrate()
cases$pco2$pco2_full_pipeline <- gen_pco2_full_pipeline()
cases$dic$dic <- gen_dic_combined()

output <- list(
  metadata = list(
    generator = "r_reference/generate_fixtures.R",
    source = "portal calculation_functions.R via r_reference/functions/",
    r_version = paste(R.version$major, R.version$minor, sep = "."),
    generated_at = format(Sys.time(), "%Y-%m-%dT%H:%M:%SZ", tz = "UTC"),
    seed = 42,
    cases_per_function = N
  ),
  modules = cases
)

output_path <- file.path(script_dir, "..", "tests", "fixtures", "golden_values.json")
dir.create(dirname(output_path), showWarnings = FALSE, recursive = TRUE)

json_text <- toJSON(output, auto_unbox = TRUE, pretty = TRUE, na = "null", digits = NA)
writeLines(json_text, output_path)

total_cases <- sum(sapply(cases, function(m) sum(sapply(m, length))))
cat("Generated", output_path, "\n")
for (m in names(cases)) {
  for (fn in names(cases[[m]])) {
    cat(sprintf("  %s.%s: %d cases\n", m, fn, length(cases[[m]][[fn]])))
  }
}
cat("Total test cases:", total_cases, "\n")
