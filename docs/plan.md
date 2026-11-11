# Plan: 2nd-generation units of measurement in Rust

## Context

The Raku `Physics::Unit` / `Physics::Measure` pair (`measures/raku-Physics-Unit`, `measures/raku-Physics-Measure`) works but carries structure that exists only because Raku is slow: a per-parse regenerated grammar, ~12 denormalised registry tables kept in sync by hand, a pre-computed prefix x unit "postfix" cross product, a `dmix` shadow of the dimension vector, a "shortest name" substitution heuristic, lazy mutate-then-finalize construction, and ~50 hard-coded subclasses (`Length`, `Force`, ...) with types inferred from dimensions via global hint tables.

omasheet (Rust, edition 2024, 6-crate workspace) has **no units code**: only a deferred OpenSpec change `openspec/changes/add-units/` (units as integer exponent vectors, dimension mismatch is a type error, bare cells take the column unit, mixed-unit cells convert to the column unit). Its uncertainty work is finished: `Value::Scientific(f64, f64)` = value + 1-sigma. The new crate is the engine for that proposal and must stay free of Qt/polars/omasheet so omasheet can depend on it.

Error/uncertainty (Raku `Physics::Error`), Currency, Constants and Navigation are out of scope.

## Scope rules (from the user)

- **Cell text is tightly controlled.** Live in cells: `XX` immediately followed by an SI unit, e.g. `5kg`, `300cm`, `9.81m.s^-2`. `XX` is exactly an omasheet Scientific literal (so `10.5(3)e-3kg` carries uncertainty). Magnitude is `f64` + sigma; no exact-Decimal path (`5m + 300cm = 8m` holds in f64 anyway).
- **Compound notation is only** `kg.m^2.s^-2`: `.` separator, `^` with a signed integer exponent, no spaces, no `/`, no parentheses, no superscripts, no trailing-digit exponents (`m2`), no names/plurals/aliases.
- **Non-SI units are not cell syntax.** They are chosen at column level (unit picker); the column type is the point of definition of the subtype.
- Angles `45°30′30″` are Stage 3, Time is Stage 4.

## Design

**Dimension.** `Dim([i8; 10])`, slots `L M T I Θ N J A $ D` with indices fixed in code (enum), not inferred from YAML order. `A` used from Stage 3, `D` from Stage 2, `$` reserved for currency.

**Unit.** Immutable canonical term list `Vec<Term { prefix: Option<PrefixId>, unit: UnitId, exp: i16 }>` (canonical order), with `dims`, `factor: f64`, `offset: f64` derived once at construction. Replaces `dmix`; compatibility is `dims` equality only. No name-simplification (an optional `simplify()` using a dims -> coherent-named-unit index can come later). Formatter emits the same grammar the parser accepts, so `parse(format(u)) == u`.

**Registry.** Built once (`OnceLock`) from YAML embedded with `include_str!`, immutable, fully normalised: `dimensions`, `prefixes`, `units`, `quantity_types` are separate tables; `symbol -> id` is one map. Units are defined by a `defn` string over earlier units (resolved in dependency order, cycle-checked). Factors are parsed from decimal strings in YAML (correctly rounded f64), prefixes are `{symbol, name, base, exp}`.

**No prefix x unit cross product.** A unit declares `prefixable: none | si | binary | both`. Lookup: exact unit symbol first, else longest prefix + prefixable unit. The mass unit is defined as prefixable `g` with factor 1e-3 relative to the coherent base, so `kg` = `k` + `g` with no special-casing. A test enumerates all prefix x unit combinations in the live set and fails on any ambiguous symbol.

**One parser, two scopes.** Hand-written tokenizer, strictly the grammar above, parameterised by symbol scope: `Scope::Live` (cell text: SI units only) and `Scope::Any` (column-unit setting / file format: any registry unit, still same notation). Names are used only for picker labels. Typographic alternates are explicit `alt` symbols in YAML (e.g. `μ`/`µ`, `Ω`/`Ω`); angle alternates (`º`, `'`, `"`) arrive in Stage 3.

**Affine units** (`offset` in schema). `°C` (alt `ºC`) is a core live unit alongside K: `20°C` is valid cell text. An affine unit is legal only as a whole unit, never inside a compound (`°C.s^-1`, `°C^2` rejected), and is not prefixable. `°F` and other affine units are `Scope::Any` only (picker). Conversion applies the offset to the value but not to sigma. Arithmetic on affine quantities converts to the base scale first (`°C` -> K), so `°C - °C` yields a K difference and `°C * x` is computed in K; this keeps one rule and avoids delta-unit machinery for now.

**Errors** are a typed enum (parse error with span, unknown unit, ambiguous symbol, dimension mismatch, affine-in-compound).

**Quantity and column subtypes.**
- One generic runtime type: `Quantity { value: f64, sigma: f64, unit: Unit }`. Dimensions travel with `unit`.
- `Length`, `Force`, `Energy`, ... are **refinements of Quantity, not separate Rust types**: a `QuantityType` row in `resources/quantity_types.yaml` is just `name -> Dim` (plus an optional preferred display unit). `Length` = `Quantity` where `unit.dims == dims(Length)`; `QuantityType::admits(&Unit)` is the whole check, so every `Length` is a `Quantity` (subtype) but not vice versa.
- Several names share dims (Energy/Torque, Frequency/Radioactivity, Dose/SpecificEnergy). Since the **column type is the point of definition**, nothing is inferred from dims at runtime and there is no type-hint global. Arithmetic results are plain `Quantity`; a reverse dims -> names index exists only to suggest column types in the UI.
- Column-level unit: a column of type `Length` may set a display unit (picker lists registry units of matching dims, SI first, then non-SI). Bare numbers in such a column are interpreted in that unit; explicit SI-suffixed cells are converted to it on entry (matches the `add-units` spec). The omx syntax for declaring this is omasheet's decision (its open question Q11).
- `Quantity` ops in-crate (convert, add/sub with dimension check, mul/div/pow/root) use first-order independent quadrature for sigma, a few lines; omasheet-engine can later swap in its own `uncertain_arith` kernels.

**Host boundary.** The crate does not scan numbers: the host (omasheet lexer, which already lexes `10.5(3)e-3`) splits a cell into number and unit text, so exponent-`e` versus unit-prefix collisions (`1e3m` vs `1Em`) stay its concern. The crate API is `Unit::parse(text, Scope)`; tests use a trivial test-only splitter.

## Stages

### Stage 0 — scaffold
New standalone repo and crate `siquant` at `measures/siquant` (name free on crates.io as of 2026-11-11; no `rust-` prefix needed), edition 2024, crates.io-ready metadata (description, license, repository, keywords, categories, `include` for `resources/`), MIT, `// Copyright (c) 2026 Stephen Roe` + `//!` docs per file, `.gitignore` with `target/`. CI parity with omasheet: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test`. Pick a maintained YAML crate (serde_yaml is archived; verify `serde_yaml_ng` or similar at implementation time).

### Stage 1 — SI units + SI prefixes (live cell units only)
- `dim.rs`, `registry.rs`, `unit.rs`, `parse.rs`, `quantity.rs`, `fmt.rs`.
- `resources/dimensions.yaml`; `prefixes.yaml`: the 24 SI prefixes quetta..quecto plus `da`, as `{base: 10, exp}`.
- `resources/units.yaml` live set: SI base units m, g (kg), s, A, K, mol, cd; named derived units without angle: Hz N Pa J W C V F Ω S Wb T H Bq Gy Sv kat, plus `°C` (affine, `K + 273.15`, non-prefixable, alt `ºC`). Excluded for now: rad/sr/lm/lx (Stage 3) and all other non-SI units (picker, later).
- `resources/quantity_types.yaml`: names -> dims for the dimensions available here (Length, Mass, Current, Temperature, AmountOfSubstance, LuminousIntensity, Area, Volume, Speed, Acceleration, Frequency, Force, Pressure, Energy, Power, Charge, Potential, Capacitance, Resistance, Conductance, MagneticFlux, MagneticField, Inductance, ...), sourced from Raku `dims.yaml`. The type name `Time` itself is held back to Stage 4; the SI second `s` stays because most derived units and every rate compound need it.
- Tests: accept/reject table for the cell grammar (accept `5kg`, `300cm`, `9.81m.s^-2`, `5kg.m^2.s^-2`, `2kPa`; reject `5 kg`, `5km/s`, `5m2`, `5m s`, `5m^2.s^-1/kg`, `5ft`, `5L`, `5min`, `5°C.s^-1`, `5m.°C`, `5k°C`, plurals/names); accept `20°C` and `20ºC`; every unit def resolves; prefix-ambiguity validator over the live set; `a -> b -> a` conversion round trips including `°C` <-> K (offset on value, not sigma; Raku goldens `39°C` = `312.15K`); `°C - °C` gives K; `5m + 300cm == 8m`; dimension mismatch error; `QuantityType::admits`; goldens ported from Raku `U/t/02-dim` (derived-unit dims) and the SI-only cases of `M/t/05-cvt` and `06-com`.

### Stage 2 — data prefixes
Activate `D`. Add `b` (bit) and `B` (byte = 8 b) to the live set with `prefixable: both`; add binary prefixes Ki Mi Gi Ti Pi Ei Zi Yi Ri Qi to `prefixes.yaml` as `{base: 2, exp: 10n}` (schema unchanged). `kB` = 1000 B and `KiB` = 1024 B coexist; rates (`kb.s^-1`, `MiB.s^-1`) fall out with no hand-listed `kbps` entries. Extend the ambiguity validator to the binary prefixes (case-sensitive). Add `Information`/`DataRate` quantity types.

### Stage 3 — Angles
Activate `A`. Add rad, sr, lm (cd.sr), lx to the live set; define `°` (and ′ ″) as units with factor pi/180 etc. Add the DMS entry form `45°30′30″` for Angle columns, accepting alternates `º`, `'`, `"` and a leading minus; the numeric parts follow Scientific/decimal rules (detail to settle in that stage). Add `Angle`, `SolidAngle`, `AngularSpeed`, `LuminousFlux`, `Illuminance` types.

### Stage 4 — Time
Add the `Duration` quantity type (dims `T`, a refinement of Quantity like any other) and make it transparently interchangeable with omasheet's `Duration` (`Value::Duration(i64)` microseconds, ISO 8601 `PT1H30M`, `omasheet-omx/src/duration.rs`).
- **In siquant (no omasheet dependency):** `Quantity::from_micros(i64)` (value in `s`, sigma 0) and `Quantity::to_micros() -> Option<i64>` (any unit with dims `T`, e.g. `ms`, `min`, `h`; rounds to the nearest microsecond; `None` if sigma != 0, non-finite, or out of i64 range). Non-SI picker time units (min, h, d, wk) are added here.
- **In omasheet (later integration):** a `Duration` and a `Time`-typed value coerce both ways wherever one is expected (assign, compare, add/sub, `*` and `/` by a number), using those two functions; the `Duration` ISO literal and display stay the canonical forms, so the file format does not change. Exact integer microseconds are preserved on the Duration side; the Quantity side is f64 seconds, exact for |t| < ~285 years (2^53 µs) and rounded beyond that.
- **Dropped from the Raku design:** the `hh:mm:ss` entry/display form. omasheet already uses `09:00` for the time-of-day `Time` value, and ISO durations cover lengths of time, so the clock form would collide.
- **Naming (settled):** omasheet's `Time` stays the time of day on the clock. siquant has no type called `Time`; the dims-`T` quantity type is **`Duration`**, so no new name is introduced.
- **Uncertainty on Duration, without a new name:** a physical time measurement needs sigma and often sub-microsecond resolution (ns, ps), which the `i64` microsecond backing cannot hold. So `Duration` is one type with two representations on the same ladder omasheet already uses for numbers (exact -> Float -> Scientific): the compact **exact** form (`i64` µs, sigma 0 — today's Duration, unchanged, ISO literal and file format unchanged) and the **measured** form (the `Quantity` shape: `f64` seconds + `f64` sigma). There is no separate "Uncertainty" role or trait; `Quantity`'s `(value, sigma)` pair already has the shape of omasheet's `Scientific(f64, f64)`.
  - **Promote** to the measured form whenever sigma is attached (`1.5(2)s`, `PT30M ± 5s`), the value is sub-µs, or it is out of i64 µs range.
  - **Demote** to exact only when lossless (sigma == 0 and a whole number of µs); never silently round away sigma. `to_micros()` returns `None` otherwise, and an explicit `round`-style conversion is the only lossy path.
  - **Dates:** adding or subtracting a measured Duration to a Date/Datetime/clock `Time` is an error at first (dates carry no sigma), rather than silently dropping it; revisit when omasheet integrates.
  - **Display:** exact form keeps ISO (`PT1H30M`); measured form uses the Scientific concise form with an SI unit (`1.50(2)s`, `3.2(1)ns`).
  - **Coercion, following omasheet's existing conventions** (`Type(x)` / `x.Type` / `cast(x, Type)`, and `x.Float` dropping sigma from a `Scientific`):
    - *Implicit, lossless (up the ladder):* exact -> measured whenever sigma enters: `PT30M ± 5s`, `1.5(2)s`, or arithmetic mixing an exact and a measured Duration (`PT1H + 1.5(2)s`). Exact + exact stays exact.
    - *Explicit, lossy (down):* `x.Duration` / `Duration(x)` on a measured value rounds to the nearest microsecond and drops sigma, exactly as `x.Float` drops sigma from a `Scientific`. Never implicit.
    - *To plain numbers:* `total_seconds() total_minutes() total_hours() total_days()` give an exact number for an exact Duration and a `Scientific` (value and sigma, scaled by the unit) for a measured one.
    - *In siquant:* `Quantity::from_micros(i64)`, `Quantity::to_micros() -> Option<i64>` (None if sigma != 0, non-finite or out of range, so the host decides between error and explicit drop), and `Quantity::drop_sigma()`; the omasheet-side syntax above is omasheet's decision at integration.

### Later (not in this plan)
Non-SI unit data and picker API (`units_for(dims)`; port imperial/US from Raku `general.yaml`, fixing its defects: `cqm^2`, `cubic cm`, Unicode minus in `Jy`), affine picker units (°C/°F), `norm`/auto-prefix, optional `simplify()`, currency, constants, and the omasheet integration.

## Reference inputs (read-only)
- Raku data: `measures/raku-Physics-Unit/resources/Unit/Definitions/en_SI/{base,dims,derived,prefix,general}.yaml`
- Raku behaviour: `raku-Physics-Unit/t/{01-tst,02-dim}.rakutest`, `raku-Physics-Measure/t/{05-cvt,06-com}.rakutest`, `xt/{07-ugg,12-uax}`; DMS and clock parsing in `raku-Physics-Measure/lib/Physics/Measure.rakumod` (`defn-extract`)
- omasheet seams: `openspec/changes/add-units/{design.md,specs/units/spec.md}`; `omasheet-engine/src/value.rs` (`uncertain_arith`, `with_sigma`); lexer `omasheet-omx/src/lexer.rs:604-655`

## Verification
After each stage: `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test`. Do not port Raku golden values that are implementation accidents (`0.000567m/m^3`, `J/kg` for m²/s², `1unity` vs `1①`). To check any Raku reference value, run the Raku suite with `prove6`, not `raku`. Done when: Stage 1 — all SI prefixes x prefixable live units parse and convert, validator clean, reject table passes; Stage 2 — kB/KiB/Mb/MiB and rate cases pass and the validator stays clean; Stage 3/4 — DMS and `hh:mm:ss` round-trip.

## Assumptions to confirm
1. "Exclude time" until Stage 4 means the Duration quantity type and non-SI time units; the SI second `s` stays in Stage 1. omasheet's clock `Time` is untouched, `hh:mm:ss` is dropped, and Duration gains the measured (sigma) form as described in Stage 4.
2. Stage 1 cell grammar includes SI named derived units (`5N`, `2kPa`) and `°C` (standalone only). Arithmetic on `°C` goes through K, so `°C - °C` is a K difference; say if you want proper delta-temperature units instead.
2b. Stage 3 adds the angle `°`; `20°C` vs `45°` is resolved by longest match, so no conflict.
3. New standalone repo/crate `measures/siquant`, not inside the omasheet workspace.
4. The crate takes pre-split number/unit text from the host and never scans numbers itself.
