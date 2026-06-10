//! SimParams と CLI 引数ビルダー
//! gui/src-tauri/src/lib.rs から等価移植

use std::path::Path;

use serde::{Deserialize, Serialize};

fn default_square_duty() -> f64 {
    0.5
}

fn default_n_adf() -> u64 {
    90
}

fn default_n_2adf() -> u64 {
    180
}

/// フロントエンドから受け取る計算条件。すべて CLI 引数に変換する。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimParams {
    pub cycles: u64,
    pub voltage_mode: String,
    pub rf_amplitude_v: f64,
    pub powered_dc_v: f64,
    pub grounded_dc_v: f64,
    #[serde(default = "default_square_duty")]
    pub square_duty: f64,
    #[serde(default)]
    pub square_rise: f64,
    #[serde(default)]
    pub rf2_amplitude_v: f64,
    #[serde(default)]
    pub rf2_frequency_hz: f64,
    pub grid_points: u64,
    pub steps_per_period: u64,
    pub frequency_hz: f64,
    pub gas_model: String,
    pub gap_m: f64,
    pub pressure_pa: f64,
    pub temperature_k: f64,
    pub weight: f64,
    pub electrode_area_m2: f64,
    pub initial_particles: u64,
    pub max_particles: u64,
    pub ion_subcycling: u64,
    pub xt_bin_steps: u64,
    #[serde(default = "default_n_adf")]
    pub n_adf: u64,
    #[serde(default = "default_n_2adf")]
    pub n_2adf: u64,
    pub electron_reflection_probability: f64,
    pub secondary_yield: f64,
    pub secondary_energy_ev: f64,
    pub fn_emission_enabled: bool,
    pub fn_work_function_ev: f64,
    pub fn_field_enhancement: f64,
    pub fn_emission_area_factor: f64,
    pub go2010_ion_enhanced_fn: bool,
    pub go2010_k: f64,
    #[serde(default)]
    pub dt_s: Option<f64>,
    #[serde(default)]
    pub total_time_s: Option<f64>,
    #[serde(default)]
    pub lxcat_path: Option<String>,
    #[serde(default)]
    pub gas_mass_kg: f64,
    #[serde(default)]
    pub ion_mass_kg: f64,
    #[serde(default)]
    pub ion_hs_sigma_iso_m2: f64,
    #[serde(default)]
    pub ion_hs_sigma_back_m2: f64,
    #[serde(default)]
    pub waveform_file_path: Option<String>,
    #[serde(default)]
    pub custom_waveform_data: Option<Vec<(f64, f64)>>,
}

impl Default for SimParams {
    fn default() -> Self {
        Self {
            cycles: 100,
            voltage_mode: "rf".to_string(),
            rf_amplitude_v: 100.0,
            powered_dc_v: 0.0,
            grounded_dc_v: 0.0,
            square_duty: default_square_duty(),
            square_rise: 0.0,
            rf2_amplitude_v: 0.0,
            rf2_frequency_hz: 0.0,
            grid_points: 128,
            steps_per_period: 1000,
            frequency_hz: 13.56e6,
            gas_model: "ar".to_string(),
            gap_m: 0.025,
            pressure_pa: 10.0,
            temperature_k: 300.0,
            weight: 1e7,
            electrode_area_m2: 1e-4,
            initial_particles: 1000,
            max_particles: 100_000,
            ion_subcycling: 10,
            xt_bin_steps: 10,
            n_adf: default_n_adf(),
            n_2adf: default_n_2adf(),
            electron_reflection_probability: 0.0,
            secondary_yield: 0.0,
            secondary_energy_ev: 0.0,
            fn_emission_enabled: false,
            fn_work_function_ev: 4.5,
            fn_field_enhancement: 1.0,
            fn_emission_area_factor: 1.0,
            go2010_ion_enhanced_fn: false,
            go2010_k: 0.0,
            dt_s: None,
            total_time_s: None,
            lxcat_path: None,
            gas_mass_kg: 0.0,
            ion_mass_kg: 0.0,
            ion_hs_sigma_iso_m2: 0.0,
            ion_hs_sigma_back_m2: 0.0,
            waveform_file_path: None,
            custom_waveform_data: None,
        }
    }
}

/// f64 を最短表現文字列に変換する。
pub fn ff(v: f64) -> String {
    format!("{}", v)
}

/// cycle 文字列と measure フラグから、計算バイナリへの引数列を組み立てる。
pub fn build_args(p: &SimParams, cycle_arg: &str, measure: bool, lxcat_dir: &Path) -> Vec<String> {
    let mut args: Vec<String> = vec![cycle_arg.to_string()];
    if measure {
        args.push("m".to_string());
    }

    args.push("--rf-amplitude-v".into());
    args.push(ff(p.rf_amplitude_v));
    if p.voltage_mode != "rf" {
        args.push("--voltage-mode".into());
        args.push(p.voltage_mode.clone());
    }
    if matches!(p.voltage_mode.as_str(), "square" | "sin-sin" | "square-sin") {
        args.push("--square-duty".into());
        args.push(ff(p.square_duty / 100.0));
        args.push("--square-rise".into());
        args.push(ff(p.square_rise / 100.0));
    }
    args.push("--powered-dc-v".into());
    args.push(ff(p.powered_dc_v));
    args.push("--grounded-dc-v".into());
    args.push(ff(p.grounded_dc_v));

    if p.rf2_amplitude_v != 0.0 && p.rf2_frequency_hz > 0.0 && p.frequency_hz > 0.0 {
        args.push("--rf2-amplitude-v".into());
        args.push(ff(p.rf2_amplitude_v));
        args.push("--rf2-frequency-ratio".into());
        args.push(ff(p.rf2_frequency_hz / p.frequency_hz));
    }

    let pairs: Vec<(&str, String)> = vec![
        ("--grid-points", p.grid_points.to_string()),
        ("--steps-per-period", p.steps_per_period.to_string()),
        ("--frequency-hz", ff(p.frequency_hz)),
        ("--gas-model", p.gas_model.clone()),
        ("--gap-m", ff(p.gap_m)),
        ("--pressure-pa", ff(p.pressure_pa)),
        ("--temperature-k", ff(p.temperature_k)),
        ("--weight", ff(p.weight)),
        ("--electrode-area-m2", ff(p.electrode_area_m2)),
        ("--initial-particles", p.initial_particles.to_string()),
        ("--max-particles", p.max_particles.to_string()),
        ("--ion-subcycling", p.ion_subcycling.to_string()),
        ("--xt-bin-steps", p.xt_bin_steps.to_string()),
        ("--n-adf", p.n_adf.to_string()),
        ("--n-2adf", p.n_2adf.to_string()),
        (
            "--electron-reflection-probability",
            ff(p.electron_reflection_probability),
        ),
        ("--secondary-yield", ff(p.secondary_yield)),
        ("--secondary-energy-ev", ff(p.secondary_energy_ev)),
        ("--fn-emission-enabled", p.fn_emission_enabled.to_string()),
        ("--fn-work-function-ev", ff(p.fn_work_function_ev)),
        ("--fn-field-enhancement", ff(p.fn_field_enhancement)),
        ("--fn-emission-area-factor", ff(p.fn_emission_area_factor)),
        (
            "--go2010-ion-enhanced-fn",
            p.go2010_ion_enhanced_fn.to_string(),
        ),
        ("--go2010-k", ff(p.go2010_k)),
    ];
    for (k, v) in pairs {
        args.push(k.to_string());
        args.push(v);
    }

    if let Some(dt) = p.dt_s {
        args.push("--dt-s".into());
        args.push(ff(dt));
    }
    if let Some(tt) = p.total_time_s {
        args.push("--total-time-s".into());
        args.push(ff(tt));
    }

    if p.gas_model == "lxcat" {
        if let Some(filename) = &p.lxcat_path {
            if !filename.is_empty() {
                args.push("--lxcat-file".into());
                args.push(lxcat_dir.join(filename).to_string_lossy().into_owned());
            }
        }
        if p.gas_mass_kg > 0.0 {
            args.push("--gas-mass-kg".into());
            args.push(ff(p.gas_mass_kg));
        }
        if p.ion_mass_kg > 0.0 {
            args.push("--ion-mass-kg".into());
            args.push(ff(p.ion_mass_kg));
        }
        if p.ion_hs_sigma_iso_m2 > 0.0 {
            args.push("--ion-hs-sigma-iso-m2".into());
            args.push(ff(p.ion_hs_sigma_iso_m2));
        }
        if p.ion_hs_sigma_back_m2 > 0.0 {
            args.push("--ion-hs-sigma-back-m2".into());
            args.push(ff(p.ion_hs_sigma_back_m2));
        }
    }

    if p.voltage_mode == "custom" {
        if let Some(waveform_path) = &p.waveform_file_path {
            args.push("--waveform-file".into());
            args.push(waveform_path.clone());
        }
    }

    args
}
