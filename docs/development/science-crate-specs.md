# Science Crate Scaffolding Specs

> Paste the relevant section into a fresh Claude session in each repo directory.
> All directories already created with full structure (src/, tests/, benches/, examples/, scripts/, docs/, .github/workflows/).
> Tell the agent: "Scaffold this crate. Use the P(-1) development loop from CLAUDE.md. All implementations must use REAL physics — no todo!() or unimplemented!()."

---

## Common Pattern (all crates)

- **Edition**: 2024 | **MSRV**: 1.89 | **License**: GPL-3.0
- **Core deps**: hisab = "0.24", serde = { version = "1", features = ["derive"] }, thiserror = "2", tracing = "0.1"
- **Optional**: tracing-subscriber = { version = "0.3", features = ["env-filter", "fmt"], optional = true }
- **Dev deps**: criterion = { version = "0.5", features = ["html_reports"] }, serde_json = "1"
- **Features**: default = [], logging = ["dep:tracing-subscriber"]
- **Files needed**: Cargo.toml, VERSION (0.1.0), src/lib.rs, src/error.rs, src/*.rs (domain modules), tests/integration.rs, benches/benchmarks.rs, examples/basic.rs, README.md, CHANGELOG.md, CLAUDE.md (P(-1) loop + dev loop + key principles + DO NOTs), CONTRIBUTING.md, SECURITY.md, CODE_OF_CONDUCT.md, LICENSE, Makefile (check/fmt/clippy/test/audit/deny/bench/coverage/build/doc/clean), scripts/bench-history.sh, scripts/version-bump.sh, .github/workflows/ci.yml (7 jobs: check, security, deny, test×2, msrv, coverage, doc), .github/workflows/release.yml (VERSION/Cargo.toml/tag match, crates.io publish, GitHub release), rust-toolchain.toml, deny.toml, codecov.yml, .gitignore, docs/architecture/overview.md, docs/development/roadmap.md
- **Quality**: #[non_exhaustive] on enums, #[must_use] on pure fns, #[inline] on hot paths, zero unwrap/panic in library, real physics with real tests

---

## 1. goonj (Hindi/Urdu: echo, resonance) — Acoustics

**Domain**: Sound propagation, room simulation, wave physics, impulse responses

**Modules**:
- `error.rs` — GoonjError: InvalidGeometry, InvalidMaterial, InvalidFrequency, PropagationFailed, ComputationError
- `material.rs` — AcousticMaterial { absorption_coefficients: [f32; 6] (125Hz–4kHz), scattering }. Presets: concrete, carpet, glass, wood, curtain, drywall, tile
- `propagation.rs` — speed_of_sound(temp) = 331.3 + 0.606*T, inverse_square_law, atmospheric_absorption, doppler_shift
- `room.rs` — Wall, RoomGeometry, AcousticRoom. shoebox() constructor. volume(), surface_area()
- `impulse.rs` — ImpulseResponse, sabine_rt60 = 0.161*V/A, eyring_rt60, energy_decay_curve
- `ray.rs` — AcousticRay, RayHit, reflect_ray (specular + absorption), ray_wall_intersection
- `diffraction.rs` — occlusion_check, edge_diffraction_loss (simplified UTD)
- `resonance.rs` — room_modes, axial_modes, schroeder_frequency = 2000*√(RT60/V)

**Key tests**: speed at 20°C ≈ 343.4 m/s, shoebox RT60 matches Sabine, 5m room first mode ≈ 34.3 Hz
**Consumers**: dhvani (impulse responses for convolution reverb), shruti (room simulation), kiran/joshua (game audio), aethersafha (spatial audio)

---

## 2. pavan (Sanskrit: wind) — Aerodynamics

**Domain**: Lift, drag, airflow, airfoils, boundary layers, atmosphere

**Modules**:
- `error.rs` — PavanError: InvalidAngle, InvalidAltitude, InvalidVelocity, InvalidGeometry, ComputationError
- `atmosphere.rs` — ISA model: T = 288.15 - 0.0065*h, barometric pressure, density from ideal gas. SEA_LEVEL_TEMP=288.15, P=101325, ρ=1.225. dynamic_pressure = 0.5*ρ*V², mach_number, speed_of_sound = √(1.4*287.058*T)
- `airfoil.rs` — NACA 4-digit generation: camber, thickness distribution yt = 5t*(0.2969√x - 0.1260x - 0.3516x² + 0.2843x³ - 0.1015x⁴)
- `coefficients.rs` — Cl = 2π*sin(α) (thin airfoil), Cd = Cd0 + Cl²/(π*e*AR), moment coefficient
- `forces.rs` — lift, drag from q*S*C, Reynolds = ρVL/μ, AeroForce struct
- `boundary.rs` — Blasius thickness = 5x/√Re, transition Re ≈ 500,000, turbulent thickness
- `wind.rs` — WindField, logarithmic wind profile, WindGust
- `vehicle.rs` — AeroBody, compute net aerodynamic force vector

**Key tests**: sea level ρ = 1.225, Cl at 5° ≈ 0.548, Earth escape velocity (cross-check with falak)
**Consumers**: kiran/joshua (flight, projectiles, wind), impetus (aero forces), badal (atmospheric feed)
**Optional dep**: pravash (cfd feature for CFD coupling)

---

## 3. dravya (Sanskrit: substance) — Material Science

**Domain**: Stress, strain, elasticity, fracture, deformation, fatigue

**Modules**:
- `error.rs` — DravyaError: InvalidMaterial, InvalidStress, InvalidStrain, YieldExceeded, ComputationError
- `material.rs` — Material { youngs_modulus, poisson_ratio, yield_strength, density, thermal_expansion }. Presets: steel(200e9,0.30,250e6,7850), aluminum(69e9,0.33,276e6,2700), copper, titanium, glass, rubber, concrete, wood_oak, carbon_fiber
- `stress.rs` — StressTensor [f64;6] symmetric, principal_stresses, von_mises = √((σ1-σ2)²+(σ2-σ3)²+(σ1-σ3)²)/√2, max_shear, hydrostatic
- `strain.rs` — StrainTensor, engineering_strain = (L-L0)/L0, true_strain = ln(L/L0), volumetric
- `elastic.rs` — Hooke σ=Eε, bulk_modulus = E/(3(1-2ν)), shear_modulus = E/(2(1+ν)), Lamé parameters
- `yield_criteria.rs` — von_mises_check, tresca_check, safety_factor
- `beam.rs` — cantilever_deflection = FL³/(3EI), simply_supported = FL³/(48EI), bending_stress = My/I, moment_of_inertia (rect, circle, I-beam)
- `fatigue.rs` — Basquin's law, Miner's rule cumulative damage, endurance_limit ≈ 0.5*σ_ultimate

**Key tests**: uniaxial von_mises = applied stress, steel bulk modulus ≈ 167 GPa
**Consumers**: impetus (collision materials), soorat (PBR from physical props), kiran/joshua (destructible environments), ushma (thermal expansion)

---

## 4. kimiya (Arabic: alchemy) — Chemistry

**Domain**: Elements, molecules, reactions, kinetics, gas laws, solutions

**Modules**:
- `error.rs` — KimiyaError: InvalidElement, InvalidReaction, InvalidConcentration, InvalidTemperature, ComputationError
- `element.rs` — Element { atomic_number, symbol, name, atomic_mass, electronegativity, category }. First 36 elements with REAL data (H=1.008 through Kr=83.80). ElementCategory enum
- `molecule.rs` — Molecule { atoms }, molecular_weight, Bond enum, formula_string. H2O=18.015, CO2=44.01
- `reaction.rs` — Reaction, is_balanced, gibbs_free_energy ΔG=ΔH-TΔS, equilibrium_constant K=exp(-ΔG/RT)
- `kinetics.rs` — arrhenius k=A*exp(-Ea/RT), half_life = ln2/k. R=8.314 J/(mol·K)
- `gas.rs` — ideal_gas PV=nRT, van_der_waals, partial_pressure, gas_density = PM/(RT). 1 mol at STP ≈ 22.4 L
- `solution.rs` — molarity, molality, dilution M1V1=M2V2, pH=-log10([H+]), Henderson-Hasselbalch
- `thermo.rs` — q=mcΔT, Hess's law, WATER_SPECIFIC_HEAT=4.184 J/(g·°C). 1kg water +1°C = 4184 J

**Key tests**: water MW=18.015, pure water pH≈7, ideal gas at STP, Avogadro=6.022e23
**Consumers**: ushma (thermochemistry), bijli (electrochemistry), joshua (chemistry sim), dravya (material composition)

---

## 5. falak (Arabic/Persian: celestial sphere) — Orbital Mechanics

**Domain**: N-body simulation, Keplerian orbits, transfers, tidal forces

**Modules**:
- `error.rs` — FalakError: InvalidOrbit, InvalidBody, InvalidTime, IntegrationFailed, ComputationError
- `constants.rs` — G=6.674e-11, AU=1.496e11, SOLAR_MASS=1.989e30, EARTH_MASS=5.972e24, EARTH_MU=3.986e14, SUN_MU=1.327e20, planetary masses
- `kepler.rs` — OrbitalElements { a, e, i, Ω, ω, ν }, period T=2π√(a³/μ), vis-viva v²=μ(2/r-1/a), circular_velocity=√(μ/r)
- `nbody.rs` — Body { mass, position, velocity }, gravitational_acceleration, Verlet integration, RK4 integration
- `transfer.rs` — Hohmann Δv, transfer time, escape_velocity=√(2μ/r)
- `tidal.rs` — tidal_acceleration=2GMd/r³, Roche limit
- `time.rs` — JulianDate, julian_day_number, j2000_centuries. J2000.0 = JD 2451545.0

**Key tests**: Earth period ≈ 365.25 days, escape velocity ≈ 11.2 km/s, energy conservation over 1000 Verlet steps
**Consumers**: jyotish (planetary positions), joshua (space sim), kiran (space games), bhava v3 (galactic dynamics)

---

## 6. badal (Hindi/Urdu: cloud) — Weather / Atmospheric

**Domain**: Pressure systems, moisture, clouds, wind, atmospheric stability

**Modules**:
- `error.rs` — BadalError: InvalidTemperature, InvalidPressure, InvalidHumidity, InvalidAltitude, ComputationError
- `atmosphere.rs` — AtmosphericState, ISA T=288.15-0.0065h, barometric pressure, density=P/(287.058*T), dew_point
- `pressure.rs` — barometric_formula, pressure_gradient_force, geostrophic_wind, sea_level_correction
- `moisture.rs` — saturation_vapor_pressure (Magnus-Tetens: 611.2*exp(17.67T/(T+243.5))), mixing_ratio, relative_humidity, wet_bulb, heat_index
- `cloud.rs` — CloudType enum (10 types), cloud_base_altitude = (T-Td)/8*1000, lifting_condensation_level
- `wind.rs` — Ω=7.292e-5, coriolis f=2Ω*sin(φ), wind_chill (NWS formula), beaufort_scale (0-12)
- `stability.rs` — DRY_ADIABATIC=9.8°C/km, MOIST≈6°C/km, StabilityClass, CAPE, lifted_index

**Key tests**: sea level 288.15K/101325Pa/1.225ρ, saturation VP at 20°C ≈ 2338 Pa, coriolis at 45° ≈ 1.03e-4
**Consumers**: kiran/joshua (game weather), bhava 1.5 (planetary conditions → personality), pavan (atmospheric feed), goonj (temperature → speed of sound)
**Optional deps**: ushma (thermo feature), pravash (fluids feature)

---

## 7. jyotish (Sanskrit: light/astrology) — Computational Astrology

**Domain**: Zodiac, planets, houses, aspects, nakshatras, natal charts

**Modules**:
- `error.rs` — JyotishError: InvalidDegree, InvalidDate, InvalidLocation, InvalidPlanet, ComputationError
- `zodiac.rs` — ZodiacSign (12), Element (Fire/Earth/Air/Water), Modality (Cardinal/Fixed/Mutable), from_longitude (0-30°=Aries, etc.), ruling_planet
- `planet.rs` — Planet enum (Sun through Chiron, 13 total), PlanetaryPosition { longitude, latitude, speed, retrograde }
- `house.rs` — HouseSystem (WholeSign/Equal/Placidus), calculate_houses, house_of_planet
- `aspect.rs` — AspectKind (Conjunction 0°, Sextile 60°, Square 90°, Trine 120°, Opposition 180°, Quincunx 150°), find_aspects with orbs (conj±8°, trine±8°, square±7°, sextile±6°, quincunx±3°)
- `nakshatra.rs` — 27 Nakshatras (Ashwini–Revati), 13.333° each, Guna (Sattva/Rajas/Tamas), Motivation (Dharma/Artha/Kama/Moksha)
- `chart.rs` — NatalChart, ChartInput { year, month, day, hour, lat, lon }, from_positions()
- `dignity.rs` — Domicile/Exaltation/Detriment/Fall/Peregrine, lookup table (Sun rules Leo, exalted Aries 19°, etc.)

**Key tests**: 45°=Taurus, 0°=Ashwini, 14°=Bharani, Sun domicile Leo, 0° and 120° are trine
**Consumers**: bhava v2.0 (zodiac manifestation), joshua (NPC natal charts), kiran (procedural characters)
**Optional dep**: falak (orbital feature for precise planetary positions)

---

## 8. tara (Sanskrit: star) — Stellar Catalog & Galactic Structure

**Domain**: Fixed stars, coordinate transforms, precession, constellations, galactic structure

**Modules**:
- `error.rs` — TaraError: InvalidCoordinate, InvalidMagnitude, InvalidEpoch, StarNotFound, ComputationError
- `star.rs` — FixedStar { name, constellation, ra_deg, dec_deg, magnitude, spectral_type, ecliptic_lon, ecliptic_lat }. 30-star catalog with REAL data: Sirius(RA=101.29°,Dec=-16.72°,mag=-1.46), Vega(279.23°,38.78°,0.03), Polaris(37.95°,89.26°,1.98), etc.
- `coords.rs` — OBLIQUITY_J2000=23.4393°, equatorial↔ecliptic, equatorial↔galactic transforms. GALACTIC_NORTH: RA=192.85°, Dec=27.13°
- `precession.rs` — 50.29 arcsec/year, precess_equatorial, precess_ecliptic_longitude. 72 years ≈ 1°
- `constellation.rs` — 88 IAU Constellation enum, Display impl
- `galaxy.rs` — SOLAR_DISTANCE_FROM_CENTER=26,000 ly, MILKY_WAY_DIAMETER=100,000 ly, GalacticPosition
- `magnitude.rs` — apparent_to_absolute M=m-5log10(d/10), Pogson ratio 2.512, luminosity. 5 mag diff = 100x brightness

**Key tests**: coordinate roundtrips, Sirius ecliptic lon ≈ 104°, 72yr precession ≈ 1°, Pogson 5 mag = 100x
**Consumers**: jyotish (fixed star positions), bhava v2/v3 (star archetypes, galactic scales), joshua (star maps), soorat (star rendering)
