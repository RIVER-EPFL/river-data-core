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
    nutrients: HashMap<String, Vec<TestCase>>,
}

#[derive(Deserialize)]
struct TestCase {
    name: String,
    inputs: serde_json::Value,
    #[serde(default)]
    expected: Option<f64>,
    #[serde(default)]
    expected_map: Option<HashMap<String, Option<f64>>>,
    #[serde(default = "default_tolerance")]
    tolerance: Option<f64>,
}

fn default_tolerance() -> Option<f64> {
    Some(1e-9)
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
            .map(|x| {
                if x.is_null() {
                    f64::NAN
                } else {
                    x.as_f64().unwrap()
                }
            })
            .collect(),
        Some(serde_json::Value::Number(n)) => vec![n.as_f64().unwrap()],
        Some(serde_json::Value::Null) | None => vec![f64::NAN],
        other => panic!("expected array for {key}, got {other:?}"),
    }
}

fn assert_close(actual: f64, expected: Option<f64>, tol: f64, ctx: &str) {
    match expected {
        None => assert!(actual.is_nan(), "{ctx}: expected NaN, got {actual}"),
        Some(exp) => {
            let bound = tol * exp.abs().max(1.0);
            assert!(
                (actual - exp).abs() <= bound,
                "{ctx}: expected {exp}, got {actual} (rel tol={tol})"
            );
        }
    }
}

fn check(actual: f64, case: &TestCase, module: &str, func: &str) {
    let ctx = format!("{module}::{func}::{}", case.name);
    assert_close(actual, case.expected, case.tolerance.unwrap_or(1e-9), &ctx);
}

fn check_map_value(
    map: &HashMap<String, Option<f64>>,
    key: &str,
    actual: f64,
    tol: f64,
    ctx: &str,
) {
    let expected = map
        .get(key)
        .copied()
        .unwrap_or_else(|| panic!("{ctx}: fixture missing expected key {key}"));
    assert_close(actual, expected, tol, &format!("{ctx}::{key}"));
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
        check(
            toolbox::common::mean(&v_vec(&case.inputs, "values")),
            case,
            "common",
            "mean",
        );
        count += 1;
    }
    for case in &g.modules.common["std_dev"] {
        check(
            toolbox::common::std_dev(&v_vec(&case.inputs, "values")),
            case,
            "common",
            "std_dev",
        );
        count += 1;
    }
    for case in &g.modules.common["minus"] {
        check(
            toolbox::common::minus(v(&case.inputs, "a"), v(&case.inputs, "b")),
            case,
            "common",
            "minus",
        );
        count += 1;
    }
    for case in &g.modules.common["equals"] {
        check(
            toolbox::common::equals(v(&case.inputs, "primary"), v(&case.inputs, "fallback")),
            case,
            "common",
            "equals",
        );
        count += 1;
    }
    for case in &g.modules.common["apply_standard_curve"] {
        check(
            toolbox::common::apply_standard_curve(
                v(&case.inputs, "raw"),
                v(&case.inputs, "slope"),
                v(&case.inputs, "intercept"),
            ),
            case,
            "common",
            "apply_standard_curve",
        );
        count += 1;
    }
    for case in &g.modules.common["ratio"] {
        check(
            toolbox::common::ratio(v(&case.inputs, "dividend"), v(&case.inputs, "divisor")),
            case,
            "common",
            "ratio",
        );
        count += 1;
    }
    eprintln!("common: {count} passed");
}

#[test]
fn golden_tss_afdm() {
    let g = fixture();
    let mut count = 0;
    for case in &g.modules.tss_afdm["tss_mg_l"] {
        let result = toolbox::tss_afdm::tss_mg_l(
            v(&case.inputs, "wgt_dried"),
            v(&case.inputs, "wgt_prefilt"),
            v(&case.inputs, "vol_filtered"),
        );
        check(result, case, "tss_afdm", "tss_mg_l");
        count += 1;
    }
    for case in &g.modules.tss_afdm["afdm_mg_l"] {
        let result = toolbox::tss_afdm::afdm_mg_l(
            v(&case.inputs, "wgt_dried"),
            v(&case.inputs, "wgt_ashed"),
            v(&case.inputs, "vol_filtered"),
        );
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
        check(
            toolbox::dom::suva(v(&case.inputs, "a254"), v(&case.inputs, "doc_avg_ppb")),
            case,
            "dom",
            "suva",
        );
        count += 1;
    }
    for case in &g.modules.dom["absorbance_ratio"] {
        check(
            toolbox::dom::absorbance_ratio(
                v(&case.inputs, "numerator"),
                v(&case.inputs, "denominator"),
            ),
            case,
            "dom",
            "absorbance_ratio",
        );
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
        check(
            toolbox::doc::doc_average(&reps, curve),
            case,
            "doc",
            "doc_average",
        );
        count += 1;
    }
    for case in &g.modules.doc["doc_std_dev"] {
        let reps = v_vec(&case.inputs, "replicates");
        let curve = v_opt(&case.inputs, "slope").zip(v_opt(&case.inputs, "intercept"));
        check(
            toolbox::doc::doc_std_dev(&reps, curve),
            case,
            "doc",
            "doc_std_dev",
        );
        count += 1;
    }
    eprintln!("doc: {count} passed");
}

#[test]
fn golden_chlorophyll() {
    let g = fixture();
    let mut count = 0;
    for case in &g.modules.chlorophyll["chla_acid"] {
        let result = toolbox::chlorophyll::chla_acid(
            v(&case.inputs, "fluor_before"),
            v(&case.inputs, "fluor_after"),
            v(&case.inputs, "slope"),
            v(&case.inputs, "intercept"),
        );
        check(result, case, "chlorophyll", "chla_acid");
        count += 1;
    }
    for case in &g.modules.chlorophyll["chla_no_acid"] {
        let result = toolbox::chlorophyll::chla_no_acid(
            v(&case.inputs, "fluor"),
            v(&case.inputs, "slope"),
            v(&case.inputs, "intercept"),
        );
        check(result, case, "chlorophyll", "chla_no_acid");
        count += 1;
    }
    eprintln!("chlorophyll: {count} passed");
}

#[test]
fn golden_chla_benthic_replicates() {
    let g = fixture();
    let mut count = 0;
    for case in &g.modules.chlorophyll["chla_benthic_replicates"] {
        let ctx = format!("chlorophyll::chla_benthic_replicates::{}", case.name);
        let tol = case.tolerance.unwrap_or(1e-9);
        let map = case.expected_map.as_ref().expect("expected_map");

        let inputs: Vec<toolbox::ChlaReplicateInput> = case.inputs["replicates"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| toolbox::ChlaReplicateInput {
                fluor_before: v(r, "fluor_before"),
                fluor_after: v_opt(r, "fluor_after"),
                vol_total_ml: v(r, "vol_total_ml"),
                vol_after_ml: v(r, "vol_after_ml"),
                diameters_cm: v_vec(r, "diameters_cm"),
                afdm_g_filter: v_opt(r, "afdm_g_filter"),
            })
            .collect();

        let result = toolbox::chla_benthic_replicates(
            &inputs,
            v(&case.inputs, "acid_slope"),
            v(&case.inputs, "acid_intercept"),
            v(&case.inputs, "noacid_slope"),
            v(&case.inputs, "noacid_intercept"),
        );

        let opt = |o: Option<f64>| o.unwrap_or(f64::NAN);
        check_map_value(
            map,
            "chla_noacid_ug_l_avg",
            result.chla_noacid_ug_l_avg,
            tol,
            &ctx,
        );
        check_map_value(
            map,
            "chla_noacid_ug_l_sd",
            result.chla_noacid_ug_l_sd,
            tol,
            &ctx,
        );
        check_map_value(
            map,
            "chla_noacid_ug_m2_avg",
            result.chla_noacid_ug_m2_avg,
            tol,
            &ctx,
        );
        check_map_value(
            map,
            "chla_noacid_ug_m2_sd",
            result.chla_noacid_ug_m2_sd,
            tol,
            &ctx,
        );
        if map.contains_key("chla_acid_ug_l_avg") {
            check_map_value(
                map,
                "chla_acid_ug_l_avg",
                opt(result.chla_acid_ug_l_avg),
                tol,
                &ctx,
            );
            check_map_value(
                map,
                "chla_acid_ug_l_sd",
                opt(result.chla_acid_ug_l_sd),
                tol,
                &ctx,
            );
            check_map_value(
                map,
                "chla_acid_ug_m2_avg",
                opt(result.chla_acid_ug_m2_avg),
                tol,
                &ctx,
            );
            check_map_value(
                map,
                "chla_acid_ug_m2_sd",
                opt(result.chla_acid_ug_m2_sd),
                tol,
                &ctx,
            );
        } else {
            assert!(
                result.chla_acid_ug_l_avg.is_none(),
                "{ctx}: expected no acid results"
            );
        }
        if map.contains_key("afdm_g_m2_avg") {
            check_map_value(map, "afdm_g_m2_avg", opt(result.afdm_g_m2_avg), tol, &ctx);
            check_map_value(map, "afdm_g_m2_sd", opt(result.afdm_g_m2_sd), tol, &ctx);
        } else {
            assert!(
                result.afdm_g_m2_avg.is_none(),
                "{ctx}: expected no AFDM results"
            );
        }
        count += 1;
    }
    eprintln!("chla_benthic_replicates: {count} passed");
}

#[test]
fn golden_benthic() {
    let g = fixture();
    let mut count = 0;
    for case in &g.modules.benthic["rock_surface_area_m2"] {
        check(
            toolbox::benthic::rock_surface_area_m2(&v_vec(&case.inputs, "dims_cm")),
            case,
            "benthic",
            "rock_surface_area_m2",
        );
        count += 1;
    }
    for case in &g.modules.benthic["per_m2"] {
        let dims = v_vec(&case.inputs, "dims_cm");
        let area = toolbox::benthic::rock_surface_area_m2(&dims);
        check(
            toolbox::benthic::per_m2(
                v(&case.inputs, "sample_value"),
                v(&case.inputs, "vol_total"),
                v(&case.inputs, "vol_filtrated"),
                area,
            ),
            case,
            "benthic",
            "per_m2",
        );
        count += 1;
    }
    for case in &g.modules.benthic["benthic_afdm_per_m2"] {
        let dims = v_vec(&case.inputs, "dims_cm");
        check(
            toolbox::benthic::benthic_afdm_per_m2(
                v(&case.inputs, "afdm_g"),
                &dims,
                v(&case.inputs, "vol_filtrated"),
                v(&case.inputs, "vol_total"),
            ),
            case,
            "benthic",
            "benthic_afdm_per_m2",
        );
        count += 1;
    }
    for case in &g.modules.benthic["benthic_chla_per_m2"] {
        let dims = v_vec(&case.inputs, "dims_cm");
        check(
            toolbox::benthic::benthic_chla_per_m2(
                v(&case.inputs, "chla_ug_l"),
                &dims,
                v(&case.inputs, "vol_filtrated"),
                v(&case.inputs, "vol_total"),
            ),
            case,
            "benthic",
            "benthic_chla_per_m2",
        );
        count += 1;
    }
    eprintln!("benthic: {count} passed");
}

#[test]
fn golden_field_data() {
    let g = fixture();
    let mut count = 0;
    for case in &g.modules.field_data["barometric_pressure_from_altitude"] {
        check(
            toolbox::field_data::barometric_pressure_from_altitude(
                v(&case.inputs, "elevation_m"),
                v(&case.inputs, "temp_c"),
            ),
            case,
            "field_data",
            "barometric_pressure_from_altitude",
        );
        count += 1;
    }
    for case in &g.modules.field_data["co2_correction"] {
        let curve = v_opt(&case.inputs, "std_slope").zip(v_opt(&case.inputs, "std_intercept"));
        check(
            toolbox::field_data::co2_correction(
                v(&case.inputs, "raw_co2"),
                v(&case.inputs, "pressure_hpa"),
                v(&case.inputs, "temp_c"),
                curve,
            ),
            case,
            "field_data",
            "co2_correction",
        );
        count += 1;
    }
    for case in &g.modules.field_data["reach_depth_stats"] {
        let ctx = format!("field_data::reach_depth_stats::{}", case.name);
        let tol = case.tolerance.unwrap_or(1e-9);
        let map = case.expected_map.as_ref().expect("expected_map");
        let (avg, sd) = toolbox::field_data::reach_depth_stats(&v_vec(&case.inputs, "depths"));
        check_map_value(map, "avg", avg, tol, &ctx);
        check_map_value(map, "sd", sd, tol, &ctx);
        count += 1;
    }
    for case in &g.modules.field_data["select_pressure"] {
        let result = toolbox::field_data::select_pressure(
            v_opt(&case.inputs, "field_pressure"),
            v_opt(&case.inputs, "altitude_pressure"),
        );
        check(
            result.unwrap_or(f64::NAN),
            case,
            "field_data",
            "select_pressure",
        );
        count += 1;
    }
    eprintln!("field_data: {count} passed");
}

#[test]
fn golden_pco2() {
    let g = fixture();
    let mut count = 0;

    for case in &g.modules.pco2["ch4_dry"] {
        check(
            toolbox::pco2::ch4_dry(v(&case.inputs, "ch4_wet"), v(&case.inputs, "h2o_percent")),
            case,
            "pco2",
            "ch4_dry",
        );
        count += 1;
    }
    for case in &g.modules.pco2["pco2_from_co2aq"] {
        let gc = toolbox::pco2::GasConstants {
            c_const: v(&case.inputs, "c_const"),
            ..Default::default()
        };
        check(
            toolbox::pco2::pco2_from_co2aq(
                v(&case.inputs, "co2_aq"),
                v(&case.inputs, "water_temp_c"),
                &gc,
            ),
            case,
            "pco2",
            "pco2_from_co2aq",
        );
        count += 1;
    }
    for case in &g.modules.pco2["pco2_p1"] {
        let gc = toolbox::pco2::GasConstants {
            c_const: v(&case.inputs, "c_const"),
            ..Default::default()
        };
        check(
            toolbox::pco2::pco2_p1(
                v(&case.inputs, "co2_aq"),
                v(&case.inputs, "water_temp_c"),
                v(&case.inputs, "bp_hpa"),
                &gc,
            ),
            case,
            "pco2",
            "pco2_p1",
        );
        count += 1;
    }
    for case in &g.modules.pco2["pco2_p2"] {
        let gc = toolbox::pco2::GasConstants {
            c_const: v(&case.inputs, "c_const"),
            ..Default::default()
        };
        check(
            toolbox::pco2::pco2_p2(
                v(&case.inputs, "co2_aq"),
                v(&case.inputs, "water_temp_c"),
                v(&case.inputs, "bp_hpa"),
                &gc,
            ),
            case,
            "pco2",
            "pco2_p2",
        );
        count += 1;
    }
    for case in &g.modules.pco2["dissolved_ch4"] {
        let gc = toolbox::pco2::GasConstants {
            h_ch4_29815k: v(&case.inputs, "h_ch4_29815k"),
            ch4_in_sa: v(&case.inputs, "ch4_in_sa"),
            gas_const_r_mol: v(&case.inputs, "gas_const_r_mol"),
            ..Default::default()
        };
        check(
            toolbox::pco2::dissolved_ch4(
                v(&case.inputs, "ch4_dry"),
                v(&case.inputs, "water_temp_c"),
                v(&case.inputs, "bp_hpa"),
                v(&case.inputs, "lab_temp_c"),
                &gc,
            ),
            case,
            "pco2",
            "dissolved_ch4",
        );
        count += 1;
    }
    eprintln!("pco2: {count} passed");
}

#[test]
fn golden_pco2_replicates() {
    let g = fixture();
    let mut count = 0;
    for case in &g.modules.pco2["pco2_replicates"] {
        let ctx = format!("pco2::pco2_replicates::{}", case.name);
        let tol = case.tolerance.unwrap_or(1e-9);
        let map = case.expected_map.as_ref().expect("expected_map");

        let gc = toolbox::pco2::GasConstants {
            c_const: v(&case.inputs, "c_const"),
            gas_const_r_atm: v(&case.inputs, "gas_const_r_atm"),
            gas_const_r_mol: v(&case.inputs, "gas_const_r_mol"),
            h_ch4_29815k: v(&case.inputs, "h_ch4_29815k"),
            ch4_in_sa: v(&case.inputs, "ch4_in_sa"),
        };
        let make_input = |rep: &serde_json::Value| toolbox::Pco2FullInput {
            co2_ppm: v(rep, "co2_ppm"),
            h2o_percent: v(rep, "h2o_percent"),
            ch4_ppm: v(rep, "ch4_ppm"),
            d13co2_permil: None,
            lab_temp_c: v(&case.inputs, "lab_temp_c"),
            lab_pressure_atm: v(&case.inputs, "lab_pressure_atm"),
            vol_sa_ml: v(&case.inputs, "vol_sa_ml"),
            vol_water_ml: v(&case.inputs, "vol_water_ml"),
            water_temp_c: v(&case.inputs, "water_temp_c"),
            field_pressure_hpa: v(&case.inputs, "bp_hpa"),
        };
        let rep = toolbox::pco2_replicates(
            &make_input(&case.inputs["a"]),
            &make_input(&case.inputs["b"]),
            &gc,
        );

        check_map_value(map, "co2_hs_a", rep.a.co2_hs_umol, tol, &ctx);
        check_map_value(map, "co2_hs_b", rep.b.co2_hs_umol, tol, &ctx);
        check_map_value(map, "co2_hs_avg", rep.co2_hs_umol_avg, tol, &ctx);
        check_map_value(map, "co2_hs_sd", rep.co2_hs_umol_sd, tol, &ctx);
        check_map_value(map, "pco2_avg", rep.pco2_uatm_avg, tol, &ctx);
        check_map_value(map, "pco2_sd", rep.pco2_uatm_sd, tol, &ctx);
        check_map_value(map, "pco2_p1_avg", rep.pco2_p1_uatm_avg, tol, &ctx);
        check_map_value(map, "pco2_p1_sd", rep.pco2_p1_uatm_sd, tol, &ctx);
        check_map_value(map, "pco2_p2_avg", rep.pco2_p2_uatm_avg, tol, &ctx);
        check_map_value(map, "pco2_p2_sd", rep.pco2_p2_uatm_sd, tol, &ctx);
        check_map_value(map, "ch4_dry_avg", rep.ch4_dry_ppm_avg, tol, &ctx);
        check_map_value(map, "ch4_dry_sd", rep.ch4_dry_ppm_sd, tol, &ctx);
        check_map_value(
            map,
            "ch4_dissolved_avg",
            rep.ch4_dissolved_umol_avg,
            tol,
            &ctx,
        );
        check_map_value(
            map,
            "ch4_dissolved_sd",
            rep.ch4_dissolved_umol_sd,
            tol,
            &ctx,
        );
        count += 1;
    }
    eprintln!("pco2_replicates: {count} passed");
}

#[test]
fn golden_co2_air() {
    let g = fixture();
    let mut count = 0;
    for case in &g.modules.co2_air["co2_headspace"] {
        let gc = toolbox::pco2::GasConstants {
            c_const: v(&case.inputs, "c_const"),
            gas_const_r_atm: v(&case.inputs, "gas_const_r_atm"),
            ..Default::default()
        };
        check(
            toolbox::co2_air::co2_headspace(
                v(&case.inputs, "co2_ppm"),
                v(&case.inputs, "lab_temp_c"),
                v(&case.inputs, "lab_pressure_atm"),
                v(&case.inputs, "vol_sa_ml"),
                v(&case.inputs, "vol_water_ml"),
                &gc,
            ),
            case,
            "co2_air",
            "co2_headspace",
        );
        count += 1;
    }
    eprintln!("co2_air: {count} passed");
}

fn dic_constants(inputs: &serde_json::Value) -> toolbox::dic::DICConstants {
    toolbox::dic::DICConstants {
        h_co2_29815k: v(inputs, "h_co2_29815k"),
        gas_const_r_mol: v(inputs, "gas_const_r_mol"),
        vial_volume: v(inputs, "vial_volume"),
        h3po4_added: v(inputs, "h3po4_added"),
    }
}

#[test]
fn golden_dic() {
    let g = fixture();
    let mut count = 0;
    for case in &g.modules.dic["dic_concentration"] {
        let dc = dic_constants(&case.inputs);
        check(
            toolbox::dic::dic_concentration(
                v(&case.inputs, "acid_sample_wght"),
                v(&case.inputs, "acid_wght"),
                v(&case.inputs, "vol_overpressure"),
                v(&case.inputs, "sa_added"),
                v(&case.inputs, "co2_dry"),
                v(&case.inputs, "air_temp_c"),
                &dc,
            ),
            case,
            "dic",
            "dic_concentration",
        );
        count += 1;
    }
    for case in &g.modules.dic["d13c_dic"] {
        let dc = dic_constants(&case.inputs);
        check(
            toolbox::dic::d13c_dic(
                v(&case.inputs, "acid_sample_wght"),
                v(&case.inputs, "acid_wght"),
                v(&case.inputs, "vol_overpressure"),
                v(&case.inputs, "delta_13co2"),
                v(&case.inputs, "air_temp_c"),
                &dc,
            ),
            case,
            "dic",
            "d13c_dic",
        );
        count += 1;
    }
    for case in &g.modules.dic["dic"] {
        let ctx = format!("dic::dic::{}", case.name);
        let tol = case.tolerance.unwrap_or(1e-9);
        let map = case.expected_map.as_ref().expect("expected_map");
        let dc = dic_constants(&case.inputs);
        let r = toolbox::dic::dic(
            v(&case.inputs, "acid_sample_wght"),
            v(&case.inputs, "acid_wght"),
            v(&case.inputs, "vol_overpressure"),
            v(&case.inputs, "sa_added"),
            v(&case.inputs, "co2_dry"),
            v(&case.inputs, "delta_13co2"),
            v(&case.inputs, "air_temp_c"),
            &dc,
        );
        check_map_value(map, "dic", r.dic_umol_l, tol, &ctx);
        check_map_value(map, "d13c", r.d13c_dic_permil, tol, &ctx);
        count += 1;
    }
    eprintln!("dic: {count} passed");
}

#[test]
fn golden_dic_replicates() {
    let g = fixture();
    let mut count = 0;
    for case in &g.modules.dic["dic_replicates"] {
        let ctx = format!("dic::dic_replicates::{}", case.name);
        let tol = case.tolerance.unwrap_or(1e-9);
        let map = case.expected_map.as_ref().expect("expected_map");
        let dc = dic_constants(&case.inputs);
        let a = &case.inputs["a"];
        let b = &case.inputs["b"];

        let rep = toolbox::dic::dic_replicates(
            v(a, "acid_sample_wght"),
            v(a, "acid_wght"),
            v(a, "vol_overpressure"),
            v(a, "sa_added"),
            v(a, "co2_dry"),
            v_opt(a, "delta_13co2"),
            v(b, "acid_sample_wght"),
            v(b, "acid_wght"),
            v(b, "vol_overpressure"),
            v(b, "sa_added"),
            v(b, "co2_dry"),
            v_opt(b, "delta_13co2"),
            v(&case.inputs, "air_temp_c"),
            &dc,
        );

        let opt = |o: Option<f64>| o.unwrap_or(f64::NAN);
        check_map_value(map, "dic_a", rep.dic_a, tol, &ctx);
        check_map_value(map, "dic_b", rep.dic_b, tol, &ctx);
        check_map_value(map, "dic_avg", rep.dic_avg, tol, &ctx);
        check_map_value(map, "dic_std", rep.dic_std, tol, &ctx);
        if a.get("delta_13co2").is_some_and(|d| !d.is_null())
            && b.get("delta_13co2").is_some_and(|d| !d.is_null())
        {
            check_map_value(map, "d13c_a", opt(rep.d13c_a), tol, &ctx);
            check_map_value(map, "d13c_b", opt(rep.d13c_b), tol, &ctx);
            check_map_value(map, "d13c_avg", opt(rep.d13c_avg), tol, &ctx);
            check_map_value(map, "d13c_std", opt(rep.d13c_std), tol, &ctx);
        }
        count += 1;
    }
    eprintln!("dic_replicates: {count} passed");
}

#[test]
fn golden_nutrients() {
    let g = fixture();
    let mut count = 0;
    for case in &g.modules.nutrients["multi_nutrient_replicates"] {
        let ctx = format!("nutrients::multi_nutrient_replicates::{}", case.name);
        let tol = case.tolerance.unwrap_or(1e-9);
        let map = case.expected_map.as_ref().expect("expected_map");

        let species: HashMap<String, Vec<f64>> = case.inputs["species"]
            .as_object()
            .unwrap()
            .keys()
            .map(|k| (k.clone(), v_vec(&case.inputs["species"], k)))
            .collect();
        let results = toolbox::multi_nutrient_replicates(&species);

        for (name, _) in case.inputs["species"].as_object().unwrap() {
            let nr = results
                .get(name)
                .unwrap_or_else(|| panic!("{ctx}: missing species result {name}"));
            check_map_value(map, &format!("{name}_avg"), nr.mean, tol, &ctx);
            check_map_value(map, &format!("{name}_sd"), nr.std_dev, tol, &ctx);
        }
        let no3 = results
            .get("NO3")
            .unwrap_or_else(|| panic!("{ctx}: missing NO3"));
        check_map_value(map, "NO3_avg", no3.mean, tol, &ctx);
        check_map_value(map, "NO3_sd", no3.std_dev, tol, &ctx);
        count += 1;
    }
    for case in &g.modules.nutrients["nutrient_from_replicates"] {
        let ctx = format!("nutrients::nutrient_from_replicates::{}", case.name);
        let tol = case.tolerance.unwrap_or(1e-9);
        let map = case.expected_map.as_ref().expect("expected_map");
        let nr = toolbox::nutrient_from_replicates(&v_vec(&case.inputs, "replicates"));
        check_map_value(map, "avg", nr.mean, tol, &ctx);
        check_map_value(map, "sd", nr.std_dev, tol, &ctx);
        count += 1;
    }
    for case in &g.modules.nutrients["nitrate_from_nox_no2"] {
        check(
            toolbox::nitrate_from_nox_no2(v(&case.inputs, "nox"), v(&case.inputs, "no2")),
            case,
            "nutrients",
            "nitrate_from_nox_no2",
        );
        count += 1;
    }
    eprintln!("nutrients: {count} passed");
}

#[test]
fn golden_pco2_full_pipeline() {
    let g = fixture();
    let mut count = 0;
    for case in &g.modules.pco2["pco2_full_pipeline"] {
        let ctx = format!("pco2::pco2_full_pipeline::{}", case.name);
        let tol = case.tolerance.unwrap_or(1e-9);
        let map = case.expected_map.as_ref().expect("expected_map");

        let gc = toolbox::pco2::GasConstants {
            c_const: v(&case.inputs, "c_const"),
            gas_const_r_atm: v(&case.inputs, "gas_const_r_atm"),
            gas_const_r_mol: v(&case.inputs, "gas_const_r_mol"),
            h_ch4_29815k: v(&case.inputs, "h_ch4_29815k"),
            ch4_in_sa: v(&case.inputs, "ch4_in_sa"),
        };
        let input = toolbox::Pco2FullInput {
            co2_ppm: v(&case.inputs, "co2_ppm"),
            h2o_percent: v(&case.inputs, "h2o_percent"),
            ch4_ppm: v(&case.inputs, "ch4_ppm"),
            d13co2_permil: None,
            lab_temp_c: v(&case.inputs, "lab_temp_c"),
            lab_pressure_atm: v(&case.inputs, "lab_pressure_atm"),
            vol_sa_ml: v(&case.inputs, "vol_sa_ml"),
            vol_water_ml: v(&case.inputs, "vol_water_ml"),
            water_temp_c: v(&case.inputs, "water_temp_c"),
            field_pressure_hpa: v(&case.inputs, "bp_hpa"),
        };
        let r = toolbox::pco2_full_pipeline(&input, &gc);

        check_map_value(map, "co2_hs", r.co2_hs_umol, tol, &ctx);
        check_map_value(map, "pco2", r.pco2_uatm, tol, &ctx);
        check_map_value(map, "pco2_p1", r.pco2_p1_uatm, tol, &ctx);
        check_map_value(map, "pco2_p2", r.pco2_p2_uatm, tol, &ctx);
        check_map_value(map, "ch4_dry", r.ch4_dry_ppm, tol, &ctx);
        check_map_value(map, "ch4_dissolved", r.ch4_dissolved_umol, tol, &ctx);
        count += 1;
    }
    eprintln!("pco2_full_pipeline: {count} passed");
}
