// switching off some compliler warnings
#![allow(non_snake_case)]
#![allow(dead_code)]
#![allow(unused_must_use)]
#![allow(unused_assignments)]

// include required modules
use rand::prelude::*;
use rand_distr::Normal;
use rayon::prelude::*;
use std::env;
use std::fs::{create_dir_all, File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::OnceLock;
use std::time::Instant;

mod lxcat;
mod surface;

// constants

const PI: f64 = 3.141592653589793; // mathematical constant Pi
const TWO_PI: f64 = 2.0 * PI; // two times Pi
const E_CHARGE: f64 = 1.60217662e-19; // electron charge [C]
const EV_TO_J: f64 = E_CHARGE; // eV <-> Joule conversion factor
const E_MASS: f64 = 9.10938356e-31; // mass of electron [kg]
const AR_MASS: f64 = 6.63352090e-26; // mass of argon atom [kg]
const AIR_MASS: f64 = 4.811e-26; // mean mass of dry air molecule [kg]
const MU_ARAR: f64 = AR_MASS / 2.0; // reduced mass of two argon atoms [kg]
const K_BOLTZMANN: f64 = 1.38064852e-23; // Boltzmann's constant [J/K]
const EPSILON0: f64 = 8.85418781e-12; // permittivity of free space [F/m]

// simulation parameters

const N_G: usize = 200; // number of grid points
const N_T: u32 = 4000; // time steps within an RF period
const FREQUENCY: f64 = 13.56e6; // driving frequency [Hz]
const VOLTAGE: f64 = 250.0; // voltage amplitude [V]
const L: f64 = 0.001; // electrode L [m]
const PRESSURE: f64 = 10.0; // gas pressure [Pa]
const TEMPERATURE: f64 = 350.0; // background gas temperature [K]
const WEIGHT: f64 = 7.0e4; // weight of superparticles
const ELECTRODE_AREA: f64 = 1.0e-4; // (fictive) electrode area [m^2]
const N_INIT: usize = 1000; // number of initial electrona and ions
                            // O2 and other electronegative gases consume electrons rapidly via attachment; increase N_INIT or use a different initialization for such gases.
const RESULT_1D_DIR: &str = "result/1d"; // 1D diagnostic output directory
const GO2010_AIR_TOWNSEND_A_PER_PA_M: f64 = 84.38; // air A coefficient [1/(Pa m)]
const GO2010_AIR_TOWNSEND_B_V_PER_PA_M: f64 = 2053.0; // air B coefficient [V/(Pa m)]

// additional (derived) constants

const PERIOD: f64 = 1.0 / FREQUENCY; // RF period length [s]
const DT_E: f64 = PERIOD / (N_T as f64); // electron time step [s]
const N_SUB: u32 = 20; // ions move only in these cycles (subcycling)
const DT_I: f64 = (N_SUB as f64) * DT_E; // ion time step [s]
const DX: f64 = L / ((N_G - 1) as f64); // spatial grid division [m]
const INV_DX: f64 = 1.0 / DX; // inverse of spatial grid size [1/m]
const GAS_DENSITY: f64 = PRESSURE / (K_BOLTZMANN * TEMPERATURE); // background gas gas density [m-3]
const OMEGA: f64 = TWO_PI * FREQUENCY; // angular frequency [rad/s]

// electron and ion cross sections

const N_CS: usize = 5; // total number of processes / cross sections
const E_ELA: usize = 0; // process identifier: electron/elastic
const E_EXC: usize = 1; // process identifier: electron/excitation
const E_ION: usize = 2; // process identifier: electron/ionization
const I_ISO: usize = 3; // process identifier: ion/elastic/isotropic
const I_BACK: usize = 4; // process identifier: ion/elastic/backscattering
const E_EXC_TH: f64 = 11.5; // electron impact excitation threshold [eV]
const E_ION_TH: f64 = 15.8; // electron impact ionization threshold [eV]
const CS_RANGES: usize = 1_000_000; // number of entries in cross section arrays
const DE_CS: f64 = 0.001; // energy division in cross section arrays [eV]

// measurement conditions

const MIN_X: f64 = 0.45 * L; // lower limit of central region
const MAX_X: f64 = 0.55 * L; // upper limit of central region
const N_EEPF: usize = 2000; // number of energy bins in Electron Energy Probability Function (EEPF)
const DE_EEPF: f64 = 0.05; // resolution of EEPF [eV]
const N_FED: usize = 200; // number of energy bins in Flux-Energy Distributions (EFED and IFED)
const DE_FED: f64 = 1.0; // resolution of FEDs (EFED and IFED) [eV]
const N_ADF: usize = 90; // number of angle bins in Ion Angle Distribution Function (IADF) [0ﾂｰ-90ﾂｰ]
const DA_ADF: f64 = 1.0; // resolution of IADF [degree]
const N_BIN: u32 = 20; // number of time steps binned for the XT distributions // number of time steps binned for the XT distributions
const N_XT: usize = (N_T / N_BIN) as usize; // number of spatial bins for the XT distributions

// structure type definitions

#[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug)]
struct ParticleType {
    // coordinates of particles (one spatial, three velocity components)
    x: f64,
    vx: f64,
    vy: f64,
    vz: f64,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AnimData {
    gap_m: f64,
    frames_per_cycle: usize,
    frames: Vec<AnimFrame>,
}

#[derive(serde::Serialize)]
struct AnimFrame {
    t: f64,
    ex: Vec<f64>,
    evx: Vec<f64>,
    ix: Vec<f64>,
    ivx: Vec<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum OneDVoltageMode {
    Rf,        // V_dc + V_amp * cos(φ)
    Square,    // V_dc + V_amp * sq(φ, duty, rise)
    SinSin,    // V_dc + V_amp * cos(φ) * cos(φ)
    SquareSin, // V_dc + V_amp * sq(φ, duty, rise) * cos(φ)
    Dc,        // V_dc
    Custom,    // 外部ファイルから読み込んだ任意波形
}

impl OneDVoltageMode {
    fn as_str(&self) -> &'static str {
        match self {
            OneDVoltageMode::Rf => "rf",
            OneDVoltageMode::Dc => "dc",
            OneDVoltageMode::Square => "square",
            OneDVoltageMode::SinSin => "sin-sin",
            OneDVoltageMode::SquareSin => "square-sin",
            OneDVoltageMode::Custom => "custom",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct OneDBoundaryVoltage {
    mode: OneDVoltageMode,
    rf_amplitude_v: f64,
    powered_dc_v: f64,
    grounded_dc_v: f64,
    square_duty: f64,     // デューティ比 [0, 1]、デフォルト 0.5
    square_rise: f64,     // 立ち上がり・立ち下がり時間（周期の割合）、デフォルト 0.0
    rf2_amplitude_v: f64, // 第2周波数振幅 [V]、0 = 無効
    rf2_freq_ratio: f64,  // 第2周波数 f2/f1、0 = 無効
}

impl Default for OneDBoundaryVoltage {
    fn default() -> Self {
        Self {
            mode: OneDVoltageMode::Rf,
            rf_amplitude_v: VOLTAGE,
            powered_dc_v: 0.0,
            grounded_dc_v: 0.0,
            square_duty: 0.5,
            square_rise: 0.0,
            rf2_amplitude_v: 0.0,
            rf2_freq_ratio: 0.0,
        }
    }
}

impl OneDBoundaryVoltage {
    fn potentials_at_step(&self, step: usize, steps_per_period: usize) -> (f64, f64) {
        let phase_norm = (step as f64) / (steps_per_period as f64);
        let phase = phase_norm * TWO_PI;
        let cos_val = phase.cos();
        let sq_val = trapezoidal_square(phase_norm, self.square_duty, self.square_rise);

        let base_v = match self.mode {
            OneDVoltageMode::Rf => self.rf_amplitude_v * cos_val,
            OneDVoltageMode::Square => self.rf_amplitude_v * sq_val,
            OneDVoltageMode::SinSin => self.rf_amplitude_v * cos_val * cos_val,
            OneDVoltageMode::SquareSin => self.rf_amplitude_v * sq_val * cos_val,
            OneDVoltageMode::Dc => 0.0,
            OneDVoltageMode::Custom => {
                // カスタム波形データから電圧を取得
                if let Some(wf) = custom_waveform_data() {
                    let period = 1.0 / one_d_sim_config().frequency_hz;
                    let t = phase_norm * period;
                    wf.voltage_at_time(t) - self.powered_dc_v // DCオフセットは後で加算するため引く
                } else {
                    0.0 // カスタムデータがない場合は0V
                }
            }
        };

        // 第2周波数成分: V2 * cos(2π * f2/f1 * phase_norm)
        let second_v = if self.rf2_amplitude_v != 0.0 && self.rf2_freq_ratio != 0.0 {
            self.rf2_amplitude_v * (phase_norm * self.rf2_freq_ratio * TWO_PI).cos()
        } else {
            0.0
        };
        let powered_voltage = self.powered_dc_v + base_v + second_v;
        (powered_voltage, self.grounded_dc_v)
    }

    fn summary(&self) -> String {
        let second = if self.rf2_amplitude_v != 0.0 && self.rf2_freq_ratio != 0.0 {
            format!(
                " + {}*cos(2π*{:.3}f*t)",
                self.rf2_amplitude_v, self.rf2_freq_ratio
            )
        } else {
            String::new()
        };
        match self.mode {
            OneDVoltageMode::Rf => format!(
                "rf, powered={} + {}*cos(phase){} V, grounded={} V",
                self.powered_dc_v, self.rf_amplitude_v, second, self.grounded_dc_v
            ),
            OneDVoltageMode::Square => format!(
                "square, powered={} + {}*sq(phase, duty={}, rise={}){} V, grounded={} V",
                self.powered_dc_v,
                self.rf_amplitude_v,
                self.square_duty,
                self.square_rise,
                second,
                self.grounded_dc_v
            ),
            OneDVoltageMode::SinSin => format!(
                "sin-sin, powered={} + {}*cos²(phase){} V, grounded={} V",
                self.powered_dc_v, self.rf_amplitude_v, second, self.grounded_dc_v
            ),
            OneDVoltageMode::SquareSin => format!(
                "square-sin, powered={} + {}*sq(phase)*cos(phase){} V, grounded={} V",
                self.powered_dc_v, self.rf_amplitude_v, second, self.grounded_dc_v
            ),
            OneDVoltageMode::Dc => format!(
                "dc, powered={}{} V, grounded={} V",
                self.powered_dc_v, second, self.grounded_dc_v
            ),
            OneDVoltageMode::Custom => {
                let pts = if let Some(wf) = custom_waveform_data() {
                    format!("{} points, period={:.3e} s", wf.times.len(), wf.period)
                } else {
                    "no data".to_string()
                };
                format!(
                    "custom waveform ({}){}, grounded={} V",
                    pts, second, self.grounded_dc_v
                )
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum OneDGasModel {
    ArgonPic,
    Go2010AirTownsend,
    Lxcat,
}

impl Default for OneDGasModel {
    fn default() -> Self {
        Self::ArgonPic
    }
}

impl OneDGasModel {
    fn as_str(&self) -> &'static str {
        match self {
            OneDGasModel::ArgonPic => "argon-pic",
            OneDGasModel::Go2010AirTownsend => "go2010-air",
            OneDGasModel::Lxcat => "lxcat",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct OneDSimConfig {
    n_grid: usize,
    steps_per_period: usize,
    frequency_hz: f64,
    dt_s: Option<f64>,
    total_time_s: Option<f64>,
    gap_m: f64,
    pressure_pa: f64,
    temperature_k: f64,
    gas_model: OneDGasModel,
    weight: f64,
    electrode_area_m2: f64,
    initial_particles: usize,
    max_particles: usize,
    ion_subcycling: usize,
    xt_bin_steps: usize,
    n_adf: usize,
    n_2adf: usize,
    // LXCat 繧ｬ繧ｹ逕ｨ繝代Λ繝｡繝ｼ繧ｿ (argon-pic / go2010 繝｢繝ｼ繝峨〒縺ｯ譛ｪ菴ｿ逕ｨ)
    gas_mass_kg: f64,
    positive_ion_mass_kg: f64,
    ion_hs_sigma_iso_m2: f64,
    ion_hs_sigma_back_m2: f64,
    // 雋繧､繧ｪ繝ｳ雉ｪ驥・(0 = gas_mass_kg() 繧剃ｽｿ逕ｨ)
    negative_ion_mass_kg: f64,
}

impl Default for OneDSimConfig {
    fn default() -> Self {
        Self {
            n_grid: N_G,
            steps_per_period: N_T as usize,
            frequency_hz: FREQUENCY,
            dt_s: None,
            total_time_s: None,
            gap_m: L,
            pressure_pa: PRESSURE,
            temperature_k: TEMPERATURE,
            gas_model: OneDGasModel::default(),
            weight: WEIGHT,
            electrode_area_m2: ELECTRODE_AREA,
            initial_particles: N_INIT,
            max_particles: 200_000,
            ion_subcycling: N_SUB as usize,
            xt_bin_steps: N_BIN as usize,
            n_adf: N_ADF,
            n_2adf: 180,
            gas_mass_kg: 0.0,
            positive_ion_mass_kg: 0.0,
            ion_hs_sigma_iso_m2: 1.0e-18,
            ion_hs_sigma_back_m2: 1.0e-18,
            negative_ion_mass_kg: 0.0,
        }
    }
}

impl OneDSimConfig {
    fn apply_time_overrides(&mut self) -> Result<(), String> {
        if let Some(dt_s) = self.dt_s {
            if dt_s <= 0.0 || !dt_s.is_finite() {
                return Err("--dt-s must be a positive finite number".to_string());
            }
        }
        if let Some(total_time_s) = self.total_time_s {
            if total_time_s <= 0.0 || !total_time_s.is_finite() {
                return Err("--total-time-s must be a positive finite number".to_string());
            }
            if let Some(dt_s) = self.dt_s {
                self.steps_per_period = ((total_time_s / dt_s).ceil() as usize).max(1);
            }
        }
        Ok(())
    }

    fn validate(&self) -> Result<(), String> {
        if self.n_grid < 2 {
            return Err("--grid-points must be at least 2".to_string());
        }
        if self.steps_per_period == 0 {
            return Err("--steps-per-period must be at least 1".to_string());
        }
        if self.frequency_hz <= 0.0 || !self.frequency_hz.is_finite() {
            return Err("--frequency-hz must be a positive finite number".to_string());
        }
        if self.gap_m <= 0.0 || !self.gap_m.is_finite() {
            return Err("--gap-m must be a positive finite number".to_string());
        }
        if self.pressure_pa < 0.0 || !self.pressure_pa.is_finite() {
            return Err("--pressure-pa must be a non-negative finite number".to_string());
        }
        if self.temperature_k <= 0.0 || !self.temperature_k.is_finite() {
            return Err("--temperature-k must be a positive finite number".to_string());
        }
        if self.weight <= 0.0 || !self.weight.is_finite() {
            return Err("--weight must be a positive finite number".to_string());
        }
        if self.electrode_area_m2 <= 0.0 || !self.electrode_area_m2.is_finite() {
            return Err("--electrode-area-m2 must be a positive finite number".to_string());
        }
        if self.initial_particles == 0 {
            return Err("--initial-particles must be at least 1".to_string());
        }
        if self.max_particles < self.initial_particles {
            return Err("--max-particles must be at least --initial-particles".to_string());
        }
        if self.ion_subcycling == 0 {
            return Err("--ion-subcycling must be at least 1".to_string());
        }
        if self.xt_bin_steps == 0 {
            return Err("--xt-bin-steps must be at least 1".to_string());
        }
        if self.n_adf == 0 {
            return Err("--n-adf must be at least 1".to_string());
        }
        if self.n_2adf == 0 {
            return Err("--n-2adf must be at least 1".to_string());
        }
        Ok(())
    }

    fn period(&self) -> f64 {
        if let Some(total_time_s) = self.total_time_s {
            return total_time_s;
        }
        if let Some(dt_s) = self.dt_s {
            return dt_s * (self.steps_per_period as f64);
        }
        1.0 / self.frequency_hz
    }

    fn dt_e(&self) -> f64 {
        if let Some(dt_s) = self.dt_s {
            return dt_s;
        }
        self.period() / (self.steps_per_period as f64)
    }

    fn dt_i(&self) -> f64 {
        (self.ion_subcycling as f64) * self.dt_e()
    }

    fn dx(&self) -> f64 {
        self.gap_m / ((self.n_grid - 1) as f64)
    }

    fn inv_dx(&self) -> f64 {
        1.0 / self.dx()
    }

    fn gas_density(&self) -> f64 {
        self.pressure_pa / (K_BOLTZMANN * self.temperature_k)
    }

    pub(crate) fn ion_mass_kg(&self) -> f64 {
        match self.gas_model {
            OneDGasModel::ArgonPic => AR_MASS,
            OneDGasModel::Go2010AirTownsend => AIR_MASS,
            OneDGasModel::Lxcat => lxcat_gas_data().ion_mass_kg,
        }
    }

    // 讓咏噪荳ｭ諤ｧ繧ｬ繧ｹ縺ｮ雉ｪ驥上る崕蟄仙ｼｾ諤ｧ謨｣荵ｱ縺ｮ雉ｪ驥乗ｯ斐ｄ繧､繧ｪ繝ｳ辭ｱ騾溷ｺｦ縺ｫ菴ｿ縺・・
    pub(crate) fn gas_mass_kg(&self) -> f64 {
        match self.gas_model {
            OneDGasModel::ArgonPic => AR_MASS,
            OneDGasModel::Go2010AirTownsend => AIR_MASS,
            OneDGasModel::Lxcat => lxcat_gas_data().gas_mass_kg,
        }
    }

    // 雋繧､繧ｪ繝ｳ雉ｪ驥上ゅヵ繧｣繝ｼ繝ｫ繝峨′ 0 縺ｮ縺ｨ縺阪・繧ｬ繧ｹ雉ｪ驥上ｒ菴ｿ縺・・
    pub(crate) fn negative_ion_mass(&self) -> f64 {
        if self.negative_ion_mass_kg > 0.0 {
            self.negative_ion_mass_kg
        } else {
            self.gas_mass_kg()
        }
    }

    fn n_xt(&self) -> usize {
        ((self.steps_per_period + self.xt_bin_steps - 1) / self.xt_bin_steps).max(1)
    }

    pub(crate) fn da_adf(&self) -> f64 {
        90.0 / self.n_adf as f64
    }

    pub(crate) fn da_2adf(&self) -> f64 {
        180.0 / self.n_2adf as f64
    }

    fn xt_index(&self, step: usize) -> usize {
        (step / self.xt_bin_steps).min(self.n_xt() - 1)
    }

    fn min_eepf_x(&self) -> f64 {
        0.45 * self.gap_m
    }

    fn max_eepf_x(&self) -> f64 {
        0.55 * self.gap_m
    }

    fn summary(&self) -> String {
        format!(
            "gas={}, grid={}, steps/period={}, dt_e={:1.3e} s, total_time={:1.3e} s, ion_subcycling={}, gap={:1.3e} m, pressure={} Pa, temperature={} K, weight={:1.3e}, area={:1.3e} m^2, init_particles={}, max_particles={}",
            self.gas_model.as_str(),
            self.n_grid,
            self.steps_per_period,
            self.dt_e(),
            self.period(),
            self.ion_subcycling,
            self.gap_m,
            self.pressure_pa,
            self.temperature_k,
            self.weight,
            self.electrode_area_m2,
            self.initial_particles,
            self.max_particles
        )
    }
}

static ONE_D_SIM_CONFIG: OnceLock<OneDSimConfig> = OnceLock::new();

fn set_one_d_sim_config(config: OneDSimConfig) {
    if ONE_D_SIM_CONFIG.set(config).is_err() {
        println!(">> Rust-PIC: ERROR = 1D simulation config was already initialized");
        std::process::exit(1);
    }
}

pub(crate) fn one_d_sim_config() -> &'static OneDSimConfig {
    ONE_D_SIM_CONFIG.get_or_init(OneDSimConfig::default)
}

// LXCat 繝輔ぃ繧､繝ｫ繝代せ縺ｯ String 縺ｮ縺溘ａ config(Copy) 縺ｨ縺ｯ蛻･縺ｫ菫晄戟縺吶ｋ縲・
static LXCAT_PATH: OnceLock<Option<String>> = OnceLock::new();

fn set_lxcat_path(path: Option<String>) {
    let _ = LXCAT_PATH.set(path);
}

fn lxcat_path() -> Option<&'static str> {
    LXCAT_PATH.get().and_then(|o| o.as_deref())
}

// カスタム波形データ (時刻[s], 電圧[V]のペア)
#[derive(Clone, Debug)]
pub(crate) struct CustomWaveformData {
    times: Vec<f64>,
    voltages: Vec<f64>,
    period: f64, // 1周期の時間 [s]
}

impl CustomWaveformData {
    /// 線形補間で時刻tでの電圧を取得（周期的に繰り返し）
    fn voltage_at_time(&self, t: f64) -> f64 {
        if self.times.is_empty() {
            return 0.0;
        }
        if self.times.len() == 1 {
            return self.voltages[0];
        }

        // 時刻を周期で正規化 [0, period)
        let t_norm = ((t % self.period) + self.period) % self.period;

        // 線形補間
        for i in 1..self.times.len() {
            if t_norm <= self.times[i] {
                let t0 = self.times[i - 1];
                let t1 = self.times[i];
                let v0 = self.voltages[i - 1];
                let v1 = self.voltages[i];
                let alpha = (t_norm - t0) / (t1 - t0);
                return v0 + alpha * (v1 - v0);
            }
        }

        // 範囲外なら最後の値
        *self.voltages.last().unwrap()
    }
}

static CUSTOM_WAVEFORM_DATA: OnceLock<CustomWaveformData> = OnceLock::new();

fn set_custom_waveform_data(data: CustomWaveformData) {
    let _ = CUSTOM_WAVEFORM_DATA.set(data);
}

fn custom_waveform_data() -> Option<&'static CustomWaveformData> {
    CUSTOM_WAVEFORM_DATA.get()
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct OneDRunOptions {
    cycle: usize,
    measurement: bool,
    sim: OneDSimConfig,
    voltage: OneDBoundaryVoltage,
    surface: surface::SurfaceConfig,
}

//------------------------------------------------------------------------------------------//
// main                                                                                     //
// command line arguments:                                                                  //
// [1]: number of cycles (0 for init)                                                       //
// [2]: "m" turns on data collection and saving                                             //
//------------------------------------------------------------------------------------------//

fn ensure_output_dir(dir: &str) {
    create_dir_all(dir).unwrap_or_else(|err| {
        println!(
            ">> Rust-PIC: ERROR = cannot create output directory '{}': {:?}",
            dir, err
        );
        std::process::exit(1);
    });
}

fn output_path(dir: &str, filename: &str) -> String {
    Path::new(dir).join(filename).to_string_lossy().into_owned()
}

fn sample_anim_particles(particles: &[ParticleType]) -> (Vec<f64>, Vec<f64>) {
    let stride = (particles.len() / 1000).max(1);
    let sample_len = ((particles.len() + stride - 1) / stride).min(1000);
    let mut x = Vec::with_capacity(sample_len);
    let mut vx = Vec::with_capacity(sample_len);
    for part in particles.iter().step_by(stride).take(1000) {
        x.push(part.x);
        vx.push(part.vx);
    }
    (x, vx)
}

fn sample_anim_frame(
    t_step: usize,
    steps_per_period: usize,
    electrons: &[ParticleType],
    ions: &[ParticleType],
) -> AnimFrame {
    let (ex, evx) = sample_anim_particles(electrons);
    let (ix, ivx) = sample_anim_particles(ions);
    AnimFrame {
        t: (t_step as f64) / (steps_per_period as f64),
        ex,
        evx,
        ix,
        ivx,
    }
}

fn print_usage() {
    println!(">> Rust-PIC: usage:");
    println!("           1D RF default:      rust-pic <cycles> [m]");
    println!(
        "           1D DC boundary:     rust-pic <cycles> [m] --voltage-mode dc --dc-voltage <V>"
    );
    println!("           1D explicit bounds: rust-pic <cycles> [m] --voltage-mode dc --powered-dc-v <V> --grounded-dc-v <V>");
    println!("           1D timestep:        rust-pic <cycles> [m] --steps-per-period <N>");
    println!(
        "           1D DC time:         rust-pic <cycles> [m] --dt-s <dt> --total-time-s <time>"
    );
    println!("           Go2010 air:         rust-pic <cycles> [m] --go2010-mode --gap-m <m> --pressure-pa <Pa>");
    println!("           1D conditions:      --grid-points --frequency-hz --gap-m --pressure-pa --temperature-k --weight --electrode-area-m2 --initial-particles --max-particles --ion-subcycling --xt-bin-steps");
}

fn parse_1d_run_options(args: &[String]) -> Result<OneDRunOptions, String> {
    if args.is_empty() {
        return Err("need starting_cycle argument".to_string());
    }

    let cycle = args[0].parse::<usize>().map_err(|_| {
        format!(
            "starting_cycle must be a non-negative integer, got '{}'",
            args[0]
        )
    })?;
    let mut measurement = false;
    let mut sim = OneDSimConfig::default();
    let mut voltage = OneDBoundaryVoltage::default();
    let mut surface = surface::SurfaceConfig::default();
    let mut index = 1;
    while index < args.len() {
        let arg = &args[index];
        if arg == "m" || arg == "--diagnostic" || arg == "--diagnostics" {
            measurement = true;
        } else if arg == "--go2010-mode" {
            sim.gas_model = OneDGasModel::Go2010AirTownsend;
            surface.go2010_ion_enhanced_fn_enabled = true;
        } else if arg == "--gas-model" {
            index += 1;
            if index >= args.len() {
                return Err("--gas-model needs 'argon-pic' or 'go2010-air'".to_string());
            }
            sim.gas_model = parse_1d_gas_model(&args[index])?;
        } else if let Some(value) = arg.strip_prefix("--gas-model=") {
            sim.gas_model = parse_1d_gas_model(value)?;
        } else if arg == "--dc" {
            voltage.mode = OneDVoltageMode::Dc;
        } else if arg == "--rf" {
            voltage.mode = OneDVoltageMode::Rf;
        } else if arg == "--voltage-mode" {
            index += 1;
            if index >= args.len() {
                return Err(
                    "--voltage-mode needs 'rf', 'dc', 'square', 'sin-sin' or 'square-sin'"
                        .to_string(),
                );
            }
            voltage.mode = parse_1d_voltage_mode(&args[index])?;
        } else if let Some(value) = arg.strip_prefix("--voltage-mode=") {
            voltage.mode = parse_1d_voltage_mode(value)?;
        } else if arg == "--rf-amplitude-v" || arg == "--rf-voltage-v" {
            voltage.rf_amplitude_v = parse_next_f64(args, &mut index, arg)?;
        } else if let Some(value) = arg
            .strip_prefix("--rf-amplitude-v=")
            .or_else(|| arg.strip_prefix("--rf-voltage-v="))
        {
            voltage.rf_amplitude_v = parse_finite_f64(value, "--rf-amplitude-v")?;
        } else if arg == "--square-duty" {
            voltage.square_duty = parse_next_f64(args, &mut index, arg)?;
        } else if let Some(value) = arg.strip_prefix("--square-duty=") {
            voltage.square_duty = parse_finite_f64(value, "--square-duty")?;
        } else if arg == "--square-rise" {
            voltage.square_rise = parse_next_f64(args, &mut index, arg)?;
        } else if let Some(value) = arg.strip_prefix("--square-rise=") {
            voltage.square_rise = parse_finite_f64(value, "--square-rise")?;
        } else if arg == "--rf2-amplitude-v" {
            voltage.rf2_amplitude_v = parse_next_f64(args, &mut index, arg)?;
        } else if let Some(value) = arg.strip_prefix("--rf2-amplitude-v=") {
            voltage.rf2_amplitude_v = parse_finite_f64(value, "--rf2-amplitude-v")?;
        } else if arg == "--rf2-frequency-ratio" {
            voltage.rf2_freq_ratio = parse_next_f64(args, &mut index, arg)?;
        } else if let Some(value) = arg.strip_prefix("--rf2-frequency-ratio=") {
            voltage.rf2_freq_ratio = parse_finite_f64(value, "--rf2-frequency-ratio")?;
        } else if arg == "--dc-voltage" || arg == "--dc-voltage-v" {
            voltage.mode = OneDVoltageMode::Dc;
            voltage.powered_dc_v = parse_next_f64(args, &mut index, arg)?;
        } else if let Some(value) = arg
            .strip_prefix("--dc-voltage=")
            .or_else(|| arg.strip_prefix("--dc-voltage-v="))
        {
            voltage.mode = OneDVoltageMode::Dc;
            voltage.powered_dc_v = parse_finite_f64(value, "--dc-voltage")?;
        } else if arg == "--powered-dc-v" || arg == "--left-dc-v" {
            voltage.powered_dc_v = parse_next_f64(args, &mut index, arg)?;
        } else if let Some(value) = arg
            .strip_prefix("--powered-dc-v=")
            .or_else(|| arg.strip_prefix("--left-dc-v="))
        {
            voltage.powered_dc_v = parse_finite_f64(value, "--powered-dc-v")?;
        } else if arg == "--grounded-dc-v" || arg == "--right-dc-v" {
            voltage.grounded_dc_v = parse_next_f64(args, &mut index, arg)?;
        } else if let Some(value) = arg
            .strip_prefix("--grounded-dc-v=")
            .or_else(|| arg.strip_prefix("--right-dc-v="))
        {
            voltage.grounded_dc_v = parse_finite_f64(value, "--grounded-dc-v")?;
        } else if arg == "--grid-points" || arg == "--n-grid" {
            sim.n_grid = parse_next_usize(args, &mut index, arg)?;
        } else if let Some(value) = arg
            .strip_prefix("--grid-points=")
            .or_else(|| arg.strip_prefix("--n-grid="))
        {
            sim.n_grid = parse_usize_value(value, "--grid-points")?;
        } else if arg == "--steps-per-period" || arg == "--time-steps-per-period" || arg == "--nt" {
            sim.steps_per_period = parse_next_usize(args, &mut index, arg)?;
        } else if let Some(value) = arg
            .strip_prefix("--steps-per-period=")
            .or_else(|| arg.strip_prefix("--time-steps-per-period="))
            .or_else(|| arg.strip_prefix("--nt="))
        {
            sim.steps_per_period = parse_usize_value(value, "--steps-per-period")?;
        } else if arg == "--frequency-hz" {
            sim.frequency_hz = parse_next_f64(args, &mut index, arg)?;
        } else if let Some(value) = arg.strip_prefix("--frequency-hz=") {
            sim.frequency_hz = parse_finite_f64(value, "--frequency-hz")?;
        } else if arg == "--dt-s" || arg == "--time-step-s" {
            sim.dt_s = Some(parse_next_f64(args, &mut index, arg)?);
        } else if let Some(value) = arg
            .strip_prefix("--dt-s=")
            .or_else(|| arg.strip_prefix("--time-step-s="))
        {
            sim.dt_s = Some(parse_finite_f64(value, "--dt-s")?);
        } else if arg == "--total-time-s" {
            sim.total_time_s = Some(parse_next_f64(args, &mut index, arg)?);
        } else if let Some(value) = arg.strip_prefix("--total-time-s=") {
            sim.total_time_s = Some(parse_finite_f64(value, "--total-time-s")?);
        } else if arg == "--gap-m" || arg == "--length-m" {
            sim.gap_m = parse_next_f64(args, &mut index, arg)?;
        } else if let Some(value) = arg
            .strip_prefix("--gap-m=")
            .or_else(|| arg.strip_prefix("--length-m="))
        {
            sim.gap_m = parse_finite_f64(value, "--gap-m")?;
        } else if arg == "--pressure-pa" {
            sim.pressure_pa = parse_next_f64(args, &mut index, arg)?;
        } else if let Some(value) = arg.strip_prefix("--pressure-pa=") {
            sim.pressure_pa = parse_finite_f64(value, "--pressure-pa")?;
        } else if arg == "--temperature-k" {
            sim.temperature_k = parse_next_f64(args, &mut index, arg)?;
        } else if let Some(value) = arg.strip_prefix("--temperature-k=") {
            sim.temperature_k = parse_finite_f64(value, "--temperature-k")?;
        } else if arg == "--weight" || arg == "--superparticle-weight" {
            sim.weight = parse_next_f64(args, &mut index, arg)?;
        } else if let Some(value) = arg
            .strip_prefix("--weight=")
            .or_else(|| arg.strip_prefix("--superparticle-weight="))
        {
            sim.weight = parse_finite_f64(value, "--weight")?;
        } else if arg == "--electrode-area-m2" || arg == "--electrode-area" {
            sim.electrode_area_m2 = parse_next_f64(args, &mut index, arg)?;
        } else if let Some(value) = arg
            .strip_prefix("--electrode-area-m2=")
            .or_else(|| arg.strip_prefix("--electrode-area="))
        {
            sim.electrode_area_m2 = parse_finite_f64(value, "--electrode-area-m2")?;
        } else if arg == "--initial-particles" || arg == "--n-init" {
            sim.initial_particles = parse_next_usize(args, &mut index, arg)?;
        } else if let Some(value) = arg
            .strip_prefix("--initial-particles=")
            .or_else(|| arg.strip_prefix("--n-init="))
        {
            sim.initial_particles = parse_usize_value(value, "--initial-particles")?;
        } else if arg == "--max-particles" {
            sim.max_particles = parse_next_usize(args, &mut index, arg)?;
        } else if let Some(value) = arg.strip_prefix("--max-particles=") {
            sim.max_particles = parse_usize_value(value, "--max-particles")?;
        } else if arg == "--ion-subcycling" || arg == "--n-sub" {
            sim.ion_subcycling = parse_next_usize(args, &mut index, arg)?;
        } else if let Some(value) = arg
            .strip_prefix("--ion-subcycling=")
            .or_else(|| arg.strip_prefix("--n-sub="))
        {
            sim.ion_subcycling = parse_usize_value(value, "--ion-subcycling")?;
        } else if arg == "--xt-bin-steps" || arg == "--n-bin" {
            sim.xt_bin_steps = parse_next_usize(args, &mut index, arg)?;
        } else if let Some(value) = arg
            .strip_prefix("--xt-bin-steps=")
            .or_else(|| arg.strip_prefix("--n-bin="))
        {
            sim.xt_bin_steps = parse_usize_value(value, "--xt-bin-steps")?;
        } else if arg == "--n-adf" {
            sim.n_adf = parse_next_usize(args, &mut index, arg)?;
        } else if let Some(value) = arg.strip_prefix("--n-adf=") {
            sim.n_adf = parse_usize_value(value, "--n-adf")?;
        } else if arg == "--n-2adf" {
            sim.n_2adf = parse_next_usize(args, &mut index, arg)?;
        } else if let Some(value) = arg.strip_prefix("--n-2adf=") {
            sim.n_2adf = parse_usize_value(value, "--n-2adf")?;
        } else if arg == "--electron-reflection-probability" {
            surface.electron_reflection_probability = parse_next_f64(args, &mut index, arg)?;
        } else if let Some(value) = arg.strip_prefix("--electron-reflection-probability=") {
            surface.electron_reflection_probability =
                parse_finite_f64(value, "--electron-reflection-probability")?;
        } else if arg == "--secondary-yield" {
            surface.secondary_electron_yield = parse_next_f64(args, &mut index, arg)?;
        } else if let Some(value) = arg.strip_prefix("--secondary-yield=") {
            surface.secondary_electron_yield = parse_finite_f64(value, "--secondary-yield")?;
        } else if arg == "--secondary-energy-ev" {
            surface.secondary_electron_energy_ev = parse_next_f64(args, &mut index, arg)?;
        } else if let Some(value) = arg.strip_prefix("--secondary-energy-ev=") {
            surface.secondary_electron_energy_ev =
                parse_finite_f64(value, "--secondary-energy-ev")?;
        } else if arg == "--fn-emission-enabled" {
            surface.fn_emission_enabled = parse_next_bool(args, &mut index, arg)?;
        } else if let Some(value) = arg.strip_prefix("--fn-emission-enabled=") {
            surface.fn_emission_enabled = parse_bool_value(value, "--fn-emission-enabled")?;
        } else if arg == "--fn-work-function-ev" {
            surface.fn_work_function_ev = parse_next_f64(args, &mut index, arg)?;
        } else if let Some(value) = arg.strip_prefix("--fn-work-function-ev=") {
            surface.fn_work_function_ev = parse_finite_f64(value, "--fn-work-function-ev")?;
        } else if arg == "--fn-field-enhancement" {
            surface.fn_field_enhancement = parse_next_f64(args, &mut index, arg)?;
        } else if let Some(value) = arg.strip_prefix("--fn-field-enhancement=") {
            surface.fn_field_enhancement = parse_finite_f64(value, "--fn-field-enhancement")?;
        } else if arg == "--fn-emission-area-factor" {
            surface.fn_emission_area_factor = parse_next_f64(args, &mut index, arg)?;
        } else if let Some(value) = arg.strip_prefix("--fn-emission-area-factor=") {
            surface.fn_emission_area_factor = parse_finite_f64(value, "--fn-emission-area-factor")?;
        } else if arg == "--go2010-ion-enhanced-fn" {
            surface.go2010_ion_enhanced_fn_enabled = parse_next_bool(args, &mut index, arg)?;
        } else if let Some(value) = arg.strip_prefix("--go2010-ion-enhanced-fn=") {
            surface.go2010_ion_enhanced_fn_enabled =
                parse_bool_value(value, "--go2010-ion-enhanced-fn")?;
        } else if arg == "--go2010-k" {
            surface.go2010_k = parse_next_f64(args, &mut index, arg)?;
        } else if let Some(value) = arg.strip_prefix("--go2010-k=") {
            surface.go2010_k = parse_finite_f64(value, "--go2010-k")?;
        } else if arg == "--lxcat-file" {
            index += 1;
            if index >= args.len() {
                return Err("--lxcat-file needs a file path".to_string());
            }
            set_lxcat_path(Some(args[index].clone()));
        } else if let Some(value) = arg.strip_prefix("--lxcat-file=") {
            set_lxcat_path(Some(value.to_string()));
        } else if arg == "--waveform-file" {
            index += 1;
            if index >= args.len() {
                return Err("--waveform-file needs a file path".to_string());
            }
            let waveform_data = parse_waveform_csv(&args[index])?;
            set_custom_waveform_data(waveform_data);
        } else if let Some(value) = arg.strip_prefix("--waveform-file=") {
            let waveform_data = parse_waveform_csv(value)?;
            set_custom_waveform_data(waveform_data);
        } else if arg == "--gas-mass-kg" {
            sim.gas_mass_kg = parse_next_f64(args, &mut index, arg)?;
        } else if let Some(value) = arg.strip_prefix("--gas-mass-kg=") {
            sim.gas_mass_kg = parse_finite_f64(value, "--gas-mass-kg")?;
        } else if arg == "--ion-mass-kg" {
            sim.positive_ion_mass_kg = parse_next_f64(args, &mut index, arg)?;
        } else if let Some(value) = arg.strip_prefix("--ion-mass-kg=") {
            sim.positive_ion_mass_kg = parse_finite_f64(value, "--ion-mass-kg")?;
        } else if arg == "--ion-hs-sigma-iso-m2" {
            sim.ion_hs_sigma_iso_m2 = parse_next_f64(args, &mut index, arg)?;
        } else if let Some(value) = arg.strip_prefix("--ion-hs-sigma-iso-m2=") {
            sim.ion_hs_sigma_iso_m2 = parse_finite_f64(value, "--ion-hs-sigma-iso-m2")?;
        } else if arg == "--ion-hs-sigma-back-m2" {
            sim.ion_hs_sigma_back_m2 = parse_next_f64(args, &mut index, arg)?;
        } else if let Some(value) = arg.strip_prefix("--ion-hs-sigma-back-m2=") {
            sim.ion_hs_sigma_back_m2 = parse_finite_f64(value, "--ion-hs-sigma-back-m2")?;
        } else if arg == "--negative-ion-mass-kg" {
            sim.negative_ion_mass_kg = parse_next_f64(args, &mut index, arg)?;
        } else if let Some(value) = arg.strip_prefix("--negative-ion-mass-kg=") {
            sim.negative_ion_mass_kg = parse_finite_f64(value, "--negative-ion-mass-kg")?;
        } else {
            return Err(format!("unexpected 1D argument '{}'", arg));
        }
        index += 1;
    }
    sim.apply_time_overrides()?;
    sim.validate()?;
    validate_surface_config(&surface)?;
    if sim.gas_model == OneDGasModel::Lxcat && lxcat_path().is_none() {
        return Err("--gas-model lxcat には --lxcat-file <path> を指定してください".to_string());
    }
    if voltage.mode == OneDVoltageMode::Custom && custom_waveform_data().is_none() {
        return Err(
            "--voltage-mode custom には --waveform-file <path> を指定してください".to_string(),
        );
    }

    Ok(OneDRunOptions {
        cycle,
        measurement,
        sim,
        voltage,
        surface,
    })
}

/// 台形矩形波: phase_norm ∈ [0,1)、duty=on比率、rise=立ち上がり/立ち下がり時間の割合
/// rise=0 のとき理想矩形波、rise>0 のとき台形（線形遷移）
/// 戻り値は [-1, +1] の範囲
fn trapezoidal_square(phase_norm: f64, duty: f64, rise: f64) -> f64 {
    let p = phase_norm.rem_euclid(1.0);
    if rise <= 0.0 {
        return if p < duty { 1.0 } else { -1.0 };
    }
    let half_rise = rise * 0.5;
    let fall_start = duty - half_rise;
    let fall_end = duty + half_rise;
    let rise_start = 1.0 - half_rise;
    if p < fall_start {
        1.0
    } else if p < fall_end {
        1.0 - 2.0 * (p - fall_start) / rise
    } else if p < rise_start {
        -1.0
    } else {
        -1.0 + 2.0 * (p - rise_start) / rise
    }
}

fn parse_1d_voltage_mode(value: &str) -> Result<OneDVoltageMode, String> {
    match value.to_ascii_lowercase().as_str() {
        "rf" | "sine" | "sin" => Ok(OneDVoltageMode::Rf),
        "dc" => Ok(OneDVoltageMode::Dc),
        "square" => Ok(OneDVoltageMode::Square),
        "sin-sin" | "sine-sine" | "sinsin" => Ok(OneDVoltageMode::SinSin),
        "square-sin" | "square-sine" | "squaresin" => Ok(OneDVoltageMode::SquareSin),
        "custom" | "waveform" => Ok(OneDVoltageMode::Custom),
        _ => Err(format!(
            "unsupported voltage mode '{}'; expected 'rf', 'dc', 'square', 'sin-sin', 'square-sin' or 'custom'",
            value
        )),
    }
}

fn parse_1d_gas_model(value: &str) -> Result<OneDGasModel, String> {
    match value.to_ascii_lowercase().as_str() {
        "argon" | "ar" | "argon-pic" | "ar-pic" => Ok(OneDGasModel::ArgonPic),
        "go2010" | "go2010-air" | "air-townsend" | "air" => Ok(OneDGasModel::Go2010AirTownsend),
        "lxcat" | "lxcat-file" | "file" => Ok(OneDGasModel::Lxcat),
        _ => Err(format!(
            "unsupported gas model '{}'; expected 'argon-pic', 'go2010-air' or 'lxcat'",
            value
        )),
    }
}

/// CSVファイルから任意波形データを読み込む
/// フォーマット: 時刻[s], 電圧[V] の2列（ヘッダ行は無視）
/// '#'で始まる行はコメントとして無視
fn parse_waveform_csv(path: &str) -> Result<CustomWaveformData, String> {
    use std::io::{BufRead, BufReader};

    let file =
        File::open(path).map_err(|e| format!("failed to open waveform file '{}': {}", path, e))?;
    let reader = BufReader::new(file);

    let mut times = Vec::new();
    let mut voltages = Vec::new();

    for (line_num, line_result) in reader.lines().enumerate() {
        let line = line_result
            .map_err(|e| format!("failed to read line {} in '{}': {}", line_num + 1, path, e))?;
        let trimmed = line.trim();

        // 空行やコメント行はスキップ
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // カンマまたはタブ/スペースで分割
        let parts: Vec<&str> = if trimmed.contains(',') {
            trimmed.split(',').collect()
        } else {
            trimmed.split_whitespace().collect()
        };

        if parts.len() < 2 {
            return Err(format!(
                "line {} in '{}': expected 2 columns (time, voltage), got {}",
                line_num + 1,
                path,
                parts.len()
            ));
        }

        let t = parts[0].trim().parse::<f64>().map_err(|_| {
            format!(
                "line {} in '{}': invalid time '{}'",
                line_num + 1,
                path,
                parts[0]
            )
        })?;
        let v = parts[1].trim().parse::<f64>().map_err(|_| {
            format!(
                "line {} in '{}': invalid voltage '{}'",
                line_num + 1,
                path,
                parts[1]
            )
        })?;

        if !t.is_finite() || !v.is_finite() {
            return Err(format!(
                "line {} in '{}': time or voltage is not finite",
                line_num + 1,
                path
            ));
        }

        times.push(t);
        voltages.push(v);
    }

    if times.is_empty() {
        return Err(format!("waveform file '{}' contains no data", path));
    }

    // 時刻が単調増加かチェック
    for i in 1..times.len() {
        if times[i] <= times[i - 1] {
            return Err(format!(
                "waveform file '{}': time must be monotonically increasing (row {} vs {})",
                path,
                i,
                i + 1
            ));
        }
    }

    // 周期は最後の時刻（開始を0と仮定）
    let period = *times.last().unwrap();
    if period <= 0.0 {
        return Err(format!("waveform file '{}': period must be positive", path));
    }

    Ok(CustomWaveformData {
        times,
        voltages,
        period,
    })
}

fn parse_next_f64(args: &[String], index: &mut usize, option_name: &str) -> Result<f64, String> {
    *index += 1;
    if *index >= args.len() {
        return Err(format!("{} needs a numeric value", option_name));
    }
    parse_finite_f64(&args[*index], option_name)
}

fn parse_next_usize(
    args: &[String],
    index: &mut usize,
    option_name: &str,
) -> Result<usize, String> {
    *index += 1;
    if *index >= args.len() {
        return Err(format!("{} needs an integer value", option_name));
    }
    parse_usize_value(&args[*index], option_name)
}

fn parse_usize_value(value: &str, option_name: &str) -> Result<usize, String> {
    value.parse::<usize>().map_err(|_| {
        format!(
            "{} must be a non-negative integer, got '{}'",
            option_name, value
        )
    })
}

fn parse_finite_f64(value: &str, option_name: &str) -> Result<f64, String> {
    let parsed = value
        .parse::<f64>()
        .map_err(|_| format!("{} must be a finite number, got '{}'", option_name, value))?;
    if parsed.is_finite() {
        Ok(parsed)
    } else {
        Err(format!("{} must be finite, got '{}'", option_name, value))
    }
}

fn parse_next_bool(args: &[String], index: &mut usize, option_name: &str) -> Result<bool, String> {
    *index += 1;
    if *index >= args.len() {
        return Err(format!("{} needs true or false", option_name));
    }
    parse_bool_value(&args[*index], option_name)
}

fn parse_bool_value(value: &str, option_name: &str) -> Result<bool, String> {
    match value.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        _ => Err(format!(
            "{} must be true or false, got '{}'",
            option_name, value
        )),
    }
}

fn validate_surface_config(config: &surface::SurfaceConfig) -> Result<(), String> {
    if config.secondary_electron_yield < 0.0 || !config.secondary_electron_yield.is_finite() {
        return Err("--secondary-yield must be a non-negative finite number".to_string());
    }
    if config.secondary_electron_energy_ev < 0.0 || !config.secondary_electron_energy_ev.is_finite()
    {
        return Err("--secondary-energy-ev must be a non-negative finite number".to_string());
    }
    if !(0.0..=1.0).contains(&config.electron_reflection_probability)
        || !config.electron_reflection_probability.is_finite()
    {
        return Err(
            "--electron-reflection-probability must be a finite number from 0 to 1".to_string(),
        );
    }
    if config.fn_work_function_ev <= 0.0 || !config.fn_work_function_ev.is_finite() {
        return Err("--fn-work-function-ev must be a positive finite number".to_string());
    }
    if config.fn_field_enhancement < 0.0 || !config.fn_field_enhancement.is_finite() {
        return Err("--fn-field-enhancement must be a non-negative finite number".to_string());
    }
    if config.fn_emission_area_factor < 0.0 || !config.fn_emission_area_factor.is_finite() {
        return Err("--fn-emission-area-factor must be a non-negative finite number".to_string());
    }
    if config.go2010_k < 0.0 || !config.go2010_k.is_finite() {
        return Err("--go2010-k must be a non-negative finite number".to_string());
    }
    Ok(())
}

fn main() {
    println!(">> Rust-PIC: starting...");
    println!(
        ">> Rust-PIC: **************************************************************************"
    );
    println!(">> Rust-PIC: Made by Claude");
    println!(">> Rust-PIC: This program comes with ABSOLUTELY NO WARRANTY");
    println!(
        ">> Rust-PIC: This is free software, you are welcome to use, modify and redistribute it"
    );
    println!(">> Rust-PIC: MIT License, https://opensource.org/licenses/MIT");
    println!(
        ">> Rust-PIC: **************************************************************************"
    );
    // reading in command line arguments
    let args: Vec<String> = env::args().collect();
    if args.len() == 1 {
        println!(">> Rust-PIC: ERROR = need starting_cycle argument");
        std::process::exit(1);
    }
    if args[1] == "--help" || args[1] == "-h" {
        print_usage();
        return;
    }

    let one_d_options = parse_1d_run_options(&args[1..]).unwrap_or_else(|err| {
        println!(">> Rust-PIC: ERROR = {}", err);
        print_usage();
        std::process::exit(1);
    });
    let cycle: usize = one_d_options.cycle;
    let measurement: bool = one_d_options.measurement;
    let sim_config = one_d_options.sim;
    let voltage_config = one_d_options.voltage;
    set_one_d_sim_config(sim_config);
    if sim_config.gas_model == OneDGasModel::Lxcat {
        let path = lxcat_path().expect("lxcat path validated during parsing");
        match build_lxcat_gas_data(
            path,
            sim_config.gas_mass_kg,
            sim_config.positive_ion_mass_kg,
        ) {
            Ok(data) => {
                println!(
                    ">> Rust-PIC: LXCat loaded {} processes (gas_mass={:1.3e} kg, ion_mass={:1.3e} kg)",
                    data.processes.len(),
                    data.gas_mass_kg,
                    data.ion_mass_kg
                );
                set_lxcat_gas_data(data);
            }
            Err(e) => {
                println!(">> Rust-PIC: ERROR = {}", e);
                std::process::exit(1);
            }
        }
    }
    let mut cycles_done: usize = 0;
    ensure_output_dir(RESULT_1D_DIR);

    if measurement {
        println!(">> Rust-PIC: measurement mode: on");
    } else {
        println!(">> Rust-PIC: measurement mode: off");
    }
    println!(
        ">> Rust-PIC: 1D boundary voltage: {}",
        voltage_config.summary()
    );
    println!(
        ">> Rust-PIC: 1D calculation config: {}",
        sim_config.summary()
    );

    // initializing grid quantities (fixed size)
    let n_grid = sim_config.n_grid;
    let n_xt = sim_config.n_xt();
    let mut efield: Vec<f64> = vec![0.0; n_grid]; // electric field
    let mut pot: Vec<f64> = vec![0.0; n_grid]; // electric potential
    let mut e_density: Vec<f64> = vec![0.0; n_grid]; // electron density
    let mut i_density: Vec<f64> = vec![0.0; n_grid]; // ion density
    let mut rho: Vec<f64> = vec![0.0; n_grid]; // charge density
    let mut cumul_e_density: Vec<f64> = vec![0.0; n_grid]; // cumulative electron density
    let mut cumul_i_density: Vec<f64> = vec![0.0; n_grid]; // cumulative ion density
    let mut n_density: Vec<f64> = vec![0.0; n_grid]; // negative ion density
    let mut cumul_n_density: Vec<f64> = vec![0.0; n_grid]; // cumulative negative ion density

    // Particle data
    let mut Electrons: Vec<ParticleType> = Vec::new(); // new empty vector for electrons
    let mut Ions: Vec<ParticleType> = Vec::new(); // new empty vector for ions
    let mut NegativeIons: Vec<ParticleType> = Vec::new(); // new empty vector for negative ions

    // cross section stuff
    let cross_sections = init_cross_sections();
    check_cross_sections(&output_path(RESULT_1D_DIR, "cs.dat"), &cross_sections)
        .map_err(|err| println!(">> Rust-PIC: {:?}", err))
        .ok();

    // measurement data buffers
    let mut N_e: usize = 0;
    let mut N_i: usize = 0;
    let mut N_e_abs_pow: u64 = 0;
    let mut N_e_abs_gnd: u64 = 0;
    let mut N_i_abs_pow: u64 = 0;
    let mut N_i_abs_gnd: u64 = 0;
    let mut N_n_abs_pow: u64 = 0;
    let mut N_n_abs_gnd: u64 = 0;
    let surface_config = one_d_options.surface;
    let mut surface_state = surface::SurfaceState::default();
    let mut surface_stats = surface::SurfaceStats::default();

    let mut eepf: Vec<f64> = vec![0.0; N_EEPF];
    let mut efed_pow: Vec<u64> = vec![0; N_FED];
    let mut efed_gnd: Vec<u64> = vec![0; N_FED];
    let mut ifed_pow: Vec<u64> = vec![0; N_FED];
    let mut ifed_gnd: Vec<u64> = vec![0; N_FED];
    let mut iadf_pow: Vec<u64> = vec![0; one_d_sim_config().n_adf];
    let mut iadf_gnd: Vec<u64> = vec![0; one_d_sim_config().n_adf];
    let n2 = one_d_sim_config().n_2adf;
    let mut i2adf_pow: Vec<Vec<u64>> = vec![vec![0u64; n2]; n2];
    let mut i2adf_gnd: Vec<Vec<u64>> = vec![vec![0u64; n2]; n2];

    let mut pot_xt: Vec<Vec<f64>> = vec![vec![0.0; n_grid]; n_xt]; // XT distribution of the potential
    let mut efield_xt: Vec<Vec<f64>> = vec![vec![0.0; n_grid]; n_xt]; // XT distribution of the electric field
    let mut ne_xt: Vec<Vec<f64>> = vec![vec![0.0; n_grid]; n_xt]; // XT distribution of the electron density
    let mut ni_xt: Vec<Vec<f64>> = vec![vec![0.0; n_grid]; n_xt]; // XT distribution of the ion density
    let mut nn_xt: Vec<Vec<f64>> = vec![vec![0.0; n_grid]; n_xt]; // XT distribution of the negative ion density
    let mut ue_xt: Vec<Vec<f64>> = vec![vec![0.0; n_grid]; n_xt]; // XT distribution of the mean electron velocity
    let mut ui_xt: Vec<Vec<f64>> = vec![vec![0.0; n_grid]; n_xt]; // XT distribution of the mean ion velocity
    let mut je_xt: Vec<Vec<f64>> = vec![vec![0.0; n_grid]; n_xt]; // XT distribution of the electron current density
    let mut ji_xt: Vec<Vec<f64>> = vec![vec![0.0; n_grid]; n_xt]; // XT distribution of the ion current density
    let mut powere_xt: Vec<Vec<f64>> = vec![vec![0.0; n_grid]; n_xt]; // XT distribution of the electron powering (power absorption) rate
    let mut poweri_xt: Vec<Vec<f64>> = vec![vec![0.0; n_grid]; n_xt]; // XT distribution of the ion powering (power absorption) rate
    let mut meanee_xt: Vec<Vec<f64>> = vec![vec![0.0; n_grid]; n_xt]; // XT distribution of the mean electron energy
    let mut meanei_xt: Vec<Vec<f64>> = vec![vec![0.0; n_grid]; n_xt]; // XT distribution of the mean ion energy
    let mut counter_e_xt: Vec<Vec<f64>> = vec![vec![0.0; n_grid]; n_xt]; // XT counter for electron properties
    let mut counter_i_xt: Vec<Vec<f64>> = vec![vec![0.0; n_grid]; n_xt]; // XT counter for ion properties
    let mut ioniz_rate_xt: Vec<Vec<f64>> = vec![vec![0.0; n_grid]; n_xt]; // XT distribution of the ionisation rate
    let anim_frame_stride = (sim_config.steps_per_period / 200).max(1);
    let mut anim_frames = if measurement {
        let frame_capacity =
            (sim_config.steps_per_period + anim_frame_stride - 1) / anim_frame_stride;
        Some(Vec::with_capacity(frame_capacity))
    } else {
        None
    };

    let mut mean_energy_accu_center: f64 = 0.0;
    let mut mean_e_energy_pow: f64 = 0.0;
    let mut mean_e_energy_gnd: f64 = 0.0;
    let mut mean_i_energy_pow: f64 = 0.0;
    let mut mean_i_energy_gnd: f64 = 0.0;
    let mut N_center_mean_energy: u64 = 0;
    let mut N_e_coll: u64 = 0;
    let mut N_i_coll: u64 = 0;

    let mut conditions_OK: bool = true;

    let conv_path = output_path(RESULT_1D_DIR, "conv.dat");
    let needs_header = !std::path::Path::new(&conv_path).exists();
    let mut conv_file = OpenOptions::new()
        .append(true)
        .create(true)
        .open(&conv_path)
        .unwrap();
    if needs_header {
        writeln!(conv_file, "# cycle      #e           #i           #n")
            .map_err(|err| println!("{:?}", err))
            .ok();
    }

    // start clock for performance measure
    let start = Instant::now();
    let mut rng = rand::thread_rng(); // ThreadRNG - autoseeded from memory entropy, HC-128 algorithm

    if cycle == 0 {
        if std::path::Path::new("picdata.bin").exists() {
            println!(">> Rust-PIC: Warning: Data from previous calculation are detected.");
            println!("           To start a new simulation from the beginning, please delete all output files before running ./Rust-PIC 0");
            println!("           To continue the existing calculation, please specify the number of cycles to run, e.g. ./Rust-PIC 100");
            std::process::exit(0);
        }
        // initializing particles
        init_particles(
            sim_config.initial_particles,
            sim_config.gap_m,
            &mut Electrons,
            &mut rng,
        );
        init_particles(
            sim_config.initial_particles,
            sim_config.gap_m,
            &mut Ions,
            &mut rng,
        );

        println!(">> Rust-PIC: Running initializing cycle...");
        for t in 0..sim_config.steps_per_period {
            if t % 1000 == 0 {
                println!(
                    "c = {:8}  t = {:8}  #e = {:8}  #i = {:8}",
                    1,
                    t,
                    Electrons.len(),
                    Ions.len()
                );
            }

            get_density(&mut e_density, &mut cumul_e_density, &mut Electrons);
            if t % sim_config.ion_subcycling == 0 {
                get_density(&mut i_density, &mut cumul_i_density, &mut Ions);
                get_density(&mut n_density, &mut cumul_n_density, &mut NegativeIons);
            }

            rho = e_density
                .iter()
                .zip(i_density.iter())
                .zip(n_density.iter())
                .map(|((ne, ni), nn)| E_CHARGE * (ni - ne - nn))
                .collect();
            let (powered_voltage, grounded_voltage) =
                voltage_config.potentials_at_step(t, sim_config.steps_per_period);
            solve_poisson(
                &mut pot,
                &mut efield,
                &rho,
                powered_voltage,
                grounded_voltage,
            );

            surface::apply_fn_emission(
                &efield,
                &mut Electrons,
                &surface_config,
                &mut surface_state,
                &mut surface_stats,
                &mut rng,
            );
            move_particles(
                &efield,
                &mut Electrons,
                E_MASS,
                sim_config.dt_e(),
                -E_CHARGE,
            );
            if t % sim_config.ion_subcycling == 0 {
                move_particles(
                    &efield,
                    &mut Ions,
                    sim_config.ion_mass_kg(),
                    sim_config.dt_i(),
                    E_CHARGE,
                );
                move_particles(
                    &efield,
                    &mut NegativeIons,
                    sim_config.negative_ion_mass(),
                    sim_config.dt_i(),
                    -E_CHARGE,
                );
            }

            surface::check_electron_boundaries(
                &mut Electrons,
                &mut N_e_abs_pow,
                &mut N_e_abs_gnd,
                measurement,
                &mut efed_pow,
                &mut efed_gnd,
                &surface_config,
                &mut surface_stats,
                &mut rng,
            );
            if t % sim_config.ion_subcycling == 0 {
                surface::check_ion_boundaries(
                    &mut Ions,
                    &mut Electrons,
                    &efield,
                    &mut N_i_abs_pow,
                    &mut N_i_abs_gnd,
                    measurement,
                    &mut ifed_pow,
                    &mut ifed_gnd,
                    &mut iadf_pow,
                    &mut iadf_gnd,
                    &mut i2adf_pow,
                    &mut i2adf_gnd,
                    &surface_config,
                    &mut surface_stats,
                    &mut rng,
                );
                surface::check_negative_ion_boundaries(
                    &mut NegativeIons,
                    &mut N_n_abs_pow,
                    &mut N_n_abs_gnd,
                );
            }

            match sim_config.gas_model {
                OneDGasModel::ArgonPic | OneDGasModel::Lxcat => {
                    check_collisions_e(
                        &mut Electrons,
                        &mut Ions,
                        &mut NegativeIons,
                        &cross_sections,
                        &mut N_e_coll,
                        &mut rng,
                    );
                    if t % sim_config.ion_subcycling == 0 {
                        check_collisions_i(&mut Ions, &cross_sections, &mut N_i_coll, &mut rng);
                    }
                }
                OneDGasModel::Go2010AirTownsend => {
                    apply_go2010_air_townsend_ionization(
                        &mut Electrons,
                        &mut Ions,
                        &efield,
                        &mut N_e_coll,
                        &mut rng,
                    );
                }
            }
        }

        cycles_done = 1;
        writeln!(
            conv_file,
            "{:10}   {:10}   {:10}   {:10}",
            cycles_done,
            Electrons.len(),
            Ions.len(),
            NegativeIons.len()
        )
        .map_err(|err| println!("{:?}", err))
        .ok();
        save_particle_data(
            String::from("picdata.bin"),
            cycles_done,
            &Electrons,
            &Ions,
            &NegativeIons,
        )
        .map_err(|err| println!("{:?}", err))
        .ok();
    } else {
        // load particles
        let loaded_particles = load_particle_data(String::from("picdata.bin"));
        cycles_done = loaded_particles.0;
        Electrons = loaded_particles.1;
        Ions = loaded_particles.2;
        NegativeIons = loaded_particles.3;

        println!(">> Rust-PIC: Running {} cycles...", cycle);
        for c in 1..=cycle {
            for t in 0..sim_config.steps_per_period {
                if t % 1000 == 0 {
                    println!(
                        "c = {:8}  t = {:8}  #e = {:8}  #i = {:8}",
                        cycles_done + c,
                        t,
                        Electrons.len(),
                        Ions.len()
                    );
                }

                get_density(&mut e_density, &mut cumul_e_density, &mut Electrons);
                if t % sim_config.ion_subcycling == 0 {
                    get_density(&mut i_density, &mut cumul_i_density, &mut Ions);
                    get_density(&mut n_density, &mut cumul_n_density, &mut NegativeIons);
                }

                rho = e_density
                    .iter()
                    .zip(i_density.iter())
                    .zip(n_density.iter())
                    .map(|((ne, ni), nn)| E_CHARGE * (ni - ne - nn))
                    .collect();
                let (powered_voltage, grounded_voltage) =
                    voltage_config.potentials_at_step(t, sim_config.steps_per_period);
                solve_poisson(
                    &mut pot,
                    &mut efield,
                    &rho,
                    powered_voltage,
                    grounded_voltage,
                );

                surface::apply_fn_emission(
                    &efield,
                    &mut Electrons,
                    &surface_config,
                    &mut surface_state,
                    &mut surface_stats,
                    &mut rng,
                );
                move_particles(
                    &efield,
                    &mut Electrons,
                    E_MASS,
                    sim_config.dt_e(),
                    -E_CHARGE,
                );
                if t % sim_config.ion_subcycling == 0 {
                    move_particles(
                        &efield,
                        &mut Ions,
                        sim_config.ion_mass_kg(),
                        sim_config.dt_i(),
                        E_CHARGE,
                    );
                    move_particles(
                        &efield,
                        &mut NegativeIons,
                        sim_config.negative_ion_mass(),
                        sim_config.dt_i(),
                        -E_CHARGE,
                    );
                }

                surface::check_electron_boundaries(
                    &mut Electrons,
                    &mut N_e_abs_pow,
                    &mut N_e_abs_gnd,
                    measurement,
                    &mut efed_pow,
                    &mut efed_gnd,
                    &surface_config,
                    &mut surface_stats,
                    &mut rng,
                );
                if t % sim_config.ion_subcycling == 0 {
                    surface::check_ion_boundaries(
                        &mut Ions,
                        &mut Electrons,
                        &efield,
                        &mut N_i_abs_pow,
                        &mut N_i_abs_gnd,
                        measurement,
                        &mut ifed_pow,
                        &mut ifed_gnd,
                        &mut iadf_pow,
                        &mut iadf_gnd,
                        &mut i2adf_pow,
                        &mut i2adf_gnd,
                        &surface_config,
                        &mut surface_stats,
                        &mut rng,
                    );
                    surface::check_negative_ion_boundaries(
                        &mut NegativeIons,
                        &mut N_n_abs_pow,
                        &mut N_n_abs_gnd,
                    );
                }

                match sim_config.gas_model {
                    OneDGasModel::ArgonPic | OneDGasModel::Lxcat => {
                        check_collisions_e(
                            &mut Electrons,
                            &mut Ions,
                            &mut NegativeIons,
                            &cross_sections,
                            &mut N_e_coll,
                            &mut rng,
                        );
                        if t % sim_config.ion_subcycling == 0 {
                            check_collisions_i(&mut Ions, &cross_sections, &mut N_i_coll, &mut rng);
                        }
                    }
                    OneDGasModel::Go2010AirTownsend => {
                        apply_go2010_air_townsend_ionization(
                            &mut Electrons,
                            &mut Ions,
                            &efield,
                            &mut N_e_coll,
                            &mut rng,
                        );
                    }
                }

                if measurement {
                    if c == cycle && t % anim_frame_stride == 0 {
                        if let Some(frames) = anim_frames.as_mut() {
                            frames.push(sample_anim_frame(
                                t,
                                sim_config.steps_per_period,
                                &Electrons,
                                &Ions,
                            ));
                        }
                    }

                    let t_index: usize = sim_config.xt_index(t);
                    let mut p: usize;
                    let mut c1: f64;
                    let mut c2: f64;
                    let mut e_x: f64;
                    let mut mean_v: f64;
                    let mut v_sqr: f64;
                    let mut energy: f64;
                    let mut rate: f64;
                    let mut energy_index: usize;

                    // collect data from electrons: mean energy, mean velocity, ionization rate, EEPF
                    for part in Electrons.iter() {
                        p = core::cmp::min(
                            (part.x * sim_config.inv_dx()).trunc() as usize,
                            sim_config.n_grid - 2,
                        );
                        c2 = part.x * sim_config.inv_dx() - (p as f64);
                        c1 = 1.0 - c2;
                        e_x = c1 * efield[p] + c2 * efield[p + 1];
                        mean_v = part.vx - 0.5 * e_x * sim_config.dt_e() * E_CHARGE / E_MASS;
                        counter_e_xt[t_index][p] += c1;
                        counter_e_xt[t_index][p + 1] += c2;
                        ue_xt[t_index][p] += c1 * mean_v;
                        ue_xt[t_index][p + 1] += c2 * mean_v;
                        v_sqr = mean_v * mean_v + part.vy * part.vy + part.vz * part.vz;
                        energy = 0.5 * E_MASS * v_sqr / EV_TO_J;
                        meanee_xt[t_index][p] += c1 * energy;
                        meanee_xt[t_index][p + 1] += c2 * energy;
                        rate = match sim_config.gas_model {
                            OneDGasModel::ArgonPic | OneDGasModel::Lxcat => {
                                energy_index = core::cmp::min(
                                    (energy / (DE_CS as f64) + 0.5).trunc() as usize,
                                    (CS_RANGES - 1) as usize,
                                );
                                cross_sections.ionization_sigma(energy_index)
                                    * v_sqr.sqrt()
                                    * sim_config.dt_e()
                                    * sim_config.gas_density()
                            }
                            OneDGasModel::Go2010AirTownsend => {
                                go2010_air_townsend_alpha(e_x.abs(), sim_config.pressure_pa)
                                    * v_sqr.sqrt()
                                    * sim_config.dt_e()
                            }
                        };
                        ioniz_rate_xt[t_index][p] += c1 * rate;
                        ioniz_rate_xt[t_index][p + 1] += c2 * rate;

                        if (sim_config.min_eepf_x() < part.x) && (part.x < sim_config.max_eepf_x())
                        {
                            let e_index: usize = (energy / DE_EEPF).trunc() as usize;
                            if e_index < N_EEPF {
                                eepf[e_index] += 1.0;
                            }
                            mean_energy_accu_center += energy;
                            N_center_mean_energy += 1;
                        }
                    }

                    // collect data from ions: mean energy, mean velocity
                    if t % sim_config.ion_subcycling == 0 {
                        for part in Ions.iter() {
                            p = core::cmp::min(
                                (part.x * sim_config.inv_dx()).trunc() as usize,
                                sim_config.n_grid - 2,
                            );
                            c2 = part.x * sim_config.inv_dx() - (p as f64);
                            c1 = 1.0 - c2;
                            e_x = c1 * efield[p] + c2 * efield[p + 1];
                            mean_v = part.vx
                                + 0.5 * e_x * sim_config.dt_i() * E_CHARGE
                                    / sim_config.ion_mass_kg();
                            counter_i_xt[t_index][p] += c1;
                            counter_i_xt[t_index][p + 1] += c2;
                            ui_xt[t_index][p] += c1 * mean_v;
                            ui_xt[t_index][p + 1] += c2 * mean_v;
                            v_sqr = mean_v * mean_v + part.vy * part.vy + part.vz * part.vz;
                            energy = 0.5 * sim_config.ion_mass_kg() * v_sqr / EV_TO_J;
                            meanei_xt[t_index][p] += c1 * energy;
                            meanei_xt[t_index][p + 1] += c2 * energy;
                        }
                    }
                    // collect data from the grid
                    for i in 0..sim_config.n_grid {
                        pot_xt[t_index][i] += pot[i];
                        efield_xt[t_index][i] += efield[i];
                        ne_xt[t_index][i] += e_density[i];
                        ni_xt[t_index][i] += i_density[i];
                        nn_xt[t_index][i] += n_density[i];
                    }
                }
            }
            writeln!(
                conv_file,
                "{:10}   {:10}   {:10}   {:10}",
                cycles_done + c,
                Electrons.len(),
                Ions.len(),
                NegativeIons.len()
            )
            .map_err(|err| println!("{:?}", err))
            .ok();
        }
        cycles_done += cycle;
        N_e = Electrons.len();
        N_i = Ions.len();
        save_particle_data(
            String::from("picdata.bin"),
            cycles_done,
            &Electrons,
            &Ions,
            &NegativeIons,
        )
        .map_err(|err| println!("{:?}", err))
        .ok();
    }

    if measurement {
        let norm: f64 = (n_xt as f64) / (cycle as f64) / (sim_config.steps_per_period as f64);
        calc_current_and_power(
            &mut je_xt,
            &mut powere_xt,
            &ue_xt,
            &ne_xt,
            &counter_e_xt,
            &efield_xt,
            -E_CHARGE,
            norm,
        )
        .map_err(|err| println!("{:?}", err))
        .ok();
        calc_current_and_power(
            &mut ji_xt,
            &mut poweri_xt,
            &ui_xt,
            &ni_xt,
            &counter_i_xt,
            &efield_xt,
            E_CHARGE,
            norm,
        )
        .map_err(|err| println!("{:?}", err))
        .ok();
        calc_fed(
            &efed_pow,
            &efed_gnd,
            &mut mean_e_energy_pow,
            &mut mean_e_energy_gnd,
        )
        .map_err(|err| println!("{:?}", err))
        .ok();
        calc_fed(
            &ifed_pow,
            &ifed_gnd,
            &mut mean_i_energy_pow,
            &mut mean_i_energy_gnd,
        )
        .map_err(|err| println!("{:?}", err))
        .ok();
        check_and_save_info(
            &output_path(RESULT_1D_DIR, "info.txt"),
            cumul_e_density[sim_config.n_grid / 2],
            cycle,
            mean_energy_accu_center,
            N_center_mean_energy,
            N_e,
            N_i,
            NegativeIons.len(),
            N_e_coll,
            N_i_coll,
            &cross_sections.sigma_tot_e,
            &cross_sections.sigma_tot_i,
            mean_e_energy_pow,
            mean_e_energy_gnd,
            mean_i_energy_pow,
            mean_i_energy_gnd,
            N_e_abs_pow,
            N_e_abs_gnd,
            N_i_abs_pow,
            N_i_abs_gnd,
            N_n_abs_pow,
            N_n_abs_gnd,
            cycle,
            &powere_xt,
            &poweri_xt,
            &voltage_config,
            &surface_config,
            &surface_stats,
            &mut conditions_OK,
        )
        .map_err(|err| println!("{:?}", err))
        .ok();
        if !conditions_OK {
            println!(
                ">> Rust-PIC: WARNING = stability/accuracy conditions were violated; saving measurement .dat files anyway."
            );
        }
        println!(
            ">> Rust-PIC: Saving measurements to disk: {}",
            RESULT_1D_DIR
        );
        println!(">> saving density.dat");
        save_densities(
            &output_path(RESULT_1D_DIR, "density.dat"),
            &cumul_e_density,
            &cumul_i_density,
            &cumul_n_density,
            cycle,
        )
        .map_err(|err| println!("{:?}", err))
        .ok();
        println!(">> saving eepf.dat");
        save_eepf(&output_path(RESULT_1D_DIR, "eepf.dat"), &eepf)
            .map_err(|err| println!("{:?}", err))
            .ok();
        println!(">> saving efed.dat");
        save_fed(
            &output_path(RESULT_1D_DIR, "efed.dat"),
            &efed_pow,
            &efed_gnd,
        )
        .map_err(|err| println!("{:?}", err))
        .ok();
        println!(">> saving ifed.dat");
        save_fed(
            &output_path(RESULT_1D_DIR, "ifed.dat"),
            &ifed_pow,
            &ifed_gnd,
        )
        .map_err(|err| println!("{:?}", err))
        .ok();
        println!(">> saving iadf.dat");
        save_iadf(
            &output_path(RESULT_1D_DIR, "iadf.dat"),
            &iadf_pow,
            &iadf_gnd,
        )
        .map_err(|err| println!("{:?}", err))
        .ok();
        println!(">> saving i2adf_pow.dat");
        save_i2adf(&output_path(RESULT_1D_DIR, "i2adf_pow.dat"), &i2adf_pow)
            .map_err(|err| println!("{:?}", err))
            .ok();
        println!(">> saving i2adf_gnd.dat");
        save_i2adf(&output_path(RESULT_1D_DIR, "i2adf_gnd.dat"), &i2adf_gnd)
            .map_err(|err| println!("{:?}", err))
            .ok();
        println!(">> saving pot_xt.dat");
        save_xt_1(&output_path(RESULT_1D_DIR, "pot_xt.dat"), &pot_xt, norm)
            .map_err(|err| println!("{:?}", err))
            .ok();
        println!(">> saving efield_xt.dat");
        save_xt_1(
            &output_path(RESULT_1D_DIR, "efield_xt.dat"),
            &efield_xt,
            norm,
        )
        .map_err(|err| println!("{:?}", err))
        .ok();
        println!(">> saving ne_xt.dat");
        save_xt_1(&output_path(RESULT_1D_DIR, "ne_xt.dat"), &ne_xt, norm)
            .map_err(|err| println!("{:?}", err))
            .ok();
        println!(">> saving ni_xt.dat");
        save_xt_1(&output_path(RESULT_1D_DIR, "ni_xt.dat"), &ni_xt, norm)
            .map_err(|err| println!("{:?}", err))
            .ok();
        println!(">> saving nn_xt.dat");
        save_xt_1(&output_path(RESULT_1D_DIR, "nn_xt.dat"), &nn_xt, norm)
            .map_err(|err| println!("{:?}", err))
            .ok();
        println!(">> saving je_xt.dat");
        save_xt_1(&output_path(RESULT_1D_DIR, "je_xt.dat"), &je_xt, 1.0)
            .map_err(|err| println!("{:?}", err))
            .ok();
        println!(">> saving ji_xt.dat");
        save_xt_1(&output_path(RESULT_1D_DIR, "ji_xt.dat"), &ji_xt, 1.0)
            .map_err(|err| println!("{:?}", err))
            .ok();
        println!(">> saving powere_xt.dat");
        save_xt_1(
            &output_path(RESULT_1D_DIR, "powere_xt.dat"),
            &powere_xt,
            1.0,
        )
        .map_err(|err| println!("{:?}", err))
        .ok();
        println!(">> saving poweri_xt.dat");
        save_xt_1(
            &output_path(RESULT_1D_DIR, "poweri_xt.dat"),
            &poweri_xt,
            1.0,
        )
        .map_err(|err| println!("{:?}", err))
        .ok();
        let c: f64 = (sim_config.weight / sim_config.electrode_area_m2 / sim_config.dx())
            / ((cycle as f64) * sim_config.period() / (n_xt as f64));
        println!(">> saving ioniz_xt.dat");
        save_xt_1(
            &output_path(RESULT_1D_DIR, "ioniz_xt.dat"),
            &ioniz_rate_xt,
            c,
        )
        .map_err(|err| println!("{:?}", err))
        .ok();
        println!(">> saving meanee_xt.dat");
        save_xt_2(
            &output_path(RESULT_1D_DIR, "meanee_xt.dat"),
            &meanee_xt,
            &counter_e_xt,
        )
        .map_err(|err| println!("{:?}", err))
        .ok();
        println!(">> saving meanei_xt.dat");
        save_xt_2(
            &output_path(RESULT_1D_DIR, "meanei_xt.dat"),
            &meanei_xt,
            &counter_i_xt,
        )
        .map_err(|err| println!("{:?}", err))
        .ok();
        if let Some(frames) = anim_frames.take() {
            println!(">> saving anim.json");
            let anim = AnimData {
                gap_m: sim_config.gap_m,
                frames_per_cycle: frames.len(),
                frames,
            };
            save_anim(&output_path(RESULT_1D_DIR, "anim.json"), &anim)
                .map_err(|err| println!(">> Rust-PIC: ERROR = failed to save anim.json: {:?}", err))
                .ok();
        }
    }
    println!(
        ">> Rust-PIC: Simulation of {} cycle(s) is completed lasting {:.3} sec.",
        cycle,
        0.001 * start.elapsed().as_millis() as f64
    );
}

//----------------------------------------------------------------------//
// move particles in E-field                                            //
//----------------------------------------------------------------------//

fn move_particles(
    efield: &Vec<f64>,
    Particle: &mut Vec<ParticleType>,
    mass: f64,
    dt: f64,
    charge: f64,
) {
    let config = one_d_sim_config();
    let inv_dx = config.inv_dx();
    let factor: f64 = dt / mass * charge;
    Particle.par_iter_mut().for_each(|part| {
        let p = core::cmp::min((part.x * inv_dx).trunc() as usize, config.n_grid - 2);
        let c2 = part.x * inv_dx - (p as f64);
        let e_x = (1.0 - c2) * efield[p] + c2 * efield[p + 1];
        part.vx += e_x * factor;
        part.x += part.vx * dt;
    });
}

//----------------------------------------------------------------------//
// Ar+ / Ar collision                                                   //
//----------------------------------------------------------------------//

fn collision_ion(
    data: &CrossSectionData,
    particle: &mut ParticleType,
    vxa: f64,
    vya: f64,
    vza: f64,
    eindex: usize,
    rng: &mut ThreadRng,
) {
    // 豁｣繧､繧ｪ繝ｳ縺ｯ隕ｪ繧ｬ繧ｹ縺ｨ縺ｻ縺ｼ遲芽ｳｪ驥上・縺溘ａ縲・㍾蠢・ｿよ焚 0.5 縺ｮ蠑ｾ諤ｧ謨｣荵ｱ繧堤畑縺・ｋ縲・
    let t0 = data.i_iso[eindex];
    let t1 = t0 + data.i_back[eindex];

    let phi: f64;
    let theta: f64;
    let chi: f64;
    let eta: f64;

    let mut gx = particle.vx - vxa; // relative velocity in cold gas approximation
    let mut gy = particle.vy - vya;
    let mut gz = particle.vz - vza;
    let g = (gx.powf(2.0) + gy.powf(2.0) + gz.powf(2.0)).sqrt();
    let wx = 0.5 * (particle.vx + vxa);
    let wy = 0.5 * (particle.vy + vya);
    let wz = 0.5 * (particle.vz + vza);

    // find Euler angles:
    if gx == 0.0 {
        theta = 0.5 * PI;
    } else {
        theta = ((gy * gy + gz * gz).sqrt()).atan2(gx);
    }
    if gy == 0.0 {
        if gz > 0.0 {
            phi = 0.5 * PI;
        } else {
            phi = -0.5 * PI;
        }
    } else {
        phi = gz.atan2(gy);
    }

    let rnd = rng.gen::<f64>();
    if rnd < t0 / t1 {
        chi = (1.0 - 2.0 * rng.gen::<f64>()).acos();
    } else {
        chi = PI;
    }
    eta = TWO_PI * rng.gen::<f64>();

    let sc = chi.sin();
    let cc = chi.cos();
    let se = eta.sin();
    let ce = eta.cos();
    let st = theta.sin();
    let ct = theta.cos();
    let sp = phi.sin();
    let cp = phi.cos();

    // compute new relative velocity:

    gx = g * (ct * cc - st * sc * ce);
    gy = g * (st * cp * cc + ct * cp * sc * ce - sp * sc * se);
    gz = g * (st * sp * cc + ct * sp * sc * ce + cp * sc * se);

    // post-collision velocity of the electron

    particle.vx = wx + 0.5 * gx;
    particle.vy = wy + 0.5 * gy;
    particle.vz = wz + 0.5 * gz;
}

fn check_collisions_i(
    Particle: &mut Vec<ParticleType>,
    data: &CrossSectionData,
    N_coll: &mut u64,
    _rng: &mut ThreadRng,
) {
    let config = one_d_sim_config();
    let gas_mass = config.gas_mass_kg();
    let ion_mass = config.ion_mass_kg();
    let mu = ion_mass * gas_mass / (ion_mass + gas_mass); // 謠帷ｮ苓ｳｪ驥・
    let collisions: u64 = Particle
        .par_iter_mut()
        .map_init(
            || {
                (
                    rand::thread_rng(),
                    Normal::new(0.0, (K_BOLTZMANN * config.temperature_k / gas_mass).sqrt())
                        .unwrap(),
                )
            },
            |(rng, normal_range), particle| {
                let vxa = rng.sample(&*normal_range);
                let vya = rng.sample(&*normal_range);
                let vza = rng.sample(&*normal_range);
                let gx = particle.vx - vxa;
                let gy = particle.vy - vxa;
                let gz = particle.vz - vxa;
                let g2 = gx.powf(2.0) + gy.powf(2.0) + gz.powf(2.0);
                let g: f64 = g2.sqrt();
                let energy: f64 = 0.5 * mu * g2 / EV_TO_J;
                let energy_index = core::cmp::min(
                    (energy / (DE_CS as f64) + 0.5).trunc() as usize,
                    (CS_RANGES - 1) as usize,
                );

                let nu: f64 = data.sigma_tot_i[energy_index] * g;
                let p_coll: f64 = 1.0 - (-nu * config.dt_i()).exp();
                if rng.gen::<f64>() < p_coll {
                    collision_ion(data, particle, vxa, vya, vza, energy_index, rng);
                    1
                } else {
                    0
                }
            },
        )
        .sum();

    *N_coll += collisions;
}

//----------------------------------------------------------------------//
// e / Ar collision                                                     //
//----------------------------------------------------------------------//

// 髮ｻ蟄占｡晉ｪ√・邨先棡繧定｡ｨ縺・enum
enum ElectronOutcome {
    None,
    Ionized {
        new_electron: ParticleType,
        new_ion: ParticleType,
    },
    Attached {
        negative_ion: ParticleType,
    },
}

fn collision_electron(
    electron: &mut ParticleType,
    data: &CrossSectionData,
    eindex: usize,
    rng: &mut ThreadRng,
) -> ElectronOutcome {
    let config = one_d_sim_config();
    let gas_mass = config.gas_mass_kg();
    let normal_range =
        Normal::new(0.0, (K_BOLTZMANN * config.temperature_k / gas_mass).sqrt()).unwrap();
    let f1 = E_MASS / (E_MASS + gas_mass);
    let f2 = gas_mass / (E_MASS + gas_mass);

    // 陦晉ｪ・℃遞九ｒ邏ｯ遨肴妙髱｢遨阪〒驕ｸ謚槭☆繧・(蠑ｾ諤ｧ繝ｻ蜉ｱ襍ｷN繝ｻ髮ｻ髮｢繝ｻ莉倡捩繧剃ｸ闊ｬ縺ｫ謇ｱ縺・
    let total: f64 = data.e_sigma.iter().map(|s| s[eindex]).sum();
    if total <= 0.0 {
        return ElectronOutcome::None;
    }
    let target = rng.gen::<f64>() * total;
    let mut acc = 0.0;
    let mut chosen = data.e_sigma.len() - 1;
    for (k, sigma) in data.e_sigma.iter().enumerate() {
        acc += sigma[eindex];
        if target < acc {
            chosen = k;
            break;
        }
    }
    let kind = data.e_kind[chosen];
    let loss = data.e_loss_j[chosen];

    let mut gx: f64 = electron.vx; // relative velocity in cold gas approximation
    let mut gy: f64 = electron.vy;
    let mut gz: f64 = electron.vz;
    let mut g: f64 = (gx.powf(2.0) + gy.powf(2.0) + gz.powf(2.0)).sqrt();
    let wx: f64 = f1 * gx;
    let wy: f64 = f1 * gy;
    let wz: f64 = f1 * gz;

    // find Euler angles:
    let phi: f64;
    let theta: f64;
    if gx == 0.0 {
        theta = 0.5 * PI;
    } else {
        theta = ((gy * gy + gz * gz).sqrt()).atan2(gx);
    }
    if gy == 0.0 {
        if gz > 0.0 {
            phi = 0.5 * PI;
        } else {
            phi = -0.5 * PI;
        }
    } else {
        phi = gz.atan2(gy);
    }

    let chi: f64;
    let eta: f64;
    let mut sc: f64;
    let mut cc: f64;
    let mut se: f64;
    let mut ce: f64;
    let st: f64 = theta.sin();
    let ct: f64 = theta.cos();
    let sp: f64 = phi.sin();
    let cp: f64 = phi.cos();

    match kind {
        lxcat::ProcessKind::Attachment => {
            // 髮ｻ蟄蝉ｻ倡捩: 雋繧､繧ｪ繝ｳ繧堤函謌舌＠縺ｦ髮ｻ蟄舌ｒ豸亥､ｱ縺輔○繧・(NaN 繝槭・繧ｯ 竊・蠕後〒髯､蜴ｻ)
            let neg_ion_mass = config.negative_ion_mass();
            let neg_normal = Normal::new(
                0.0,
                (K_BOLTZMANN * config.temperature_k / neg_ion_mass).sqrt(),
            )
            .unwrap();
            let neg_ion = ParticleType {
                x: electron.x,
                vx: rng.sample(&neg_normal),
                vy: rng.sample(&neg_normal),
                vz: rng.sample(&neg_normal),
            };
            electron.x = f64::NAN;
            return ElectronOutcome::Attached {
                negative_ion: neg_ion,
            };
        }
        lxcat::ProcessKind::Ionization => {
            let mut energy = 0.5 * E_MASS * g.powf(2.0);
            energy = (energy - loss).abs(); // subtract ionization energy loss
            let e_new =
                10.0 * (rng.gen::<f64>() * (energy / EV_TO_J / 20.0).atan()).tan() * EV_TO_J;
            let e_orig = (energy - e_new).abs(); // [Donko PRE 57, 7126 (1998); Opal JCP 55, 4100 (1971)]
            g = (2.0 * e_orig / E_MASS).sqrt();
            let g_new: f64 = (2.0 * e_new / E_MASS).sqrt();
            chi = (e_orig / energy).sqrt().acos();
            let chi_new: f64 = (e_new / energy).sqrt().acos();
            eta = TWO_PI * rng.gen::<f64>();
            let eta_new: f64 = eta + PI;
            sc = chi_new.sin();
            cc = chi_new.cos();
            se = eta_new.sin();
            ce = eta_new.cos();
            gx = g_new * (ct * cc - st * sc * ce);
            gy = g_new * (st * cp * cc + ct * cp * sc * ce - sp * sc * se);
            gz = g_new * (st * sp * cc + ct * sp * sc * ce + cp * sc * se);
            let new_electron = ParticleType {
                x: electron.x,
                vx: wx + f2 * gx,
                vy: wy + f2 * gy,
                vz: wz + f2 * gz,
            };
            let new_ion = ParticleType {
                x: electron.x,
                vx: rng.sample(&normal_range),
                vy: rng.sample(&normal_range),
                vz: rng.sample(&normal_range),
            };

            sc = chi.sin();
            cc = chi.cos();
            se = eta.sin();
            ce = eta.cos();
            gx = g * (ct * cc - st * sc * ce);
            gy = g * (st * cp * cc + ct * cp * sc * ce - sp * sc * se);
            gz = g * (st * sp * cc + ct * sp * sc * ce + cp * sc * se);
            electron.vx = wx + f2 * gx;
            electron.vy = wy + f2 * gy;
            electron.vz = wz + f2 * gz;

            return ElectronOutcome::Ionized {
                new_electron,
                new_ion,
            };
        }
        lxcat::ProcessKind::Excitation => {
            let mut energy = 0.5 * E_MASS * g.powf(2.0);
            energy = (energy - loss).abs(); // subtract excitation energy loss
            g = (2.0 * energy / E_MASS).sqrt();
            chi = (1.0 - 2.0 * rng.gen::<f64>()).acos(); // isotropic scattering
            eta = TWO_PI * rng.gen::<f64>();
        }
        lxcat::ProcessKind::Elastic | lxcat::ProcessKind::Effective => {
            chi = (1.0 - 2.0 * rng.gen::<f64>()).acos(); // isotropic scattering
            eta = TWO_PI * rng.gen::<f64>();
        }
    }

    // scatter the incoming electron
    sc = chi.sin();
    cc = chi.cos();
    se = eta.sin();
    ce = eta.cos();

    // compute new relative velocity:
    gx = g * (ct * cc - st * sc * ce);
    gy = g * (st * cp * cc + ct * cp * sc * ce - sp * sc * se);
    gz = g * (st * sp * cc + ct * sp * sc * ce + cp * sc * se);

    // post-collision velocity of the electron
    electron.vx = wx + f2 * gx;
    electron.vy = wy + f2 * gy;
    electron.vz = wz + f2 * gz;

    ElectronOutcome::None
}

#[derive(Default)]
struct CollisionProducts {
    collisions: u64,
    electrons: Vec<ParticleType>,
    ions: Vec<ParticleType>,
    negative_ions: Vec<ParticleType>,
}

impl CollisionProducts {
    fn append(mut self, mut other: CollisionProducts) -> CollisionProducts {
        self.collisions += other.collisions;
        self.electrons.append(&mut other.electrons);
        self.ions.append(&mut other.ions);
        self.negative_ions.append(&mut other.negative_ions);
        self
    }
}

fn check_collisions_e(
    Electrons: &mut Vec<ParticleType>,
    Ions: &mut Vec<ParticleType>,
    NegativeIons: &mut Vec<ParticleType>,
    data: &CrossSectionData,
    N_coll: &mut u64,
    _rng: &mut ThreadRng,
) {
    let config = one_d_sim_config();
    let N_e: usize = Electrons.len();
    let products = Electrons[..N_e]
        .par_iter_mut()
        .map_init(
            || rand::thread_rng(),
            |rng, electron| {
                let v2 = electron.vx.powf(2.0) + electron.vy.powf(2.0) + electron.vz.powf(2.0);
                let velocity: f64 = v2.sqrt();
                let energy: f64 = 0.5 * E_MASS * v2 / EV_TO_J;
                let energy_index = core::cmp::min(
                    (energy / (DE_CS as f64) + 0.5).trunc() as usize,
                    (CS_RANGES - 1) as usize,
                );

                let nu: f64 = data.sigma_tot_e[energy_index] * velocity;
                let p_coll: f64 = 1.0 - (-nu * config.dt_e()).exp();
                if rng.gen::<f64>() < p_coll {
                    let generated = collision_electron(electron, data, energy_index, rng);
                    (1u64, generated)
                } else {
                    (0u64, ElectronOutcome::None)
                }
            },
        )
        .fold(
            CollisionProducts::default,
            |mut products, (collisions, generated)| {
                products.collisions += collisions;
                match generated {
                    ElectronOutcome::Ionized {
                        new_electron,
                        new_ion,
                    } => {
                        products.electrons.push(new_electron);
                        products.ions.push(new_ion);
                    }
                    ElectronOutcome::Attached { negative_ion } => {
                        products.negative_ions.push(negative_ion);
                    }
                    ElectronOutcome::None => {}
                }
                products
            },
        )
        .reduce(CollisionProducts::default, |left, right| left.append(right));

    *N_coll += products.collisions;
    // 莉倡捩縺ｧ豸亥､ｱ縺励◆髮ｻ蟄・(x = NaN) 繧帝勁蜴ｻ縺吶ｋ
    Electrons.retain(|p| p.x.is_finite());
    let room_e = config.max_particles.saturating_sub(Electrons.len());
    let room_i = config.max_particles.saturating_sub(Ions.len());
    let pairs_to_add = room_e
        .min(room_i)
        .min(products.electrons.len())
        .min(products.ions.len());
    Electrons.extend(products.electrons.into_iter().take(pairs_to_add));
    Ions.extend(products.ions.into_iter().take(pairs_to_add));
    // 莉倡捩縺ｧ逕滓・縺励◆雋繧､繧ｪ繝ｳ繧定ｿｽ蜉縺吶ｋ
    let room_n = config.max_particles.saturating_sub(NegativeIons.len());
    let neg_to_add = room_n.min(products.negative_ions.len());
    NegativeIons.extend(products.negative_ions.into_iter().take(neg_to_add));
}

fn apply_go2010_air_townsend_ionization(
    electrons: &mut Vec<ParticleType>,
    ions: &mut Vec<ParticleType>,
    efield: &[f64],
    n_coll: &mut u64,
    rng: &mut ThreadRng,
) {
    let config = one_d_sim_config();
    let original_electron_count = electrons.len();
    let ion_normal = Normal::new(
        0.0,
        (K_BOLTZMANN * config.temperature_k / config.ion_mass_kg()).sqrt(),
    )
    .unwrap();
    let electron_birth_energy_ev: f64 = 2.0;
    let electron_birth_speed = (2.0 * electron_birth_energy_ev * EV_TO_J / E_MASS).sqrt();

    let mut new_electrons = Vec::new();
    let mut new_ions = Vec::new();
    for electron in electrons.iter().take(original_electron_count) {
        let electron_room = config
            .max_particles
            .saturating_sub(electrons.len() + new_electrons.len());
        let ion_room = config
            .max_particles
            .saturating_sub(ions.len() + new_ions.len());
        let available_pairs = electron_room.min(ion_room);
        if available_pairs == 0 {
            break;
        }
        if electron.x <= 0.0 || electron.x >= config.gap_m {
            continue;
        }
        let local_field = interpolate_efield(efield, electron.x).abs();
        let alpha = go2010_air_townsend_alpha(local_field, config.pressure_pa);
        if alpha <= 0.0 {
            continue;
        }
        let speed = (electron.vx.powf(2.0) + electron.vy.powf(2.0) + electron.vz.powf(2.0)).sqrt();
        let expected = alpha * speed * config.dt_e();
        let generated = stochastic_count(expected, rng).min(available_pairs);
        for _ in 0..generated {
            let mu = 1.0 - 2.0 * rng.gen::<f64>();
            let transverse = (1.0 - mu * mu).sqrt();
            let azimuth = TWO_PI * rng.gen::<f64>();
            new_electrons.push(ParticleType {
                x: electron.x,
                vx: electron_birth_speed * mu,
                vy: electron_birth_speed * transverse * azimuth.cos(),
                vz: electron_birth_speed * transverse * azimuth.sin(),
            });
            new_ions.push(ParticleType {
                x: electron.x,
                vx: rng.sample(ion_normal),
                vy: rng.sample(ion_normal),
                vz: rng.sample(ion_normal),
            });
        }
    }

    *n_coll += new_electrons.len() as u64;
    electrons.extend(new_electrons);
    ions.extend(new_ions);
}

fn interpolate_efield(efield: &[f64], x: f64) -> f64 {
    let config = one_d_sim_config();
    if efield.is_empty() {
        return 0.0;
    }
    if x <= 0.0 {
        return efield[0];
    }
    if x >= config.gap_m {
        return efield[efield.len() - 1];
    }
    let p = ((x * config.inv_dx()).trunc() as usize).min(efield.len().saturating_sub(2));
    let c2 = x * config.inv_dx() - (p as f64);
    (1.0 - c2) * efield[p] + c2 * efield[p + 1]
}

fn go2010_air_townsend_alpha(electric_field_v_m: f64, pressure_pa: f64) -> f64 {
    if electric_field_v_m <= 0.0 || pressure_pa <= 0.0 {
        return 0.0;
    }
    let exponent = -GO2010_AIR_TOWNSEND_B_V_PER_PA_M * pressure_pa / electric_field_v_m;
    let alpha = GO2010_AIR_TOWNSEND_A_PER_PA_M * pressure_pa * exponent.exp();
    if alpha.is_finite() {
        alpha
    } else {
        0.0
    }
}

fn stochastic_count(expected: f64, rng: &mut ThreadRng) -> usize {
    if !expected.is_finite() || expected <= 0.0 {
        return 0;
    }
    let guaranteed = expected.floor().min((usize::MAX - 1) as f64) as usize;
    let fractional = expected - guaranteed as f64;
    guaranteed + usize::from(rng.gen::<f64>() < fractional)
}

//----------------------------------------------------------------------
// initialization routines
//----------------------------------------------------------------------

fn init_particles(np: usize, length: f64, Particle: &mut Vec<ParticleType>, rng: &mut ThreadRng) {
    let config = one_d_sim_config();
    let normal = Normal::new(0.0, (K_BOLTZMANN * config.temperature_k / AR_MASS).sqrt()).unwrap();
    for _i in 0..np {
        let p0 = ParticleType {
            x: length * rng.gen::<f64>(),
            vx: rng.sample(normal),
            vy: rng.sample(normal),
            vz: rng.sample(normal),
        };
        Particle.push(p0);
    }
}

fn solve_poisson(
    pot: &mut Vec<f64>,
    efield: &mut Vec<f64>,
    rho: &Vec<f64>,
    powered_potential: f64,
    grounded_potential: f64,
) {
    const A: f64 = 1.0;
    const B: f64 = -2.0;
    const C: f64 = 1.0;
    let config = one_d_sim_config();
    let n_grid = config.n_grid;
    let dx = config.dx();
    let inv_dx = config.inv_dx();
    let alpha: f64 = -dx * dx / EPSILON0;
    let mut h = vec![0.0; n_grid];
    let mut w = vec![0.0; n_grid];
    let mut f = vec![0.0; n_grid];

    pot[0] = powered_potential;
    pot[n_grid - 1] = grounded_potential;

    for i in 1..(n_grid - 1) {
        f[i] = alpha * rho[i];
    }
    f[1] -= pot[0];
    f[n_grid - 2] -= pot[n_grid - 1];
    w[1] = C / B;
    h[1] = f[1] / B;
    for i in 2..(n_grid - 1) {
        w[i] = C / (B - A * w[i - 1]);
        h[i] = (f[i] - A * h[i - 1]) / (B - A * w[i - 1]);
    }
    pot[n_grid - 2] = h[n_grid - 2];
    for i in (1..(n_grid - 2)).rev() {
        pot[i] = h[i] - w[i] * pot[i + 1];
    }

    for i in 1..(n_grid - 1) {
        efield[i] = 0.5 * (pot[i - 1] - pot[i + 1]) * inv_dx;
    }
    efield[0] = (pot[0] - pot[1]) * inv_dx - rho[0] * dx / (2.0 * EPSILON0);
    efield[n_grid - 1] =
        (pot[n_grid - 2] - pot[n_grid - 1]) * inv_dx + rho[n_grid - 1] * dx / (2.0 * EPSILON0);
}

//----------------------------------------------------------------------
// compute densitites from particle positions
//----------------------------------------------------------------------

fn get_density(density: &mut Vec<f64>, cumul_density: &mut Vec<f64>, Particle: &Vec<ParticleType>) {
    let config = one_d_sim_config();
    let n_grid = config.n_grid;
    let inv_dx = config.inv_dx();
    let c: f64 = config.weight / (config.electrode_area_m2 * config.dx());
    *density = Particle
        .par_chunks(1024)
        .map(|chunk| {
            let mut local_density = vec![0.0; n_grid];
            for p in chunk.iter() {
                let q: usize = core::cmp::min((p.x * inv_dx).trunc() as usize, n_grid - 2);
                let rem: f64 = p.x * inv_dx - (q as f64);
                local_density[q] += (1.0 - rem) * c;
                local_density[q + 1] += rem * c;
            }
            local_density
        })
        .reduce(
            || vec![0.0; n_grid],
            |mut left, right| {
                for i in 0..n_grid {
                    left[i] += right[i];
                }
                left
            },
        );

    density[0] *= 2.0;
    density[n_grid - 1] *= 2.0;

    *cumul_density = cumul_density
        .iter()
        .zip(density.iter())
        .map(|(x, y)| y + x)
        .collect();
}

fn save_particle_data(
    filename: String,
    cycle_done: usize,
    Electrons: &Vec<ParticleType>,
    Ions: &Vec<ParticleType>,
    NegativeIons: &Vec<ParticleType>,
) -> std::io::Result<()> {
    let mut file = std::fs::File::create(filename).expect("unable to open file for writing");
    bincode::serialize_into(&mut file, &cycle_done).expect("unable to write to file");
    bincode::serialize_into(&mut file, &Electrons).expect("unable to write to file");
    bincode::serialize_into(&mut file, &Ions).expect("unable to write to file");
    bincode::serialize_into(&mut file, &NegativeIons).expect("unable to write to file");
    Ok(())
}

fn save_anim(filename: &str, anim: &AnimData) -> std::io::Result<()> {
    let file = File::create(filename)?;
    serde_json::to_writer(file, anim)
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, err))
}

fn load_particle_data(
    filename: String,
) -> (
    usize,
    Vec<ParticleType>,
    Vec<ParticleType>,
    Vec<ParticleType>,
) {
    if !std::path::Path::new(&filename).exists() {
        println!(">> Rust-PIC: ERROR: No particle data file found, try running initial cycle using argument '0'");
        std::process::exit(0);
    }
    let mut file = std::fs::File::open(filename).expect("unable to open file for reading");
    let c: usize = bincode::deserialize_from(&mut file).expect("unable to read from file");
    let Electrons: Vec<ParticleType> =
        bincode::deserialize_from(&mut file).expect("unable to read from file");
    let Ions: Vec<ParticleType> =
        bincode::deserialize_from(&mut file).expect("unable to read from file");
    let NegativeIons: Vec<ParticleType> =
        bincode::deserialize_from(&mut file).unwrap_or_else(|_| Vec::new());
    (c, Electrons, Ions, NegativeIons)
}

// 陦晉ｪ∵妙髱｢遨阪ョ繝ｼ繧ｿ (繝励Ο繧ｻ繧ｹ謨ｰ蜿ｯ螟・縲る崕蟄宣℃遞九・蠑ｾ諤ｧ繝ｻ蜉ｱ襍ｷ(隍・焚)繝ｻ髮ｻ髮｢繝ｻ莉倡捩繧剃ｻｻ諢丞倶ｿ晄戟縲・
struct CrossSectionData {
    e_sigma: Vec<Vec<f64>>, // [n_e][CS_RANGES] 髮ｻ蟄仙推驕守ｨ九・邏疲妙髱｢遨・[m^2]
    e_kind: Vec<lxcat::ProcessKind>,
    e_loss_j: Vec<f64>,    // 蜷・℃遞九・繧ｨ繝阪Ν繧ｮ繝ｼ謳榊､ｱ [J]
    e_labels: Vec<String>, // cs.dat / GUI 陦ｨ遉ｺ逕ｨ
    i_iso: Vec<f64>,       // 繧､繧ｪ繝ｳ遲画婿蠑ｾ諤ｧ [m^2]
    i_back: Vec<f64>,      // 繧､繧ｪ繝ｳ蠕梧婿謨｣荵ｱ [m^2]
    sigma_tot_e: Vec<f64>, // ﾎ｣(e_sigma)*gas_density [1/m]
    sigma_tot_i: Vec<f64>, // (i_iso+i_back)*gas_density [1/m]
}

impl CrossSectionData {
    // 蜈ｨ髮ｻ髮｢驕守ｨ九・譁ｭ髱｢遨榊柱 (險ｺ譁ｭ: 髮ｻ髮｢繝ｬ繝ｼ繝郁ｨ育ｮ礼畑)
    fn ionization_sigma(&self, eindex: usize) -> f64 {
        self.e_sigma
            .iter()
            .zip(self.e_kind.iter())
            .filter(|(_, k)| **k == lxcat::ProcessKind::Ionization)
            .map(|(s, _)| s[eindex])
            .sum()
    }
}

// LXCat 繧ｬ繧ｹ縺ｮ遒ｺ螳壹ョ繝ｼ繧ｿ (parse 譎ゅ↓讒狗ｯ峨＠縲∵妙髱｢遨阪・雉ｪ驥上・蜿ら・蜈・↓縺吶ｋ)
struct LxcatGasData {
    processes: Vec<lxcat::LxcatProcess>,
    gas_mass_kg: f64,
    ion_mass_kg: f64,
}

static LXCAT_GAS_DATA: OnceLock<LxcatGasData> = OnceLock::new();

fn lxcat_gas_data() -> &'static LxcatGasData {
    match LXCAT_GAS_DATA.get() {
        Some(d) => d,
        None => {
            println!(">> Rust-PIC: ERROR = LXCat gas data was not initialized");
            std::process::exit(1);
        }
    }
}

fn set_lxcat_gas_data(data: LxcatGasData) {
    let _ = LXCAT_GAS_DATA.set(data);
}

// LXCat 繝輔ぃ繧､繝ｫ縺九ｉ繧ｬ繧ｹ遒ｺ螳壹ョ繝ｼ繧ｿ (譁ｭ髱｢遨阪・雉ｪ驥・ 繧呈ｧ狗ｯ峨☆繧九・
fn build_lxcat_gas_data(
    path: &str,
    gas_mass_override: f64,
    ion_mass_override: f64,
) -> Result<LxcatGasData, String> {
    let processes = lxcat::parse_lxcat_file(path)?;
    let gas_mass = if gas_mass_override > 0.0 {
        gas_mass_override
    } else {
        let mass_ratio = processes
            .iter()
            .find(|p| {
                matches!(
                    p.kind,
                    lxcat::ProcessKind::Elastic | lxcat::ProcessKind::Effective
                )
            })
            .map(|p| p.mass_ratio)
            .filter(|&m| m > 0.0);
        match mass_ratio {
            Some(m) => E_MASS / m,
            None => {
                return Err("LXCat: ELASTIC/EFFECTIVE 縺ｮ雉ｪ驥乗ｯ斐′辟｡縺・◆繧・--gas-mass-kg 繧呈欠螳壹＠縺ｦ縺上□縺輔＞".to_string())
            }
        }
    };
    let ion_mass = if ion_mass_override > 0.0 {
        ion_mass_override
    } else {
        gas_mass
    };
    if processes
        .iter()
        .all(|p| p.kind != lxcat::ProcessKind::Ionization)
    {
        println!(">> Rust-PIC: WARNING = LXCat 繝・・繧ｿ縺ｫ IONIZATION 驕守ｨ九′縺ゅｊ縺ｾ縺帙ｓ");
    }
    Ok(LxcatGasData {
        processes,
        gas_mass_kg: gas_mass,
        ion_mass_kg: ion_mass,
    })
}

fn lxcat_process_label(p: &lxcat::LxcatProcess) -> String {
    let kind = match p.kind {
        lxcat::ProcessKind::Elastic => "elastic",
        lxcat::ProcessKind::Effective => "effective",
        lxcat::ProcessKind::Excitation => "excitation",
        lxcat::ProcessKind::Ionization => "ionization",
        lxcat::ProcessKind::Attachment => "attachment",
    };
    match &p.product {
        Some(prod) => format!("{kind} ({prod})"),
        None => kind.to_string(),
    }
}

// CS_RANGES 譬ｼ蟄舌・繧ｨ繝阪Ν繧ｮ繝ｼ [eV]縲・ 逡ｪ逶ｮ縺ｯ 0 髯､邂怜屓驕ｿ縺ｮ縺溘ａ DE_CS 縺ｫ縺壹ｉ縺吶・
fn energy_grid() -> Vec<f64> {
    let mut e_vec: Vec<f64> = (0..CS_RANGES).map(|x| (x as f64) * DE_CS).collect();
    e_vec[0] = DE_CS;
    e_vec
}

// 陦晉ｪ∝捉豕｢謨ｰ逕ｨ縺ｮ邱乗妙髱｢遨・(ﾃ励ぎ繧ｹ蟇・ｺｦ) 繧定ｨ育ｮ励☆繧九・
fn accumulate_totals(data: &mut CrossSectionData, gas_density: f64) {
    for sigma in data.e_sigma.iter() {
        for i in 0..CS_RANGES {
            data.sigma_tot_e[i] += sigma[i] * gas_density;
        }
    }
    for i in 0..CS_RANGES {
        data.sigma_tot_i[i] = (data.i_iso[i] + data.i_back[i]) * gas_density;
    }
}

fn init_cross_sections() -> CrossSectionData {
    let config = one_d_sim_config();
    let gas_density = config.gas_density();
    match config.gas_model {
        OneDGasModel::Go2010AirTownsend => CrossSectionData {
            e_sigma: Vec::new(),
            e_kind: Vec::new(),
            e_loss_j: Vec::new(),
            e_labels: Vec::new(),
            i_iso: vec![0.0; CS_RANGES],
            i_back: vec![0.0; CS_RANGES],
            sigma_tot_e: vec![0.0; CS_RANGES],
            sigma_tot_i: vec![0.0; CS_RANGES],
        },
        OneDGasModel::ArgonPic => build_argon_cross_sections(gas_density),
        OneDGasModel::Lxcat => build_lxcat_cross_sections(config, gas_density),
    }
}

// Argon 縺ｮ隗｣譫仙ｼ上↓繧医ｋ譁ｭ髱｢遨・(蠕捺擂縺ｮ Rust-PIC 繝｢繝・Ν)
fn build_argon_cross_sections(gas_density: f64) -> CrossSectionData {
    let qmom = |e: f64| {
        1.0e-20
            * ((6.0 / (1.0 + e / 0.1 + (e / 0.6).powf(2.0)).powf(3.3)
                - 1.1 * e.powf(1.4)
                    / (1.0 + (e / 15.0).powf(1.2))
                    / (1.0 + (e / 5.5).powf(2.5) + (e / 60.0).powf(4.1)).sqrt())
            .abs()
                + 0.05 / (1.0 + e / 10.0).powf(2.0)
                + 0.01 * e.powf(3.0) / (1.0 + (e / 12.0).powf(6.0)))
    };
    let qexc = |e: f64| {
        if e <= E_EXC_TH {
            0.0
        } else {
            (0.034 * (e - 11.5).powf(1.1) * (1.0 + (e / 15.0).powf(2.8))
                / (1.0 + (e / 23.0).powf(5.5))
                + 0.023 * (e - 11.5) / (1.0 + e / 80.0).powf(1.9))
                * 1.0e-20
        }
    };
    let qion = |e: f64| {
        if e <= E_ION_TH {
            0.0
        } else {
            (970.0 * (e - 15.8) / (70.0 + e).powf(2.0)
                + 0.06 * (e - 15.8).powf(2.0) * (-e / 9.0).exp())
                * 1.0e-20
        }
    };
    let qmoi = |e_lab: f64| 1.15e-18 * e_lab.powf(-0.1) * (1.0 + 0.015 / e_lab).powf(0.6);
    let qiso = |e_lab: f64| {
        2.0e-19 * e_lab.powf(-0.5) / (1.0 + e_lab) + 3.0e-19 * e_lab / (1.0 + e_lab / 3.0).powf(2.0)
    };
    let qchx = |e_lab: f64| 0.5 * (qmoi(e_lab) - qiso(e_lab));

    let e_vec = energy_grid();
    let elastic: Vec<f64> = e_vec.iter().map(|&e| qmom(e)).collect();
    let excitation: Vec<f64> = e_vec.iter().map(|&e| qexc(e)).collect();
    let ionization: Vec<f64> = e_vec.iter().map(|&e| qion(e)).collect();
    let i_iso: Vec<f64> = e_vec.iter().map(|&e| qiso(2.0 * e)).collect();
    let i_back: Vec<f64> = e_vec.iter().map(|&e| qchx(2.0 * e)).collect();

    let mut data = CrossSectionData {
        e_sigma: vec![elastic, excitation, ionization],
        e_kind: vec![
            lxcat::ProcessKind::Elastic,
            lxcat::ProcessKind::Excitation,
            lxcat::ProcessKind::Ionization,
        ],
        e_loss_j: vec![0.0, E_EXC_TH * EV_TO_J, E_ION_TH * EV_TO_J],
        e_labels: vec![
            "elastic".to_string(),
            "excitation".to_string(),
            "ionization".to_string(),
        ],
        i_iso,
        i_back,
        sigma_tot_e: vec![0.0; CS_RANGES],
        sigma_tot_i: vec![0.0; CS_RANGES],
    };
    accumulate_totals(&mut data, gas_density);
    data
}

// LXCat 繝・・繝悶Ν繧・CS_RANGES 譬ｼ蟄舌∈邱壼ｽ｢陬憺俣縺励◆譁ｭ髱｢遨・
fn build_lxcat_cross_sections(config: &OneDSimConfig, gas_density: f64) -> CrossSectionData {
    let gas = lxcat_gas_data();
    let e_vec = energy_grid();

    let mut e_sigma = Vec::new();
    let mut e_kind = Vec::new();
    let mut e_loss_j = Vec::new();
    let mut e_labels = Vec::new();

    for p in &gas.processes {
        let sigma: Vec<f64> = e_vec
            .iter()
            .map(|&e| lxcat::interpolate(&p.table, e))
            .collect();
        e_sigma.push(sigma);
        e_kind.push(p.kind);
        e_loss_j.push(p.threshold_ev * EV_TO_J);
        e_labels.push(lxcat_process_label(p));
    }

    // 繧､繧ｪ繝ｳ-荳ｭ諤ｧ蠑ｾ諤ｧ縺ｯ LXCat 縺ｫ蜷ｫ縺ｾ繧後↑縺・◆繧√ワ繝ｼ繝峨せ繝輔ぅ繧｢霑台ｼｼ (螳壽焚)
    let i_iso = vec![config.ion_hs_sigma_iso_m2; CS_RANGES];
    let i_back = vec![config.ion_hs_sigma_back_m2; CS_RANGES];

    let mut data = CrossSectionData {
        e_sigma,
        e_kind,
        e_loss_j,
        e_labels,
        i_iso,
        i_back,
        sigma_tot_e: vec![0.0; CS_RANGES],
        sigma_tot_i: vec![0.0; CS_RANGES],
    };
    accumulate_totals(&mut data, gas_density);
    data
}

// calculate mean impact energies
fn calc_fed(
    fed_pow: &Vec<u64>,
    fed_gnd: &Vec<u64>,
    mean_energy_pow: &mut f64,
    mean_energy_gnd: &mut f64,
) -> std::io::Result<()> {
    let h_pow: f64 = (fed_pow.iter().sum::<u64>() as f64) * DE_FED;
    let h_gnd: f64 = (fed_gnd.iter().sum::<u64>() as f64) * DE_FED;
    *mean_energy_pow = 0.0;
    *mean_energy_gnd = 0.0;
    let mut energy: f64;
    for i in 0..N_FED {
        energy = (0.5 + i as f64) * DE_FED;
        *mean_energy_pow += energy * (fed_pow[i] as f64) / h_pow;
        *mean_energy_gnd += energy * (fed_gnd[i] as f64) / h_gnd;
    }
    Ok(())
}

fn calc_current_and_power(
    j_xt: &mut Vec<Vec<f64>>,
    pow_xt: &mut Vec<Vec<f64>>,
    u_xt: &Vec<Vec<f64>>,
    n_xt: &Vec<Vec<f64>>,
    c_xt: &Vec<Vec<f64>>,
    efield_xt: &Vec<Vec<f64>>,
    charge: f64,
    norm: f64,
) -> std::io::Result<()> {
    let config = one_d_sim_config();
    let n_xt_len = config.n_xt();
    let n_grid = config.n_grid;
    let mut factor: f64;
    let mut u: f64;
    for i in 0..n_xt_len {
        for j in 0..n_grid {
            factor = c_xt[i][j];
            if factor > 0.0 {
                factor = 1.0 / factor;
            } else {
                factor = 0.0;
            }
            u = u_xt[i][j] * factor;
            j_xt[i][j] = charge * u * n_xt[i][j] * norm;
            pow_xt[i][j] = j_xt[i][j] * efield_xt[i][j] * norm;
        }
    }
    Ok(())
}
// formatted output of cumulative densities
fn save_densities(
    filename: &str,
    eden: &Vec<f64>,
    iden: &Vec<f64>,
    nden: &Vec<f64>,
    cycle: usize,
) -> std::io::Result<()> {
    let config = one_d_sim_config();
    let mut file = File::create(filename)?;
    writeln!(file, "# x[m]\tne[m^-3]\tni[m^-3]\tnn[m^-3]")?;
    let e_norm = config.steps_per_period as f64 * cycle as f64;
    let ion_norm = config.steps_per_period as f64 * cycle as f64 / config.ion_subcycling as f64;
    for i in 0..config.n_grid {
        writeln!(
            file,
            "{:1.6e} \t{:1.6e} \t{:1.6e} \t{:1.6e}",
            (i as f64) * config.dx(),
            eden[i] / e_norm,
            iden[i] / ion_norm,
            nden[i] / ion_norm
        );
    }
    Ok(())
}

// save EEPF data
fn save_eepf(filename: &str, eepf: &Vec<f64>) -> std::io::Result<()> {
    let mut file = File::create(filename)?;
    let h: f64 = eepf.iter().sum::<f64>() * DE_EEPF;
    for i in 0..N_EEPF {
        let energy: f64 = (0.5 + i as f64) * DE_EEPF;
        writeln!(
            file,
            "{:1.6e} \t{:1.6e}",
            energy,
            eepf[i] / h / energy.sqrt()
        );
    }
    Ok(())
}

// save FED data
fn save_fed(filename: &str, fed_pow: &Vec<u64>, fed_gnd: &Vec<u64>) -> std::io::Result<()> {
    let mut file = File::create(filename)?;
    let h_pow: f64 = (fed_pow.iter().sum::<u64>() as f64) * DE_FED;
    let h_gnd: f64 = (fed_gnd.iter().sum::<u64>() as f64) * DE_FED;
    for i in 0..N_FED {
        let energy: f64 = (0.5 + i as f64) * DE_FED;
        let p = (fed_pow[i] as f64) / h_pow;
        let g = (fed_gnd[i] as f64) / h_gnd;
        writeln!(file, "{:1.6e} \t{:10} \t{:10}", energy, p, g);
    }
    Ok(())
}

fn save_iadf(filename: &str, adf_pow: &Vec<u64>, adf_gnd: &Vec<u64>) -> std::io::Result<()> {
    let config = one_d_sim_config();
    let da = config.da_adf();
    let mut file = File::create(filename)?;
    let h_pow: f64 = (adf_pow.iter().sum::<u64>() as f64) * da;
    let h_gnd: f64 = (adf_gnd.iter().sum::<u64>() as f64) * da;
    for i in 0..config.n_adf {
        let angle: f64 = (0.5 + i as f64) * da;
        let p = if h_pow > 0.0 {
            (adf_pow[i] as f64) / h_pow
        } else {
            0.0
        };
        let g = if h_gnd > 0.0 {
            (adf_gnd[i] as f64) / h_gnd
        } else {
            0.0
        };
        writeln!(file, "{:1.6e} \t{:10} \t{:10}", angle, p, g)?;
    }
    Ok(())
}

fn save_i2adf(filename: &str, adf2d: &Vec<Vec<u64>>) -> std::io::Result<()> {
    let config = one_d_sim_config();
    let da = config.da_2adf();
    let da2 = da * da;
    let total: u64 = adf2d.iter().flat_map(|row| row.iter()).sum();
    let norm = if total > 0 {
        1.0 / (total as f64 * da2)
    } else {
        0.0
    };
    let mut file = File::create(filename)?;
    for row in adf2d {
        let vals: Vec<String> = row
            .iter()
            .map(|&c| format!("{:1.6e}", c as f64 * norm))
            .collect();
        writeln!(file, "{}", vals.join("  "))?;
    }
    Ok(())
}

// save XT data
fn save_xt_1(filename: &str, xt: &Vec<Vec<f64>>, norm: f64) -> std::io::Result<()> {
    let config = one_d_sim_config();
    let mut file = File::create(filename)?;
    for j in 0..config.n_grid {
        for i in 0..config.n_xt() {
            write!(file, "{:1.6e}  ", xt[i][j] * norm);
        }
        writeln!(file, "");
    }
    Ok(())
}

fn save_xt_2(filename: &str, xt: &Vec<Vec<f64>>, norm: &Vec<Vec<f64>>) -> std::io::Result<()> {
    let config = one_d_sim_config();
    let mut file = File::create(filename)?;
    let mut factor: f64;
    for j in 0..config.n_grid {
        for i in 0..config.n_xt() {
            factor = norm[i][j];
            if factor > 0.0 {
                factor = 1.0 / factor;
            } else {
                factor = 0.0;
            }
            write!(file, "{:1.6e}  ", xt[i][j] * factor);
        }
        writeln!(file, "");
    }
    Ok(())
}

// formatted output of cross-sections for testing
fn check_cross_sections(filename: &str, data: &CrossSectionData) -> std::io::Result<()> {
    // 繝励Ο繧ｻ繧ｹ謨ｰ蜿ｯ螟峨ょ・讒区・: energy + 髮ｻ蟄仙推驕守ｨ・+ i_iso + i_back縲・
    // 蜈磯ｭ縺ｮ繝倥ャ繝陦後・謨ｰ蛟､繝代・繧ｵ蛛ｴ縺ｧ辟｡隕悶＆繧後ｋ縲・
    const N_SAVE: u32 = 1000;
    let mut file = File::create(filename)?;
    let mut header = String::from("# energy[eV]");
    for label in &data.e_labels {
        header.push('\t');
        header.push_str(label);
    }
    header.push_str("\ti_iso\ti_back");
    writeln!(file, "{}", header)?;

    let factor: f64 = (CS_RANGES as f64).powf(1.0 / (N_SAVE as f64));
    for j in 1..N_SAVE {
        let en: f64 = DE_CS * factor.powf(j as f64);
        let i: usize = (en / DE_CS).trunc() as usize;
        write!(file, "{:1.6e}", en)?;
        for sigma in &data.e_sigma {
            write!(file, " \t{:1.6e}", sigma[i])?;
        }
        write!(file, " \t{:1.6e} \t{:1.6e}", data.i_iso[i], data.i_back[i])?;
        writeln!(file)?;
    }
    Ok(())
}

// simulation report including stability and accuracy conditions       //
fn check_and_save_info(
    filename: &str,
    ne_max: f64,
    cycle: usize,
    mean_ee: f64,
    N_ee: u64,
    N_e: usize,
    N_i: usize,
    N_n: usize,
    N_e_coll: u64,
    N_i_coll: u64,
    total_cs_e: &Vec<f64>,
    total_cs_i: &Vec<f64>,
    mean_e_energy_pow: f64,
    mean_e_energy_gnd: f64,
    mean_i_energy_pow: f64,
    mean_i_energy_gnd: f64,
    N_e_abs_pow: u64,
    N_e_abs_gnd: u64,
    N_i_abs_pow: u64,
    N_i_abs_gnd: u64,
    N_n_abs_pow: u64,
    N_n_abs_gnd: u64,
    no_of_cycles: usize,
    powere_xt: &Vec<Vec<f64>>,
    poweri_xt: &Vec<Vec<f64>>,
    voltage_config: &OneDBoundaryVoltage,
    surface_config: &surface::SurfaceConfig,
    surface_stats: &surface::SurfaceStats,
    conditions_OK: &mut bool,
) -> std::io::Result<()> {
    let config = one_d_sim_config();
    let mut file = File::create(filename)?;
    let density: f64 = ne_max / (cycle as f64) / (config.steps_per_period as f64); // e density @ center
    let plas_freq: f64 = E_CHARGE * (density / EPSILON0 / E_MASS).sqrt(); // e plasma frequency @ center
    let meane: f64 = mean_ee / (N_ee as f64); // e mean energy @ center
    let kT: f64 = 2.0 * meane * EV_TO_J / 3.0; // k T_e @ center (approximate)
    let debye_length: f64 = (EPSILON0 * kT / density).sqrt() / E_CHARGE; // e Debye length @ center
    let sim_time: f64 = (cycle as f64) / config.frequency_hz; // simulated time
    let ecoll_freq: f64 = (N_e_coll as f64) / sim_time / (N_e as f64); // e collision frequency
    let icoll_freq: f64 = (N_i_coll as f64) / sim_time / (N_i as f64); // ion collision frequency

    // find upper limit of collision frequencies
    let mut max_ecoll_freq: f64 = 0.0;
    let mut max_icoll_freq: f64 = 0.0;
    let mut e: f64;
    let mut v: f64;
    let mut nu: f64;
    for i in 0..CS_RANGES {
        e = (i as f64) * DE_CS;
        v = (2.0 * e * EV_TO_J / E_MASS).sqrt();
        nu = v * total_cs_e[i];
        if nu > max_ecoll_freq {
            max_ecoll_freq = nu;
        }
        v = (2.0 * e * EV_TO_J / MU_ARAR).sqrt();
        nu = v * total_cs_i[i];
        if nu > max_icoll_freq {
            max_icoll_freq = nu;
        }
    }

    writeln!(
        file,
        "########################## Rust-PIC simulation report ###########################"
    );
    writeln!(file, "Simulation parameters:");
    writeln!(
        file,
        "Gas / ionization model                = {}",
        config.gas_model.as_str()
    );
    writeln!(
        file,
        "Gap distance                          = {:1.6e} [m]",
        config.gap_m
    );
    writeln!(
        file,
        "# of grid divisions                   = {:10}",
        config.n_grid
    );
    writeln!(
        file,
        "Frequency                             = {:1.6e} [Hz]",
        config.frequency_hz
    );
    writeln!(
        file,
        "# of time steps / period              = {:10}",
        config.steps_per_period
    );
    writeln!(
        file,
        "# of electron / ion time steps        = {:10}",
        config.ion_subcycling
    );
    writeln!(
        file,
        "Voltage mode                          = {}",
        voltage_config.mode.as_str()
    );
    writeln!(
        file,
        "Voltage amplitude (RF mode only)      = {:1.6e} [V]",
        voltage_config.rf_amplitude_v
    );
    writeln!(
        file,
        "Powered electrode DC voltage          = {:1.6e} [V]",
        voltage_config.powered_dc_v
    );
    writeln!(
        file,
        "Grounded electrode DC voltage         = {:1.6e} [V]",
        voltage_config.grounded_dc_v
    );
    writeln!(
        file,
        "Pressure                             = {:1.6e} [Pa]",
        config.pressure_pa
    );
    writeln!(
        file,
        "Temperature                           = {:1.6e} [K]",
        config.temperature_k
    );
    writeln!(
        file,
        "Superparticle weight                  = {:1.6e} [m]",
        config.weight
    );
    writeln!(
        file,
        "Electrode area                        = {:1.6e} [m^2]",
        config.electrode_area_m2
    );
    writeln!(
        file,
        "# of initial electrons and ions       = {:10}",
        config.initial_particles
    );
    writeln!(
        file,
        "Max electrons / ions in memory        = {:10}",
        config.max_particles
    );
    writeln!(file, "# of simulation cycles in this run    = {:10}", cycle);
    writeln!(
        file,
        "--------------------------------------------------------------------------------"
    );
    writeln!(file, "Plasma characteristics:");
    writeln!(
        file,
        "Electron density @ center             = {:1.6e} [m^-3]",
        density
    );
    writeln!(
        file,
        "Plasma frequency @ center             = {:1.6e} [rad/s]",
        plas_freq
    );
    writeln!(
        file,
        "Debye length @ center                 = {:1.6e} [m]",
        debye_length
    );
    writeln!(
        file,
        "Electron collision frequency          = {:1.6e} [1/s]",
        ecoll_freq
    );
    writeln!(
        file,
        "Ion collision frequency               = {:1.6e} [1/s]",
        icoll_freq
    );
    writeln!(
        file,
        "--------------------------------------------------------------------------------"
    );
    writeln!(file, "Stability and accuracy conditions:");
    *conditions_OK = true;
    let mut c: f64 = plas_freq * config.dt_e();
    writeln!(
        file,
        "Plasma frequency @ center * DT_E      = {:10.4} (OK if less than 0.20)",
        c
    );
    if c > 0.2 {
        *conditions_OK = false;
    }
    c = config.dx() / debye_length;
    writeln!(
        file,
        "DX / Debye length @ center            = {:10.4} (OK if less than 1.00)",
        c
    );
    if c > 1.0 {
        *conditions_OK = false;
    }
    c = max_ecoll_freq * config.dt_e();
    writeln!(
        file,
        "Max. electron coll. frequency * DT_E  = {:10.4} (OK if less than 0.05)",
        c
    );
    if c > 0.05 {
        *conditions_OK = false;
    }
    c = max_icoll_freq * config.dt_i();
    writeln!(
        file,
        "Max. ion coll. frequency * DT_I       = {:10.4} (OK if less than 0.05)",
        c
    );
    if c > 0.05 {
        *conditions_OK = false;
    }
    if *conditions_OK == false {
        writeln!(
            file,
            "--------------------------------------------------------------------------------"
        );
        writeln!(
            file,
            "** STABILITY AND ACCURACY CONDITION(S) VIOLATED - REFINE SIMULATION SETTINGS! **"
        );
        writeln!(
            file,
            "--------------------------------------------------------------------------------"
        );
        println!(">> Rust-PIC:  WARNING = STABILITY AND ACCURACY CONDITION(S) VIOLATED!");
        println!(
            ">> Rust-PIC:  for details see '{}'; measurement .dat files will still be saved.",
            filename
        );
    }
    // calculate maximum energy for which the Courant condition holds:
    let v_max: f64 = config.dx() / config.dt_e();
    let e_max: f64 = 0.5 * E_MASS * v_max * v_max / EV_TO_J;
    writeln!(
        file,
        "Max e- energy for CFL condition       = {:10.4} [eV]",
        e_max
    );
    writeln!(
        file,
        "Check EEPF to ensure that CFL is fulfilled for the majority of the electrons!"
    );
    writeln!(
        file,
        "--------------------------------------------------------------------------------"
    );
    writeln!(file, "Particle characteristics at the electrodes:");
    writeln!(file, "N_n                                  = {:10}", N_n);
    writeln!(
        file,
        "N_n_abs_pow                          = {:10}",
        N_n_abs_pow
    );
    writeln!(
        file,
        "N_n_abs_gnd                          = {:10}",
        N_n_abs_gnd
    );
    writeln!(
        file,
        "Ion flux at powered electrode         = {:1.6e} [m^(-2) s^(-1)]",
        (N_i_abs_pow as f64) * config.weight
            / config.electrode_area_m2
            / ((no_of_cycles as f64) * config.period())
    );
    writeln!(
        file,
        "Ion flux at grounded electrode        = {:1.6e} [m^(-2) s^(-1)]",
        (N_i_abs_gnd as f64) * config.weight
            / config.electrode_area_m2
            / ((no_of_cycles as f64) * config.period())
    );
    writeln!(
        file,
        "Mean ion energy at powered electrode  = {:1.6e} [eV]",
        mean_i_energy_pow
    );
    writeln!(
        file,
        "Mean ion energy at grounded electrode = {:1.6e} [eV]",
        mean_i_energy_gnd
    );
    writeln!(
        file,
        "Electron flux at powered electrode    = {:1.6e} [m^(-2) s^(-1)]",
        (N_e_abs_pow as f64) * config.weight
            / config.electrode_area_m2
            / ((no_of_cycles as f64) * config.period())
    );
    writeln!(
        file,
        "Electron flux at grounded electrode   = {:1.6e} [m^(-2) s^(-1)]",
        (N_e_abs_gnd as f64) * config.weight
            / config.electrode_area_m2
            / ((no_of_cycles as f64) * config.period())
    );
    writeln!(
        file,
        "Mean electron energy at powered ele.  = {:1.6e} [eV]",
        mean_e_energy_pow
    );
    writeln!(
        file,
        "Mean electron energy at grounded ele. = {:1.6e} [eV]",
        mean_e_energy_gnd
    );
    writeln!(
        file,
        "Electron reflection probability       = {:1.6e}",
        surface_config.electron_reflection_probability
    );
    writeln!(
        file,
        "Electrons reflected at powered ele.   = {:10}",
        surface_stats.electron_reflected_powered
    );
    writeln!(
        file,
        "Electrons reflected at grounded ele.  = {:10}",
        surface_stats.electron_reflected_grounded
    );
    writeln!(
        file,
        "Secondary emission coefficient        = {:1.6e}",
        surface_config.secondary_electron_yield
    );
    writeln!(
        file,
        "Secondary electron emission energy    = {:1.6e} [eV]",
        surface_config.secondary_electron_energy_ev
    );
    writeln!(
        file,
        "Secondary e- emitted at powered ele.  = {:10}",
        surface_stats.secondary_emitted_powered
    );
    writeln!(
        file,
        "Secondary e- emitted at grounded ele. = {:10}",
        surface_stats.secondary_emitted_grounded
    );
    writeln!(
        file,
        "FN emission enabled                   = {:10}",
        surface_config.fn_emission_enabled
    );
    writeln!(
        file,
        "FN work function                      = {:1.6e} [eV]",
        surface_config.fn_work_function_ev
    );
    writeln!(
        file,
        "FN field enhancement factor           = {:1.6e}",
        surface_config.fn_field_enhancement
    );
    writeln!(
        file,
        "FN emission area factor               = {:1.6e}",
        surface_config.fn_emission_area_factor
    );
    writeln!(
        file,
        "Go2010 ion-enhanced FN enabled        = {:10}",
        surface_config.go2010_ion_enhanced_fn_enabled
    );
    writeln!(
        file,
        "Go2010 K                              = {:1.6e}",
        surface_config.go2010_k
    );
    writeln!(
        file,
        "FN e- emitted at powered ele.         = {:10}",
        surface_stats.fn_emitted_powered
    );
    writeln!(
        file,
        "FN e- emitted at grounded ele.        = {:10}",
        surface_stats.fn_emitted_grounded
    );
    writeln!(
        file,
        "Ion-enhanced FN e- at powered ele.    = {:10}",
        surface_stats.ion_enhanced_fn_emitted_powered
    );
    writeln!(
        file,
        "Ion-enhanced FN e- at grounded ele.   = {:10}",
        surface_stats.ion_enhanced_fn_emitted_grounded
    );
    writeln!(
        file,
        "--------------------------------------------------------------------------------"
    );

    // calculate spatially and temporally averaged power absorption by the electrons and ions
    let mut power_e: f64 = 0.0;
    let mut power_i: f64 = 0.0;
    for i in 0..config.n_xt() {
        for j in 0..config.n_grid {
            power_e += powere_xt[i][j];
            power_i += poweri_xt[i][j];
        }
    }
    power_e /= (config.n_xt() * config.n_grid) as f64;
    power_i /= (config.n_xt() * config.n_grid) as f64;
    writeln!(file, "Absorbed power calculated as <j*E>:");
    writeln!(
        file,
        "Electron power density (average)      = {:1.6e} [W m^(-3)]",
        power_e
    );
    writeln!(
        file,
        "Ion power density (average)           = {:1.6e} [W m^(-3)]",
        power_i
    );
    writeln!(
        file,
        "Total power density(average)          = {:1.6e} [W m^(-3)]",
        power_e + power_i
    );
    writeln!(
        file,
        "--------------------------------------------------------------------------------\n"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owned_args(args: &[&str]) -> Vec<String> {
        args.iter().map(|arg| (*arg).to_string()).collect()
    }

    fn assert_close(left: f64, right: f64) {
        assert!(
            (left - right).abs() < 1.0e-9,
            "left={} right={}",
            left,
            right
        );
    }

    #[test]
    fn default_1d_voltage_keeps_original_rf_waveform() {
        let voltage = OneDBoundaryVoltage::default();

        let steps = N_T as usize;
        assert_close(voltage.potentials_at_step(0, steps).0, VOLTAGE);
        assert_close(voltage.potentials_at_step(steps / 2, steps).0, -VOLTAGE);
        assert_close(voltage.potentials_at_step(0, steps).1, 0.0);
    }

    #[test]
    fn dc_1d_voltage_is_constant_at_both_boundaries() {
        let voltage = OneDBoundaryVoltage {
            mode: OneDVoltageMode::Dc,
            rf_amplitude_v: VOLTAGE,
            powered_dc_v: -80.0,
            grounded_dc_v: 5.0,
        };

        let steps = N_T as usize;
        for step in [0, steps / 4, steps / 2, steps - 1] {
            let (powered, grounded) = voltage.potentials_at_step(step, steps);
            assert_close(powered, -80.0);
            assert_close(grounded, 5.0);
        }
    }

    #[test]
    fn parse_1d_dc_voltage_flags() {
        let options = parse_1d_run_options(&owned_args(&[
            "100",
            "m",
            "--voltage-mode",
            "dc",
            "--powered-dc-v",
            "-120.5",
            "--grounded-dc-v=3.0",
        ]))
        .unwrap();

        assert_eq!(options.cycle, 100);
        assert!(options.measurement);
        assert_eq!(options.voltage.mode, OneDVoltageMode::Dc);
        assert_close(options.voltage.powered_dc_v, -120.5);
        assert_close(options.voltage.grounded_dc_v, 3.0);
    }

    #[test]
    fn parse_dc_voltage_alias_implies_dc_mode() {
        let options = parse_1d_run_options(&owned_args(&["0", "--dc-voltage", "-50"])).unwrap();

        assert_eq!(options.voltage.mode, OneDVoltageMode::Dc);
        assert_close(options.voltage.powered_dc_v, -50.0);
    }

    #[test]
    fn parse_1d_runtime_condition_flags() {
        let options = parse_1d_run_options(&owned_args(&[
            "1",
            "--grid-points",
            "256",
            "--steps-per-period",
            "64000",
            "--frequency-hz",
            "1.0e6",
            "--dt-s",
            "2.0e-10",
            "--total-time-s",
            "1.0e-6",
            "--gap-m",
            "0.002",
            "--pressure-pa",
            "12.5",
            "--temperature-k",
            "400",
            "--weight",
            "8e4",
            "--electrode-area-m2",
            "2e-4",
            "--initial-particles",
            "2000",
            "--max-particles",
            "10000",
            "--ion-subcycling",
            "10",
            "--xt-bin-steps",
            "40",
            "--electron-reflection-probability",
            "0.3",
            "--secondary-yield",
            "0.1",
            "--secondary-energy-ev",
            "3.0",
            "--fn-emission-enabled",
            "false",
            "--fn-work-function-ev",
            "4.2",
            "--fn-field-enhancement",
            "1.5",
            "--fn-emission-area-factor",
            "0.5",
            "--gas-model",
            "go2010-air",
            "--go2010-ion-enhanced-fn",
            "true",
            "--go2010-k",
            "1e7",
        ]))
        .unwrap();

        assert_eq!(options.sim.n_grid, 256);
        assert_eq!(options.sim.steps_per_period, 5000);
        assert_close(options.sim.frequency_hz, 1.0e6);
        assert_close(options.sim.dt_e(), 2.0e-10);
        assert_close(options.sim.gap_m, 0.002);
        assert_close(options.sim.pressure_pa, 12.5);
        assert_eq!(options.sim.gas_model, OneDGasModel::Go2010AirTownsend);
        assert_eq!(options.sim.initial_particles, 2000);
        assert_eq!(options.sim.max_particles, 10000);
        assert_eq!(options.sim.ion_subcycling, 10);
        assert_eq!(options.sim.xt_bin_steps, 40);
        assert_close(options.surface.electron_reflection_probability, 0.3);
        assert_close(options.surface.secondary_electron_yield, 0.1);
        assert!(!options.surface.fn_emission_enabled);
        assert!(options.surface.go2010_ion_enhanced_fn_enabled);
        assert_close(options.surface.go2010_k, 1.0e7);
    }

    #[test]
    fn go2010_air_townsend_alpha_increases_with_field() {
        let low = go2010_air_townsend_alpha(1.0e7, 101_325.0);
        let high = go2010_air_townsend_alpha(1.0e8, 101_325.0);

        assert!(low >= 0.0);
        assert!(high > low);
    }
}
