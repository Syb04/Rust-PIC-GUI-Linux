use rand::prelude::*;

use crate::{one_d_sim_config, ParticleType, DE_FED, EV_TO_J, E_CHARGE, E_MASS, N_FED, TWO_PI};

// Go and Pohlman (J. Appl. Phys. 107, 103303, 2010), Fig. 1 representative values.
const DEFAULT_SECONDARY_ELECTRON_YIELD: f64 = 0.01;
const DEFAULT_SECONDARY_ELECTRON_ENERGY_EV: f64 = 2.0;
const DEFAULT_ELECTRON_REFLECTION_PROBABILITY: f64 = 0.2;
const DEFAULT_FN_WORK_FUNCTION_EV: f64 = 4.0;
const DEFAULT_FN_FIELD_ENHANCEMENT: f64 = 50.0;
const DEFAULT_FN_EMISSION_AREA_FACTOR: f64 = 1.0;
const DEFAULT_GO2010_ION_ENHANCED_FN_ENABLED: bool = false;
const DEFAULT_GO2010_K: f64 = 1.0e7;
const FOWLER_NORDHEIM_A: f64 = 1.541_434e-6;
const FOWLER_NORDHEIM_B: f64 = 6.830_890e9;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SurfaceConfig {
    pub secondary_electron_yield: f64,
    pub secondary_electron_energy_ev: f64,
    pub electron_reflection_probability: f64,
    pub fn_emission_enabled: bool,
    pub fn_work_function_ev: f64,
    pub fn_field_enhancement: f64,
    pub fn_emission_area_factor: f64,
    pub go2010_ion_enhanced_fn_enabled: bool,
    pub go2010_k: f64,
}

impl Default for SurfaceConfig {
    fn default() -> Self {
        Self {
            secondary_electron_yield: DEFAULT_SECONDARY_ELECTRON_YIELD,
            secondary_electron_energy_ev: DEFAULT_SECONDARY_ELECTRON_ENERGY_EV,
            electron_reflection_probability: DEFAULT_ELECTRON_REFLECTION_PROBABILITY,
            fn_emission_enabled: true,
            fn_work_function_ev: DEFAULT_FN_WORK_FUNCTION_EV,
            fn_field_enhancement: DEFAULT_FN_FIELD_ENHANCEMENT,
            fn_emission_area_factor: DEFAULT_FN_EMISSION_AREA_FACTOR,
            go2010_ion_enhanced_fn_enabled: DEFAULT_GO2010_ION_ENHANCED_FN_ENABLED,
            go2010_k: DEFAULT_GO2010_K,
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct SurfaceState {
    fn_remainder_powered: f64,
    fn_remainder_grounded: f64,
}

#[derive(Debug, Default)]
pub(crate) struct SurfaceStats {
    pub secondary_emitted_powered: u64,
    pub secondary_emitted_grounded: u64,
    pub fn_emitted_powered: u64,
    pub fn_emitted_grounded: u64,
    pub ion_enhanced_fn_emitted_powered: u64,
    pub ion_enhanced_fn_emitted_grounded: u64,
    pub electron_reflected_powered: u64,
    pub electron_reflected_grounded: u64,
}

#[derive(Clone, Copy, Debug)]
enum Electrode {
    Powered,
    Grounded,
}

impl Electrode {
    fn emission_position(self) -> f64 {
        let config = one_d_sim_config();
        match self {
            Electrode::Powered => 0.5 * config.dx(),
            Electrode::Grounded => config.gap_m - 0.5 * config.dx(),
        }
    }

    fn inward_velocity_sign(self) -> f64 {
        match self {
            Electrode::Powered => 1.0,
            Electrode::Grounded => -1.0,
        }
    }

    fn field_pulling_electrons_from_wall(self, efield: &[f64]) -> f64 {
        match self {
            Electrode::Powered => (-efield[0]).max(0.0),
            Electrode::Grounded => efield[efield.len() - 1].max(0.0),
        }
    }
}

pub(crate) fn apply_fn_emission(
    efield: &[f64],
    electrons: &mut Vec<ParticleType>,
    config: &SurfaceConfig,
    state: &mut SurfaceState,
    stats: &mut SurfaceStats,
    rng: &mut ThreadRng,
) {
    if !config.fn_emission_enabled {
        return;
    }

    emit_fn_electrons(
        Electrode::Powered,
        efield,
        electrons,
        config,
        &mut state.fn_remainder_powered,
        &mut stats.fn_emitted_powered,
        rng,
    );
    emit_fn_electrons(
        Electrode::Grounded,
        efield,
        electrons,
        config,
        &mut state.fn_remainder_grounded,
        &mut stats.fn_emitted_grounded,
        rng,
    );
}

pub(crate) fn check_electron_boundaries(
    electrons: &mut Vec<ParticleType>,
    abs_powered: &mut u64,
    abs_grounded: &mut u64,
    measurement: bool,
    fed_powered: &mut Vec<u64>,
    fed_grounded: &mut Vec<u64>,
    config: &SurfaceConfig,
    stats: &mut SurfaceStats,
    rng: &mut ThreadRng,
) {
    let config_1d = one_d_sim_config();
    let reflect_probability = config.electron_reflection_probability.max(0.0).min(1.0);
    let mut ind: usize = 0;
    while ind < electrons.len() {
        let electrode = if electrons[ind].x < 0.0 {
            Some(Electrode::Powered)
        } else if electrons[ind].x > config_1d.gap_m {
            Some(Electrode::Grounded)
        } else {
            None
        };

        if let Some(electrode) = electrode {
            if rng.gen::<f64>() < reflect_probability {
                reflect_electron(electrode, &mut electrons[ind]);
                add_electrode_count(
                    electrode,
                    1,
                    &mut stats.electron_reflected_powered,
                    &mut stats.electron_reflected_grounded,
                );
                ind += 1;
            } else {
                match electrode {
                    Electrode::Powered => {
                        *abs_powered += 1;
                        if measurement {
                            record_flux_energy(&electrons[ind], E_MASS, fed_powered);
                        }
                    }
                    Electrode::Grounded => {
                        *abs_grounded += 1;
                        if measurement {
                            record_flux_energy(&electrons[ind], E_MASS, fed_grounded);
                        }
                    }
                }
                electrons.swap_remove(ind);
            }
        } else {
            ind += 1;
        }
    }
}

pub(crate) fn check_ion_boundaries(
    ions: &mut Vec<ParticleType>,
    electrons: &mut Vec<ParticleType>,
    efield: &[f64],
    abs_powered: &mut u64,
    abs_grounded: &mut u64,
    measurement: bool,
    fed_powered: &mut Vec<u64>,
    fed_grounded: &mut Vec<u64>,
    adf_powered: &mut Vec<u64>,
    adf_grounded: &mut Vec<u64>,
    adf2d_powered: &mut Vec<Vec<u64>>,
    adf2d_grounded: &mut Vec<Vec<u64>>,
    config: &SurfaceConfig,
    stats: &mut SurfaceStats,
    rng: &mut ThreadRng,
) {
    absorb_wall_hits(
        ions,
        abs_powered,
        abs_grounded,
        measurement,
        fed_powered,
        fed_grounded,
        adf_powered,
        adf_grounded,
        adf2d_powered,
        adf2d_grounded,
        one_d_sim_config().ion_mass_kg(),
        |electrode| {
            emit_secondary_electrons(electrode, efield, electrons, config, stats, rng);
        },
    );
}

pub(crate) fn check_negative_ion_boundaries(
    negative_ions: &mut Vec<ParticleType>,
    abs_powered: &mut u64,
    abs_grounded: &mut u64,
) {
    let config = one_d_sim_config();
    let mut ind: usize = 0;
    while ind < negative_ions.len() {
        if negative_ions[ind].x < 0.0 {
            *abs_powered += 1;
            negative_ions.swap_remove(ind);
        } else if negative_ions[ind].x > config.gap_m {
            *abs_grounded += 1;
            negative_ions.swap_remove(ind);
        } else {
            ind += 1;
        }
    }
}

fn absorb_wall_hits<F>(
    particles: &mut Vec<ParticleType>,
    abs_powered: &mut u64,
    abs_grounded: &mut u64,
    measurement: bool,
    fed_powered: &mut Vec<u64>,
    fed_grounded: &mut Vec<u64>,
    adf_powered: &mut Vec<u64>,
    adf_grounded: &mut Vec<u64>,
    adf2d_powered: &mut Vec<Vec<u64>>,
    adf2d_grounded: &mut Vec<Vec<u64>>,
    mass: f64,
    mut on_absorbed: F,
) where
    F: FnMut(Electrode),
{
    let config = one_d_sim_config();
    let mut ind: usize = 0;
    while ind < particles.len() {
        let electrode = if particles[ind].x < 0.0 {
            *abs_powered += 1;
            if measurement {
                record_flux_energy(&particles[ind], mass, fed_powered);
                record_flux_angle(&particles[ind], adf_powered);
                record_flux_angle_2d(&particles[ind], adf2d_powered);
            }
            Some(Electrode::Powered)
        } else if particles[ind].x > config.gap_m {
            *abs_grounded += 1;
            if measurement {
                record_flux_energy(&particles[ind], mass, fed_grounded);
                record_flux_angle(&particles[ind], adf_grounded);
                record_flux_angle_2d(&particles[ind], adf2d_grounded);
            }
            Some(Electrode::Grounded)
        } else {
            None
        };

        if let Some(electrode) = electrode {
            on_absorbed(electrode);
            particles.swap_remove(ind);
        } else {
            ind += 1;
        }
    }
}

fn emit_secondary_electrons(
    electrode: Electrode,
    efield: &[f64],
    electrons: &mut Vec<ParticleType>,
    config: &SurfaceConfig,
    stats: &mut SurfaceStats,
    rng: &mut ThreadRng,
) {
    let room = one_d_sim_config()
        .max_particles
        .saturating_sub(electrons.len());
    let secondary_emitted =
        stochastic_count(config.secondary_electron_yield.max(0.0), rng).min(room);

    let mut secondary_actual = 0;
    for _ in 0..secondary_emitted {
        emit_wall_electron(
            electrode,
            electrons,
            config.secondary_electron_energy_ev,
            rng,
        );
        secondary_actual += 1;
    }
    add_electrode_count(
        electrode,
        secondary_actual as u64,
        &mut stats.secondary_emitted_powered,
        &mut stats.secondary_emitted_grounded,
    );

    if config.go2010_ion_enhanced_fn_enabled {
        let room = one_d_sim_config()
            .max_particles
            .saturating_sub(electrons.len());
        let ion_enhanced_yield = go2010_ion_enhanced_fn_yield(
            electrode.field_pulling_electrons_from_wall(efield),
            config,
        );
        let ion_enhanced_emitted = stochastic_count(ion_enhanced_yield, rng).min(room);
        let mut ion_enhanced_actual = 0;
        for _ in 0..ion_enhanced_emitted {
            emit_wall_electron(
                electrode,
                electrons,
                config.secondary_electron_energy_ev,
                rng,
            );
            ion_enhanced_actual += 1;
        }
        add_electrode_count(
            electrode,
            ion_enhanced_actual as u64,
            &mut stats.ion_enhanced_fn_emitted_powered,
            &mut stats.ion_enhanced_fn_emitted_grounded,
        );
    }
}

fn emit_fn_electrons(
    electrode: Electrode,
    efield: &[f64],
    electrons: &mut Vec<ParticleType>,
    config: &SurfaceConfig,
    remainder: &mut f64,
    emitted_total: &mut u64,
    rng: &mut ThreadRng,
) {
    let normal_field = electrode.field_pulling_electrons_from_wall(efield);
    let current_density = fowler_nordheim_current_density(normal_field, config);
    if current_density <= 0.0 {
        return;
    }

    let config_1d = one_d_sim_config();
    let area = config_1d.electrode_area_m2 * config.fn_emission_area_factor.max(0.0);
    let expected = current_density * area * config_1d.dt_e() / (E_CHARGE * config_1d.weight);
    if !expected.is_finite() || expected <= 0.0 {
        return;
    }

    *remainder += expected;
    let room = config_1d.max_particles.saturating_sub(electrons.len());
    let emitted = (remainder.floor() as usize).min(room);
    *remainder -= emitted as f64;

    for _ in 0..emitted {
        emit_wall_electron(
            electrode,
            electrons,
            config.secondary_electron_energy_ev,
            rng,
        );
    }
    *emitted_total += emitted as u64;
}

fn emit_wall_electron(
    electrode: Electrode,
    electrons: &mut Vec<ParticleType>,
    energy_ev: f64,
    rng: &mut ThreadRng,
) {
    let speed = (2.0 * energy_ev.max(0.0) * EV_TO_J / E_MASS).sqrt();
    let normal_component = rng.gen::<f64>();
    let tangential_component = (1.0 - normal_component * normal_component).sqrt();
    let azimuth = TWO_PI * rng.gen::<f64>();

    electrons.push(ParticleType {
        x: electrode.emission_position(),
        vx: electrode.inward_velocity_sign() * speed * normal_component,
        vy: speed * tangential_component * azimuth.cos(),
        vz: speed * tangential_component * azimuth.sin(),
    });
}

fn reflect_electron(electrode: Electrode, electron: &mut ParticleType) {
    let config = one_d_sim_config();
    electron.x = match electrode {
        Electrode::Powered => (-electron.x).max(0.0).min(config.gap_m),
        Electrode::Grounded => (2.0 * config.gap_m - electron.x).max(0.0).min(config.gap_m),
    };
    electron.vx = -electron.vx;
}

fn record_flux_energy(particle: &ParticleType, mass: f64, fed: &mut Vec<u64>) {
    let v2 = particle.vx.powf(2.0) + particle.vy.powf(2.0) + particle.vz.powf(2.0);
    let energy = 0.5 * mass * v2 / EV_TO_J;
    let energy_index = (energy / DE_FED + 0.5).trunc() as usize;
    if energy_index < N_FED {
        fed[energy_index] += 1;
    }
}

fn record_flux_angle(particle: &ParticleType, adf: &mut Vec<u64>) {
    let config = one_d_sim_config();
    let vt = (particle.vy.powf(2.0) + particle.vz.powf(2.0)).sqrt();
    let angle_deg = vt.atan2(particle.vx.abs()).to_degrees();
    let angle_index = (angle_deg / config.da_adf()).trunc() as usize;
    if angle_index < config.n_adf {
        adf[angle_index] += 1;
    }
}

fn record_flux_angle_2d(particle: &ParticleType, adf2d: &mut Vec<Vec<u64>>) {
    let config = one_d_sim_config();
    let da = config.da_2adf();
    let n = config.n_2adf;
    let theta_y = particle.vy.atan2(particle.vx.abs()).to_degrees();
    let theta_z = particle.vz.atan2(particle.vx.abs()).to_degrees();
    let iy = ((theta_y + 90.0) / da).trunc() as usize;
    let iz = ((theta_z + 90.0) / da).trunc() as usize;
    if iy < n && iz < n {
        adf2d[iy][iz] += 1;
    }
}

pub(crate) fn fowler_nordheim_current_density(normal_field: f64, config: &SurfaceConfig) -> f64 {
    let effective_field = normal_field * config.fn_field_enhancement.max(0.0);
    let work_function = config.fn_work_function_ev;
    if effective_field <= 0.0 || work_function <= 0.0 {
        return 0.0;
    }

    let exponent = -FOWLER_NORDHEIM_B * work_function.powf(1.5) / effective_field;
    if exponent < -745.0 {
        return 0.0;
    }

    let current_density =
        FOWLER_NORDHEIM_A * effective_field.powf(2.0) / work_function * exponent.exp();
    if current_density.is_finite() {
        current_density
    } else {
        0.0
    }
}

pub(crate) fn go2010_ion_enhanced_fn_yield(normal_field: f64, config: &SurfaceConfig) -> f64 {
    if normal_field <= 0.0 || config.go2010_k <= 0.0 || config.fn_work_function_ev <= 0.0 {
        return 0.0;
    }
    let beta = config.fn_field_enhancement.max(0.0);
    if beta <= 0.0 {
        return 0.0;
    }

    let exponent =
        -FOWLER_NORDHEIM_B * config.fn_work_function_ev.powf(1.5) / (beta * normal_field);
    if exponent < -745.0 {
        return 0.0;
    }
    let yield_per_ion = config.go2010_k * exponent.exp();
    if yield_per_ion.is_finite() {
        yield_per_ion
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

fn add_electrode_count(
    electrode: Electrode,
    emitted: u64,
    powered_count: &mut u64,
    grounded_count: &mut u64,
) {
    match electrode {
        Electrode::Powered => *powered_count += emitted,
        Electrode::Grounded => *grounded_count += emitted,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wall_field_direction_uses_inward_electron_force() {
        let config = one_d_sim_config();
        let mut efield = vec![0.0; config.n_grid];
        efield[0] = -10.0;
        efield[config.n_grid - 1] = 20.0;

        assert_eq!(
            Electrode::Powered.field_pulling_electrons_from_wall(&efield),
            10.0
        );
        assert_eq!(
            Electrode::Grounded.field_pulling_electrons_from_wall(&efield),
            20.0
        );
    }

    #[test]
    fn fowler_nordheim_current_is_zero_for_unpulling_field() {
        let config = SurfaceConfig::default();
        assert_eq!(fowler_nordheim_current_density(0.0, &config), 0.0);
    }

    #[test]
    fn reflected_electron_keeps_speed_and_returns_inside_gap() {
        let config = one_d_sim_config();
        let mut electron = ParticleType {
            x: -0.1 * config.dx(),
            vx: -12.0,
            vy: 3.0,
            vz: -4.0,
        };

        reflect_electron(Electrode::Powered, &mut electron);

        assert!(electron.x >= 0.0);
        assert!(electron.x <= config.gap_m);
        assert_eq!(electron.vx, 12.0);
        assert_eq!(electron.vy, 3.0);
        assert_eq!(electron.vz, -4.0);
    }
}
