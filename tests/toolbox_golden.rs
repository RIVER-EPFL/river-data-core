#![cfg(feature = "toolbox")]

use river_data_core::toolbox;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize)]
struct GoldenFixture {
    modules: Modules,
}

#[derive(Deserialize)]
struct Modules {
    common: HashMap<String, Vec<TestCase>>,
    tss_afdm: HashMap<String, Vec<TestCase>>,
    dom: HashMap<String, Vec<TestCase>>,
    doc: HashMap<String, Vec<TestCase>>,
    chlorophyll: HashMap<String, Vec<TestCase>>,
    benthic: HashMap<String, Vec<TestCase>>,
    field_data: HashMap<String, Vec<TestCase>>,
    pco2: HashMap<String, Vec<TestCase>>,
    co2_air: HashMap<String, Vec<TestCase>>,
    dic: HashMap<String, Vec<TestCase>>,
}

#[derive(Deserialize)]
struct TestCase {
    name: String,
    inputs: serde_json::Value,
    expected: Option<f64>,
    #[serde(default = "default_tolerance")]
    tolerance: Option<f64>,
}

fn default_tolerance() -> Option<f64> {
    Some(1e-10)
}

fn f(v: &serde_json::Value, key: &str) -> f64 {
    v[key].as_f64().expect(key)
}

fn f_opt(v: &serde_json::Value, key: &str) -> Option<f64> {
    v.get(key).and_then(|v| v.as_f64())
}

fn f_vec(v: &serde_json::Value, key: &str) -> Vec<f64> {
    match &v[key] {
        serde_json::Value::Array(arr) => arr
            .iter()
            .map(|x| if x.is_null() { f64::NAN } else { x.as_f64().unwrap() })
            .collect(),
        serde_json::Value::Number(n) => vec![n.as_f64().unwrap()],
        serde_json::Value::Null => vec![f64::NAN],
        other => panic!("expected array or number for {key}, got {other}"),
    }
}

fn assert_close(actual: f64, expected: f64, tol: f64, ctx: &str) {
    assert!(
        (actual - expected).abs() < tol,
        "{ctx}: expected {expected}, got {actual} (tol={tol})"
    );
}

fn assert_nan(actual: f64, ctx: &str) {
    assert!(actual.is_nan(), "{ctx}: expected NaN, got {actual}");
}

fn check(actual: f64, case: &TestCase, module: &str, func: &str) {
    let ctx = format!("{module}::{func}::{}", case.name);
    match case.expected {
        None => assert_nan(actual, &ctx),
        Some(exp) => assert_close(actual, exp, case.tolerance.unwrap_or(1e-10), &ctx),
    }
}

// =============================================================================
// Tests
// =============================================================================

fn fixture() -> GoldenFixture {
    let json = include_str!("fixtures/golden_values.json");
    serde_json::from_str(json).expect("parse golden_values.json")
}

#[test]
fn golden_common() {
    let g = fixture();
    for case in &g.modules.common["mean"] {
        let vals = f_vec(&case.inputs, "values");
        check(toolbox::common::mean(&vals), case, "common", "mean");
    }
    for case in &g.modules.common["std_dev"] {
        let vals = f_vec(&case.inputs, "values");
        check(toolbox::common::std_dev(&vals), case, "common", "std_dev");
    }
    for case in &g.modules.common["minus"] {
        let a = if case.inputs["a"].is_null() { f64::NAN } else { f(&case.inputs, "a") };
        let b = if case.inputs["b"].is_null() { f64::NAN } else { f(&case.inputs, "b") };
        check(toolbox::common::minus(a, b), case, "common", "minus");
    }
    for case in &g.modules.common["equals"] {
        let primary = if case.inputs["primary"].is_null() { f64::NAN } else { f(&case.inputs, "primary") };
        let fallback = if case.inputs["fallback"].is_null() { f64::NAN } else { f(&case.inputs, "fallback") };
        check(toolbox::common::equals(primary, fallback), case, "common", "equals");
    }
    for case in &g.modules.common["ratio"] {
        let dividend = if case.inputs["dividend"].is_null() { f64::NAN } else { f(&case.inputs, "dividend") };
        let divisor = if case.inputs["divisor"].is_null() { f64::NAN } else { f(&case.inputs, "divisor") };
        check(toolbox::common::ratio(dividend, divisor), case, "common", "ratio");
    }
}

#[test]
fn golden_tss_afdm() {
    let g = fixture();
    for case in &g.modules.tss_afdm["tss_mg_l"] {
        let result = if case.inputs["wgt_dried"].is_null()
            || case.inputs["wgt_prefilt"].is_null()
            || case.inputs["vol_filtered"].is_null()
        {
            f64::NAN
        } else {
            toolbox::tss_afdm::tss_mg_l(
                f(&case.inputs, "wgt_dried"),
                f(&case.inputs, "wgt_prefilt"),
                f(&case.inputs, "vol_filtered"),
            )
        };
        check(result, case, "tss_afdm", "tss_mg_l");
    }
    for case in &g.modules.tss_afdm["afdm_mg_l"] {
        let result = if case.inputs["wgt_dried"].is_null()
            || case.inputs["wgt_ashed"].is_null()
            || case.inputs["vol_filtered"].is_null()
        {
            f64::NAN
        } else {
            toolbox::tss_afdm::afdm_mg_l(
                f(&case.inputs, "wgt_dried"),
                f(&case.inputs, "wgt_ashed"),
                f(&case.inputs, "vol_filtered"),
            )
        };
        check(result, case, "tss_afdm", "afdm_mg_l");
    }
}

#[test]
fn golden_dom() {
    let g = fixture();
    for case in &g.modules.dom["suva"] {
        let result = if case.inputs["a254"].is_null() || case.inputs["doc_avg_ppb"].is_null() {
            f64::NAN
        } else {
            toolbox::dom::suva(f(&case.inputs, "a254"), f(&case.inputs, "doc_avg_ppb"))
        };
        check(result, case, "dom", "suva");
    }
}

#[test]
fn golden_doc() {
    let g = fixture();
    for case in &g.modules.doc["doc_average"] {
        let reps = f_vec(&case.inputs, "replicates");
        let curve = f_opt(&case.inputs, "slope")
            .zip(f_opt(&case.inputs, "intercept"));
        check(
            toolbox::doc::doc_average(&reps, curve),
            case,
            "doc",
            "doc_average",
        );
    }
    for case in &g.modules.doc["doc_std_dev"] {
        let reps = f_vec(&case.inputs, "replicates");
        let curve = f_opt(&case.inputs, "slope")
            .zip(f_opt(&case.inputs, "intercept"));
        check(
            toolbox::doc::doc_std_dev(&reps, curve),
            case,
            "doc",
            "doc_std_dev",
        );
    }
}

#[test]
fn golden_chlorophyll() {
    let g = fixture();
    for case in &g.modules.chlorophyll["chla_acid"] {
        let result = if case.inputs["fluor_before"].is_null() {
            f64::NAN
        } else {
            toolbox::chlorophyll::chla_acid(
                f(&case.inputs, "fluor_before"),
                f(&case.inputs, "fluor_after"),
                f(&case.inputs, "slope"),
                f(&case.inputs, "intercept"),
            )
        };
        check(result, case, "chlorophyll", "chla_acid");
    }
    for case in &g.modules.chlorophyll["chla_no_acid"] {
        let result = if case.inputs["slope"].is_null() {
            f64::NAN
        } else {
            toolbox::chlorophyll::chla_no_acid(
                f(&case.inputs, "fluor"),
                f(&case.inputs, "slope"),
                f(&case.inputs, "intercept"),
            )
        };
        check(result, case, "chlorophyll", "chla_no_acid");
    }
}

#[test]
fn golden_benthic() {
    let g = fixture();
    for case in &g.modules.benthic["rock_surface_area_m2"] {
        let dims = f_vec(&case.inputs, "dims_cm");
        check(
            toolbox::benthic::rock_surface_area_m2(&dims),
            case,
            "benthic",
            "rock_surface_area_m2",
        );
    }
    for case in &g.modules.benthic["per_m2"] {
        let dims = f_vec(&case.inputs, "dims_cm");
        let area = toolbox::benthic::rock_surface_area_m2(&dims);
        check(
            toolbox::benthic::per_m2(
                f(&case.inputs, "sample_value"),
                f(&case.inputs, "vol_total"),
                f(&case.inputs, "vol_filtrated"),
                area,
            ),
            case,
            "benthic",
            "per_m2",
        );
    }
    for case in &g.modules.benthic["benthic_afdm_per_m2"] {
        let dims = f_vec(&case.inputs, "dims_cm");
        check(
            toolbox::benthic::benthic_afdm_per_m2(
                f(&case.inputs, "afdm_g"),
                &dims,
                f(&case.inputs, "vol_filtrated"),
                f(&case.inputs, "vol_total"),
            ),
            case,
            "benthic",
            "benthic_afdm_per_m2",
        );
    }
}

#[test]
fn golden_field_data() {
    let g = fixture();
    for case in &g.modules.field_data["barometric_pressure_from_altitude"] {
        let result = if case.inputs["elevation_m"].is_null() {
            f64::NAN
        } else {
            toolbox::field_data::barometric_pressure_from_altitude(
                f(&case.inputs, "elevation_m"),
                f(&case.inputs, "temp_c"),
            )
        };
        check(result, case, "field_data", "barometric_pressure_from_altitude");
    }
    for case in &g.modules.field_data["co2_correction"] {
        let result = if case.inputs["temp_c"].is_null() {
            f64::NAN
        } else {
            let curve = f_opt(&case.inputs, "std_slope")
                .zip(f_opt(&case.inputs, "std_intercept"));
            toolbox::field_data::co2_correction(
                f(&case.inputs, "raw_co2"),
                f(&case.inputs, "pressure_hpa"),
                f(&case.inputs, "temp_c"),
                curve,
            )
        };
        check(result, case, "field_data", "co2_correction");
    }
}

#[test]
fn golden_pco2() {
    let g = fixture();
    let constants = toolbox::pco2::GasConstants::default();

    for case in &g.modules.pco2["ch4_dry"] {
        let result = if case.inputs["ch4_wet"].is_null() {
            f64::NAN
        } else {
            toolbox::pco2::ch4_dry(
                f(&case.inputs, "ch4_wet"),
                f(&case.inputs, "h2o_percent"),
            )
        };
        check(result, case, "pco2", "ch4_dry");
    }
    for case in &g.modules.pco2["pco2_from_co2aq"] {
        let result = if case.inputs["water_temp_c"].is_null() {
            f64::NAN
        } else {
            toolbox::pco2::pco2_from_co2aq(
                f(&case.inputs, "co2_aq"),
                f(&case.inputs, "water_temp_c"),
                &constants,
            )
        };
        check(result, case, "pco2", "pco2_from_co2aq");
    }
    for case in &g.modules.pco2["pco2_p1"] {
        let result = if case.inputs["bp_hpa"].is_null() {
            f64::NAN
        } else {
            toolbox::pco2::pco2_p1(
                f(&case.inputs, "co2_aq"),
                f(&case.inputs, "water_temp_c"),
                f(&case.inputs, "bp_hpa"),
                &constants,
            )
        };
        check(result, case, "pco2", "pco2_p1");
    }
    for case in &g.modules.pco2["pco2_p2"] {
        check(
            toolbox::pco2::pco2_p2(
                f(&case.inputs, "co2_aq"),
                f(&case.inputs, "water_temp_c"),
                f(&case.inputs, "bp_hpa"),
                &constants,
            ),
            case,
            "pco2",
            "pco2_p2",
        );
    }
    for case in &g.modules.pco2["dissolved_ch4"] {
        let result = if case.inputs["ch4_dry"].is_null() {
            f64::NAN
        } else {
            let gc = toolbox::pco2::GasConstants {
                kh_ch4: f(&case.inputs, "kh_ch4"),
                ch4_temp_const: f(&case.inputs, "ch4_temp_const"),
                ch4_in_sa: f(&case.inputs, "ch4_in_sa"),
                gas_const_r_mol: f(&case.inputs, "gas_const_r_mol"),
                ..constants
            };
            toolbox::pco2::dissolved_ch4(
                f(&case.inputs, "ch4_dry"),
                f(&case.inputs, "water_temp_c"),
                f(&case.inputs, "bp_hpa"),
                f(&case.inputs, "lab_temp_c"),
                f(&case.inputs, "lab_pressure_atm"),
                &gc,
            )
        };
        check(result, case, "pco2", "dissolved_ch4");
    }
}

#[test]
fn golden_co2_air() {
    let g = fixture();
    for case in &g.modules.co2_air["co2_headspace"] {
        let gc = toolbox::pco2::GasConstants {
            kh_co2: f(&case.inputs, "kh_co2"),
            c_const: f(&case.inputs, "c_const"),
            gas_const_r_atm: f(&case.inputs, "gas_const_r_atm"),
            ..toolbox::pco2::GasConstants::default()
        };
        let result = toolbox::co2_air::co2_headspace(
            f(&case.inputs, "co2_ppm"),
            f(&case.inputs, "lab_temp_c"),
            f(&case.inputs, "lab_pressure_atm"),
            f(&case.inputs, "vol_sa_ml"),
            f(&case.inputs, "vol_water_ml"),
            &gc,
        );
        check(result, case, "co2_air", "co2_headspace");
    }
}

#[test]
fn golden_dic() {
    let g = fixture();
    for case in &g.modules.dic["dic_concentration"] {
        let result = if case.inputs["acid_sample_wght"].is_null() {
            f64::NAN
        } else {
            let dc = toolbox::dic::DICConstants {
                h_co2_29815k: f(&case.inputs, "h_co2_29815k"),
                gas_const_r_mol: f(&case.inputs, "gas_const_r_mol"),
                vial_volume: f(&case.inputs, "vial_volume"),
                h3po4_added: f(&case.inputs, "h3po4_added"),
            };
            toolbox::dic::dic_concentration(
                f(&case.inputs, "acid_sample_wght"),
                f(&case.inputs, "acid_wght"),
                f(&case.inputs, "vol_overpressure"),
                f(&case.inputs, "sa_added"),
                f(&case.inputs, "co2_dry"),
                f(&case.inputs, "air_temp_c"),
                &dc,
            )
        };
        check(result, case, "dic", "dic_concentration");
    }
    for case in &g.modules.dic["d13c_dic"] {
        let result = if case.inputs["delta_13co2"].is_null() {
            f64::NAN
        } else {
            let dc = toolbox::dic::DICConstants {
                h_co2_29815k: f(&case.inputs, "h_co2_29815k"),
                gas_const_r_mol: f(&case.inputs, "gas_const_r_mol"),
                vial_volume: f(&case.inputs, "vial_volume"),
                h3po4_added: f(&case.inputs, "h3po4_added"),
            };
            toolbox::dic::d13c_dic(
                f(&case.inputs, "acid_sample_wght"),
                f(&case.inputs, "acid_wght"),
                f(&case.inputs, "vol_overpressure"),
                f(&case.inputs, "delta_13co2"),
                f(&case.inputs, "air_temp_c"),
                &dc,
            )
        };
        check(result, case, "dic", "d13c_dic");
    }
}
