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

fn v(obj: &serde_json::Value, key: &str) -> f64 {
    match obj.get(key) {
        Some(serde_json::Value::Number(n)) => n.as_f64().unwrap(),
        Some(serde_json::Value::Null) | None => f64::NAN,
        other => panic!("expected number or null for {key}, got {other:?}"),
    }
}

fn v_opt(obj: &serde_json::Value, key: &str) -> Option<f64> {
    obj.get(key).and_then(|x| x.as_f64())
}

fn v_vec(obj: &serde_json::Value, key: &str) -> Vec<f64> {
    match obj.get(key) {
        Some(serde_json::Value::Array(arr)) => arr
            .iter()
            .map(|x| if x.is_null() { f64::NAN } else { x.as_f64().unwrap() })
            .collect(),
        Some(serde_json::Value::Number(n)) => vec![n.as_f64().unwrap()],
        Some(serde_json::Value::Null) | None => vec![f64::NAN],
        other => panic!("expected array for {key}, got {other:?}"),
    }
}

fn check(actual: f64, case: &TestCase, module: &str, func: &str) {
    let ctx = format!("{module}::{func}::{}", case.name);
    match case.expected {
        None => assert!(
            actual.is_nan(),
            "{ctx}: expected NaN, got {actual}"
        ),
        Some(exp) => {
            let tol = case.tolerance.unwrap_or(1e-10);
            assert!(
                (actual - exp).abs() < tol,
                "{ctx}: expected {exp}, got {actual} (tol={tol})"
            );
        }
    }
}

fn fixture() -> GoldenFixture {
    let json = include_str!("fixtures/golden_values.json");
    serde_json::from_str(json).expect("parse golden_values.json")
}

#[test]
fn golden_common() {
    let g = fixture();
    let mut count = 0;
    for case in &g.modules.common["mean"] {
        check(toolbox::common::mean(&v_vec(&case.inputs, "values")), case, "common", "mean");
        count += 1;
    }
    for case in &g.modules.common["std_dev"] {
        check(toolbox::common::std_dev(&v_vec(&case.inputs, "values")), case, "common", "std_dev");
        count += 1;
    }
    for case in &g.modules.common["minus"] {
        check(toolbox::common::minus(v(&case.inputs, "a"), v(&case.inputs, "b")), case, "common", "minus");
        count += 1;
    }
    for case in &g.modules.common["equals"] {
        check(toolbox::common::equals(v(&case.inputs, "primary"), v(&case.inputs, "fallback")), case, "common", "equals");
        count += 1;
    }
    for case in &g.modules.common["ratio"] {
        check(toolbox::common::ratio(v(&case.inputs, "dividend"), v(&case.inputs, "divisor")), case, "common", "ratio");
        count += 1;
    }
    eprintln!("common: {count} passed");
}

#[test]
fn golden_tss_afdm() {
    let g = fixture();
    let mut count = 0;
    for case in &g.modules.tss_afdm["tss_mg_l"] {
        let result = toolbox::tss_afdm::tss_mg_l(v(&case.inputs, "wgt_dried"), v(&case.inputs, "wgt_prefilt"), v(&case.inputs, "vol_filtered"));
        check(result, case, "tss_afdm", "tss_mg_l");
        count += 1;
    }
    for case in &g.modules.tss_afdm["afdm_mg_l"] {
        let result = toolbox::tss_afdm::afdm_mg_l(v(&case.inputs, "wgt_dried"), v(&case.inputs, "wgt_ashed"), v(&case.inputs, "vol_filtered"));
        check(result, case, "tss_afdm", "afdm_mg_l");
        count += 1;
    }
    eprintln!("tss_afdm: {count} passed");
}

#[test]
fn golden_dom() {
    let g = fixture();
    let mut count = 0;
    for case in &g.modules.dom["suva"] {
        check(toolbox::dom::suva(v(&case.inputs, "a254"), v(&case.inputs, "doc_avg_ppb")), case, "dom", "suva");
        count += 1;
    }
    eprintln!("dom: {count} passed");
}

#[test]
fn golden_doc() {
    let g = fixture();
    let mut count = 0;
    for case in &g.modules.doc["doc_average"] {
        let reps = v_vec(&case.inputs, "replicates");
        let curve = v_opt(&case.inputs, "slope").zip(v_opt(&case.inputs, "intercept"));
        check(toolbox::doc::doc_average(&reps, curve), case, "doc", "doc_average");
        count += 1;
    }
    for case in &g.modules.doc["doc_std_dev"] {
        let reps = v_vec(&case.inputs, "replicates");
        let curve = v_opt(&case.inputs, "slope").zip(v_opt(&case.inputs, "intercept"));
        check(toolbox::doc::doc_std_dev(&reps, curve), case, "doc", "doc_std_dev");
        count += 1;
    }
    eprintln!("doc: {count} passed");
}

#[test]
fn golden_chlorophyll() {
    let g = fixture();
    let mut count = 0;
    for case in &g.modules.chlorophyll["chla_acid"] {
        let result = toolbox::chlorophyll::chla_acid(v(&case.inputs, "fluor_before"), v(&case.inputs, "fluor_after"), v(&case.inputs, "slope"), v(&case.inputs, "intercept"));
        check(result, case, "chlorophyll", "chla_acid");
        count += 1;
    }
    for case in &g.modules.chlorophyll["chla_no_acid"] {
        let result = toolbox::chlorophyll::chla_no_acid(v(&case.inputs, "fluor"), v(&case.inputs, "slope"), v(&case.inputs, "intercept"));
        check(result, case, "chlorophyll", "chla_no_acid");
        count += 1;
    }
    eprintln!("chlorophyll: {count} passed");
}

#[test]
fn golden_benthic() {
    let g = fixture();
    let mut count = 0;
    for case in &g.modules.benthic["rock_surface_area_m2"] {
        check(toolbox::benthic::rock_surface_area_m2(&v_vec(&case.inputs, "dims_cm")), case, "benthic", "rock_surface_area_m2");
        count += 1;
    }
    for case in &g.modules.benthic["per_m2"] {
        let dims = v_vec(&case.inputs, "dims_cm");
        let area = toolbox::benthic::rock_surface_area_m2(&dims);
        check(toolbox::benthic::per_m2(v(&case.inputs, "sample_value"), v(&case.inputs, "vol_total"), v(&case.inputs, "vol_filtrated"), area), case, "benthic", "per_m2");
        count += 1;
    }
    for case in &g.modules.benthic["benthic_afdm_per_m2"] {
        let dims = v_vec(&case.inputs, "dims_cm");
        check(toolbox::benthic::benthic_afdm_per_m2(v(&case.inputs, "afdm_g"), &dims, v(&case.inputs, "vol_filtrated"), v(&case.inputs, "vol_total")), case, "benthic", "benthic_afdm_per_m2");
        count += 1;
    }
    eprintln!("benthic: {count} passed");
}

#[test]
fn golden_field_data() {
    let g = fixture();
    let mut count = 0;
    for case in &g.modules.field_data["barometric_pressure_from_altitude"] {
        check(toolbox::field_data::barometric_pressure_from_altitude(v(&case.inputs, "elevation_m"), v(&case.inputs, "temp_c")), case, "field_data", "barometric_pressure_from_altitude");
        count += 1;
    }
    for case in &g.modules.field_data["co2_correction"] {
        let curve = v_opt(&case.inputs, "std_slope").zip(v_opt(&case.inputs, "std_intercept"));
        check(toolbox::field_data::co2_correction(v(&case.inputs, "raw_co2"), v(&case.inputs, "pressure_hpa"), v(&case.inputs, "temp_c"), curve), case, "field_data", "co2_correction");
        count += 1;
    }
    eprintln!("field_data: {count} passed");
}

#[test]
fn golden_pco2() {
    let g = fixture();
    let constants = toolbox::pco2::GasConstants::default();
    let mut count = 0;

    for case in &g.modules.pco2["ch4_dry"] {
        check(toolbox::pco2::ch4_dry(v(&case.inputs, "ch4_wet"), v(&case.inputs, "h2o_percent")), case, "pco2", "ch4_dry");
        count += 1;
    }
    for case in &g.modules.pco2["pco2_from_co2aq"] {
        check(toolbox::pco2::pco2_from_co2aq(v(&case.inputs, "co2_aq"), v(&case.inputs, "water_temp_c"), &constants), case, "pco2", "pco2_from_co2aq");
        count += 1;
    }
    for case in &g.modules.pco2["pco2_p1"] {
        check(toolbox::pco2::pco2_p1(v(&case.inputs, "co2_aq"), v(&case.inputs, "water_temp_c"), v(&case.inputs, "bp_hpa"), &constants), case, "pco2", "pco2_p1");
        count += 1;
    }
    for case in &g.modules.pco2["pco2_p2"] {
        check(toolbox::pco2::pco2_p2(v(&case.inputs, "co2_aq"), v(&case.inputs, "water_temp_c"), v(&case.inputs, "bp_hpa"), &constants), case, "pco2", "pco2_p2");
        count += 1;
    }
    for case in &g.modules.pco2["dissolved_ch4"] {
        let gc = toolbox::pco2::GasConstants {
            kh_ch4: v(&case.inputs, "kh_ch4"),
            ch4_temp_const: v(&case.inputs, "ch4_temp_const"),
            ch4_in_sa: v(&case.inputs, "ch4_in_sa"),
            gas_const_r_mol: v(&case.inputs, "gas_const_r_mol"),
            ..constants
        };
        check(toolbox::pco2::dissolved_ch4(v(&case.inputs, "ch4_dry"), v(&case.inputs, "water_temp_c"), v(&case.inputs, "bp_hpa"), v(&case.inputs, "lab_temp_c"), v(&case.inputs, "lab_pressure_atm"), &gc), case, "pco2", "dissolved_ch4");
        count += 1;
    }
    eprintln!("pco2: {count} passed");
}

#[test]
fn golden_co2_air() {
    let g = fixture();
    let mut count = 0;
    for case in &g.modules.co2_air["co2_headspace"] {
        let gc = toolbox::pco2::GasConstants {
            kh_co2: v(&case.inputs, "kh_co2"),
            c_const: v(&case.inputs, "c_const"),
            gas_const_r_atm: v(&case.inputs, "gas_const_r_atm"),
            ..toolbox::pco2::GasConstants::default()
        };
        check(toolbox::co2_air::co2_headspace(v(&case.inputs, "co2_ppm"), v(&case.inputs, "lab_temp_c"), v(&case.inputs, "lab_pressure_atm"), v(&case.inputs, "vol_sa_ml"), v(&case.inputs, "vol_water_ml"), &gc), case, "co2_air", "co2_headspace");
        count += 1;
    }
    eprintln!("co2_air: {count} passed");
}

#[test]
fn golden_dic() {
    let g = fixture();
    let mut count = 0;
    for case in &g.modules.dic["dic_concentration"] {
        let dc = toolbox::dic::DICConstants {
            h_co2_29815k: v(&case.inputs, "h_co2_29815k"),
            gas_const_r_mol: v(&case.inputs, "gas_const_r_mol"),
            vial_volume: v(&case.inputs, "vial_volume"),
            h3po4_added: v(&case.inputs, "h3po4_added"),
        };
        check(toolbox::dic::dic_concentration(v(&case.inputs, "acid_sample_wght"), v(&case.inputs, "acid_wght"), v(&case.inputs, "vol_overpressure"), v(&case.inputs, "sa_added"), v(&case.inputs, "co2_dry"), v(&case.inputs, "air_temp_c"), &dc), case, "dic", "dic_concentration");
        count += 1;
    }
    for case in &g.modules.dic["d13c_dic"] {
        let dc = toolbox::dic::DICConstants {
            h_co2_29815k: v(&case.inputs, "h_co2_29815k"),
            gas_const_r_mol: v(&case.inputs, "gas_const_r_mol"),
            vial_volume: v(&case.inputs, "vial_volume"),
            h3po4_added: v(&case.inputs, "h3po4_added"),
        };
        check(toolbox::dic::d13c_dic(v(&case.inputs, "acid_sample_wght"), v(&case.inputs, "acid_wght"), v(&case.inputs, "vol_overpressure"), v(&case.inputs, "delta_13co2"), v(&case.inputs, "air_temp_c"), &dc), case, "dic", "d13c_dic");
        count += 1;
    }
    eprintln!("dic: {count} passed");
}
