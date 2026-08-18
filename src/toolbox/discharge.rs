//! Slug-injection discharge calculator, ported from the CNET portal's discharge
//! tool (`app/modules/tools_tab/tools/discharge_tool.R`, vendored as
//! `r_reference/functions/dischargeTool.R`).
//!
//! The portal's CSV upload, time-format detection and plotting are not ported:
//! the caller supplies the already-selected injection window as sample times in
//! seconds plus the tracer series.

use serde::{Deserialize, Serialize};

/// Calculation parameters. Defaults are the portal UI defaults.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DischargeParams {
    /// Initial mass of rhodamine solution injected (g).
    pub initial_mass_rhodamine_wt_g: f64,
    /// Rhodamine concentration of the solution (%).
    pub concentration_rhodamine_pct: f64,
    /// Stream water temperature at injection (degC).
    pub initial_water_temp_c: f64,
    /// Rhodamine temperature correction factor.
    pub n_rhodamine: f64,
    /// Initial mass of salt injected (g).
    pub initial_mass_salt_g: f64,
    /// Conductivity-to-concentration slope ((uS/cm)/(g/L)).
    pub slope_conductivity: f64,
    /// Injection-to-probe distance (m).
    pub distance_m: f64,
    /// Reference temperature for the rhodamine correction (degC).
    pub t_ref_c: f64,
}

impl Default for DischargeParams {
    fn default() -> Self {
        Self {
            initial_mass_rhodamine_wt_g: 3.38019,
            concentration_rhodamine_pct: 23.83,
            initial_water_temp_c: 3.3,
            n_rhodamine: 0.026,
            initial_mass_salt_g: 2000.0,
            slope_conductivity: 1951.1,
            distance_m: 79.0,
            t_ref_c: 25.0,
        }
    }
}

/// Result for one tracer. Concentration units are ppb for rhodamine, g/L for salt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TracerResult {
    pub peak_concentration: f64,
    pub travel_time_s: f64,
    pub velocity_m_s: f64,
    pub discharge_l_s: f64,
    /// Background-corrected, smoothed, non-negative concentration series.
    pub smoothed: Vec<f64>,
}

/// Rhodamine slug discharge from a fluorometer series (ppb).
pub fn discharge_rhodamine(
    times_s: &[f64],
    rhodamine_ppb: &[f64],
    params: &DischargeParams,
) -> Result<TracerResult, String> {
    let corrected = background_corrected(times_s, rhodamine_ppb, 15)?;
    let temp_factor = (params.n_rhodamine * (params.initial_water_temp_c - params.t_ref_c)).exp();
    let concentration: Vec<f64> = corrected.iter().map(|c| c * temp_factor).collect();
    // Injected rhodamine mass in mg: solution g * concentration fraction * 1000
    let mass_mg =
        params.initial_mass_rhodamine_wt_g * (params.concentration_rhodamine_pct / 100.0) * 1000.0;
    // AUC is ppb*s = ug/L*s; /1000 makes it mg/L*s so mass_mg / it is L/s
    finish(times_s, &concentration, mass_mg, 1000.0, params.distance_m)
}

/// Salt slug discharge from a conductivity series (uS/cm).
pub fn discharge_salt(
    times_s: &[f64],
    conductivity_us_cm: &[f64],
    params: &DischargeParams,
) -> Result<TracerResult, String> {
    let corrected = background_corrected(times_s, conductivity_us_cm, 10)?;
    let concentration: Vec<f64> = corrected
        .iter()
        .map(|c| c / params.slope_conductivity)
        .collect();
    finish(
        times_s,
        &concentration,
        params.initial_mass_salt_g,
        1.0,
        params.distance_m,
    )
}

/// Smooth, clamp, integrate and derive the summary metrics.
fn finish(
    times_s: &[f64],
    concentration: &[f64],
    mass: f64,
    auc_divisor: f64,
    distance_m: f64,
) -> Result<TracerResult, String> {
    let n = concentration.len().min(11);
    let mut smoothed = sgolay_smooth(concentration, 3, n)?;
    for v in &mut smoothed {
        if *v < 0.0 {
            *v = 0.0;
        }
    }
    let rel: Vec<f64> = times_s.iter().map(|t| t - times_s[0]).collect();
    let auc = trapz(&rel, &smoothed) / auc_divisor;
    let discharge_l_s = if auc == 0.0 { f64::NAN } else { mass / auc };

    let (peak_idx, peak_concentration) =
        smoothed
            .iter()
            .copied()
            .enumerate()
            .fold(
                (0, f64::NEG_INFINITY),
                |acc, (i, v)| {
                    if v > acc.1 { (i, v) } else { acc }
                },
            );
    let travel_time_s = rel[peak_idx];
    let velocity_m_s = if travel_time_s == 0.0 {
        f64::NAN
    } else {
        distance_m / travel_time_s
    };

    Ok(TracerResult {
        peak_concentration,
        travel_time_s,
        velocity_m_s,
        discharge_l_s,
        smoothed,
    })
}

/// Subtract the linear background fitted over the first `head` and last 10 samples
/// (indices kept with duplicates, as the portal's `lm` call sees them).
fn background_corrected(times: &[f64], values: &[f64], head: usize) -> Result<Vec<f64>, String> {
    if times.len() != values.len() {
        return Err("times and values length mismatch".to_string());
    }
    let len = values.len();
    if len < 2 {
        return Err("series too short".to_string());
    }
    if times.iter().chain(values).any(|v| !v.is_finite()) {
        return Err("series contains non-finite values".to_string());
    }

    let mut idx: Vec<usize> = (0..head.min(len)).collect();
    idx.extend(len.saturating_sub(10)..len);

    let (intercept, slope) = linear_fit(
        &idx.iter().map(|&i| times[i]).collect::<Vec<_>>(),
        &idx.iter().map(|&i| values[i]).collect::<Vec<_>>(),
    )?;

    Ok(values
        .iter()
        .zip(times)
        .map(|(y, t)| y - (intercept + slope * t))
        .collect())
}

/// Ordinary least-squares line fit, returning (intercept, slope).
fn linear_fit(xs: &[f64], ys: &[f64]) -> Result<(f64, f64), String> {
    let n = xs.len() as f64;
    let mean_x = xs.iter().sum::<f64>() / n;
    let mean_y = ys.iter().sum::<f64>() / n;
    let sxx: f64 = xs.iter().map(|x| (x - mean_x).powi(2)).sum();
    if sxx == 0.0 {
        return Err("background window has no time spread".to_string());
    }
    let sxy: f64 = xs
        .iter()
        .zip(ys)
        .map(|(x, y)| (x - mean_x) * (y - mean_y))
        .sum();
    let slope = sxy / sxx;
    Ok((mean_y - slope * mean_x, slope))
}

/// Trapezoidal integration (pracma `trapz`).
fn trapz(x: &[f64], y: &[f64]) -> f64 {
    x.windows(2)
        .zip(y.windows(2))
        .map(|(xs, ys)| (xs[1] - xs[0]) * (ys[0] + ys[1]) / 2.0)
        .sum()
}

/// Savitzky-Golay smoothing matching R `signal::sgolayfilt(x, p, n)`:
/// interior samples use the centred filter, the first and last `n/2` samples use
/// the polynomial edge rows of the projection matrix.
fn sgolay_smooth(y: &[f64], p: usize, n: usize) -> Result<Vec<f64>, String> {
    if n % 2 != 1 {
        return Err("sgolay needs an odd filter length".to_string());
    }
    if p >= n {
        return Err("sgolay needs filter length larger than polynomial order".to_string());
    }
    let len = y.len();
    if len < n {
        return Err("series shorter than filter length".to_string());
    }
    let f = sgolay_matrix(p, n)?;
    let k = n / 2;

    let mut out = Vec::with_capacity(len);
    for row in f.iter().take(k) {
        out.push(dot(row, &y[..n]));
    }
    for i in k..len - k {
        out.push(dot(&f[k], &y[i - k..i - k + n]));
    }
    for row in f.iter().take(n).skip(k + 1) {
        out.push(dot(row, &y[len - n..]));
    }
    Ok(out)
}

fn dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// Rows 0..=k of the sgolay projection matrix by least squares; the remaining
/// rows are the reversed mirror (R `signal::sgolay`, m = 0).
fn sgolay_matrix(p: usize, n: usize) -> Result<Vec<Vec<f64>>, String> {
    let k = n / 2;
    let mut f = vec![vec![0.0; n]; n];
    for (row, f_row) in f.iter_mut().enumerate().take(k + 1) {
        // C[j][i] = (j - row)^i
        let c: Vec<Vec<f64>> = (0..n)
            .map(|j| {
                (0..=p)
                    .map(|i| (j as f64 - row as f64).powi(i as i32))
                    .collect()
            })
            .collect();
        // Solve (C^T C) x = e0; the filter row is x^T C^T
        let dim = p + 1;
        let mut m = vec![vec![0.0; dim]; dim];
        for (a, ma) in m.iter_mut().enumerate() {
            for (b, v) in ma.iter_mut().enumerate() {
                *v = c.iter().map(|cj| cj[a] * cj[b]).sum();
            }
        }
        let mut rhs = vec![0.0; dim];
        rhs[0] = 1.0;
        let x = solve(&mut m, &mut rhs)?;
        for (dst, cj) in f_row.iter_mut().zip(&c) {
            *dst = dot(&x, cj);
        }
    }
    let (head, tail) = f.split_at_mut(k + 1);
    for (offset, row) in tail.iter_mut().enumerate() {
        // Row k+1+offset mirrors row k-1-offset reversed
        for (dst, src) in row.iter_mut().zip(head[k - 1 - offset].iter().rev()) {
            *dst = *src;
        }
    }
    Ok(f)
}

/// Gaussian elimination with partial pivoting.
fn solve(m: &mut [Vec<f64>], rhs: &mut [f64]) -> Result<Vec<f64>, String> {
    let dim = rhs.len();
    for col in 0..dim {
        let pivot = (col..dim)
            .max_by(|&a, &b| m[a][col].abs().total_cmp(&m[b][col].abs()))
            .unwrap();
        if m[pivot][col] == 0.0 {
            return Err("singular design matrix".to_string());
        }
        m.swap(col, pivot);
        rhs.swap(col, pivot);
        let (upper, lower) = m.split_at_mut(col + 1);
        let pivot_row = &upper[col];
        let pivot_rhs = rhs[col];
        for (offset, row) in lower.iter_mut().enumerate() {
            let factor = row[col] / pivot_row[col];
            for (a, b) in row[col..].iter_mut().zip(&pivot_row[col..]) {
                *a -= factor * b;
            }
            rhs[col + 1 + offset] -= factor * pivot_rhs;
        }
    }
    let mut x = vec![0.0; dim];
    for row in (0..dim).rev() {
        let s: f64 = (row + 1..dim).map(|cc| m[row][cc] * x[cc]).sum();
        x[row] = (rhs[row] - s) / m[row][row];
    }
    Ok(x)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOL: f64 = 1e-9;

    fn cubic_series() -> (Vec<f64>, Vec<f64>) {
        // y = 0.001 * t^3 sampled at t = 0, 10, 20, 30, 40
        (
            vec![0.0, 10.0, 20.0, 30.0, 40.0],
            vec![0.0, 1.0, 8.0, 27.0, 64.0],
        )
    }

    #[test]
    fn test_trapz() {
        // (1-0)*(0+2)/2 + (3-1)*(2+2)/2 = 1 + 4 = 5
        assert!((trapz(&[0.0, 1.0, 3.0], &[0.0, 2.0, 2.0]) - 5.0).abs() < TOL);
    }

    #[test]
    fn test_sgolay_central_row_n5_p3() {
        // Classic SG coefficients for p=3, n=5: (-3, 12, 17, 12, -3)/35
        let f = sgolay_matrix(3, 5).unwrap();
        let expected = [-3.0, 12.0, 17.0, 12.0, -3.0].map(|v| v / 35.0);
        for (a, e) in f[2].iter().zip(expected) {
            assert!((a - e).abs() < TOL, "central row {:?}", f[2]);
        }
        // interior sample: (-3*1 + 12*2 + 17*4 + 12*2 - 3*1)/35 = 110/35
        let out = sgolay_smooth(&[1.0, 2.0, 4.0, 2.0, 1.0, 2.0, 4.0], 3, 5).unwrap();
        assert!((out[2] - 110.0 / 35.0).abs() < TOL, "got {}", out[2]);
    }

    #[test]
    fn test_sgolay_reproduces_cubic() {
        // A cubic-order fit reproduces exact-cubic data at every sample, edges included
        let y: Vec<f64> = (0..13)
            .map(|i| {
                let t = f64::from(i);
                t.powi(3) - 2.0 * t
            })
            .collect();
        let out = sgolay_smooth(&y, 3, 11).unwrap();
        for (a, e) in out.iter().zip(&y) {
            assert!((a - e).abs() < 1e-6, "expected {e}, got {a}");
        }
    }

    #[test]
    fn test_sgolay_rejects_even_or_short() {
        assert!(sgolay_smooth(&[1.0, 2.0, 3.0, 4.0], 3, 4).is_err());
        assert!(sgolay_smooth(&[1.0, 2.0, 3.0], 3, 3).is_err());
    }

    #[test]
    fn test_background_correction_on_cubic() {
        // Linear fit of [0,1,8,27,64] on t=[0..40]: slope = 1540/1000, intercept = 20 - 1.54*20
        // corrected = y - (1.54*t - 10.8) = [10.8, -3.6, -12.0, -8.4, 13.2]
        let (t, y) = cubic_series();
        let corrected = background_corrected(&t, &y, 15).unwrap();
        let expected = [10.8, -3.6, -12.0, -8.4, 13.2];
        for (a, e) in corrected.iter().zip(expected) {
            assert!((a - e).abs() < TOL, "expected {e}, got {a}");
        }
    }

    #[test]
    fn test_discharge_rhodamine_hand_computed() {
        // Corrected series [10.8, -3.6, -12.0, -8.4, 13.2] is an exact cubic, so
        // the p=3 filter passes it through; clamped: [10.8, 0, 0, 0, 13.2]
        // auc = 10*(10.8+0)/2 + 0 + 0 + 10*(0+13.2)/2 = 54 + 66 = 120 ppb*s
        // mass = 2.0 g * 50% * 1000 = 1000 mg; discharge = 1000/(120/1000) = 8333.33 L/s
        // peak = 13.2 at t=40; velocity = 80/40 = 2 m/s
        let (t, y) = cubic_series();
        let params = DischargeParams {
            initial_mass_rhodamine_wt_g: 2.0,
            concentration_rhodamine_pct: 50.0,
            initial_water_temp_c: 25.0,
            t_ref_c: 25.0,
            distance_m: 80.0,
            ..Default::default()
        };
        let r = discharge_rhodamine(&t, &y, &params).unwrap();
        assert!((r.smoothed[0] - 10.8).abs() < TOL);
        assert!((r.smoothed[1]).abs() < TOL);
        assert!((r.discharge_l_s - 1000.0 / 0.12).abs() < 1e-6);
        assert!((r.peak_concentration - 13.2).abs() < TOL);
        assert!((r.travel_time_s - 40.0).abs() < TOL);
        assert!((r.velocity_m_s - 2.0).abs() < TOL);
    }

    #[test]
    fn test_discharge_rhodamine_temperature_factor() {
        // Same series with a temperature correction: every concentration scales by
        // exp(0.026 * (3.3 - 25)), so discharge scales by 1/factor
        let (t, y) = cubic_series();
        let params = DischargeParams {
            initial_mass_rhodamine_wt_g: 2.0,
            concentration_rhodamine_pct: 50.0,
            initial_water_temp_c: 3.3,
            n_rhodamine: 0.026,
            t_ref_c: 25.0,
            distance_m: 80.0,
            ..Default::default()
        };
        let factor = (0.026f64 * (3.3 - 25.0)).exp();
        let r = discharge_rhodamine(&t, &y, &params).unwrap();
        assert!((r.smoothed[0] - 10.8 * factor).abs() < TOL);
        assert!((r.discharge_l_s - 1000.0 / (0.12 * factor)).abs() < 1e-6);
    }

    #[test]
    fn test_discharge_salt_hand_computed() {
        // corrected/slope with slope=2: [5.4, -1.8, -6.0, -4.2, 6.6]; clamped [5.4,0,0,0,6.6]
        // auc = 10*5.4/2 + 10*6.6/2 = 60 g/L*s; discharge = 2000/60 = 33.333 L/s
        let (t, y) = cubic_series();
        let params = DischargeParams {
            initial_mass_salt_g: 2000.0,
            slope_conductivity: 2.0,
            distance_m: 80.0,
            ..Default::default()
        };
        let r = discharge_salt(&t, &y, &params).unwrap();
        assert!((r.discharge_l_s - 2000.0 / 60.0).abs() < 1e-6);
        assert!((r.peak_concentration - 6.6).abs() < TOL);
        assert!((r.velocity_m_s - 2.0).abs() < TOL);
    }

    #[test]
    fn test_flat_series_gives_nan_discharge() {
        let t: Vec<f64> = (0..20).map(f64::from).collect();
        let y = vec![5.0; 20];
        let r = discharge_salt(&t, &y, &DischargeParams::default()).unwrap();
        assert!(r.discharge_l_s.is_nan());
    }

    #[test]
    fn test_length_mismatch_and_constant_time_rejected() {
        assert!(discharge_salt(&[0.0, 1.0], &[1.0], &DischargeParams::default()).is_err());
        let t = vec![5.0; 12];
        let y: Vec<f64> = (0..12).map(f64::from).collect();
        assert!(discharge_salt(&t, &y, &DischargeParams::default()).is_err());
    }
}
