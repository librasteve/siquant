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

**Unit.** Immutable canonical term list `Vec<Term { prefix: Option<PrefixId>, unit: UnitId, exp: i16 }>` (canonical order), with `dims`, `factor: f64`, `offset: f64` derived once at construction. Replaces `dmix`; compatibility is `dims` equality only. No name-simplification (an optional `simplify()` using a dims -> coherent-named-unit index can come later). Formatter (`Display`, in `unit.rs`) emits the same grammar the parser accepts, so `parse(format(u)) == u`. Canonical term order: positive exponents first, named derived units before base units (`N.m`, not `m.N`), then base units in the order of `units.yaml` (`g m A K mol cd s`, time last), then no prefix before a prefix. A `Unit` holds a cheap `Registry` handle (an `Arc`), so it can print and compare itself.

**Registry.** Built once (`OnceLock`) from YAML embedded with `include_str!`, immutable, fully normalised: `prefixes`, `units`, `quantity_types` are separate tables (the base-dimension slots are only in code, `dim::Base`, so there is no second copy in a `dimensions.yaml`); `symbol -> id` is one map. A unit is defined by a `defn` string over units listed *earlier* in the file (a forward reference is a load error, so cycles cannot occur). Factors are parsed from decimal strings in YAML (correctly rounded f64), prefixes are `{symbol, name, base, exp}`.

**No prefix x unit cross product.** A unit declares `prefixable: none | si | binary | both`. Lookup: exact unit symbol first, else longest prefix + prefixable unit. The mass unit is defined as prefixable `g` with factor 1e-3 relative to the coherent base, so `kg` = `k` + `g` with no special-casing. The registry loader enumerates every prefix x unit combination (alternates included) and refuses to load if two read the same or one reads as a unit symbol, so a bare unit symbol never shadows a prefixed one; a test checks every combination reads back as itself.

**Scale (a bare prefix, decided after Stage 1 began).** A prefix symbol and a trailing `.` on their own, `k.`, is a dimensionless scale: `1k.` is 1000, `5m.` is 0.005, `1T.` is 1e12 (and `1Ki.` is 1024 once Stage 2 adds binary prefixes). The dot makes it a prefix and nothing else, so `m.` and `T.` do not clash with the metre and the tesla. Implementation: a hidden, symbol-less, dimensionless `unity` unit (id 0) that only this form reaches; the scale is `Term { prefix: Some(k), unit: unity, exp: 1 }`. It is a whole unit, like an affine one: `k.m`, `k.^2` and `kg.` are errors (`ScaleInCompound` or a syntax error). `Quantity` ops rebase a scale to a plain number first (`2k. * 3m` is `6000m`), and `Quantity::to_number()` gives `(value, sigma)` with the scale applied. Known trap, accepted: a stray `5m.` or `5T.` means milli or tera, not an error; the host should echo the parsed unit. omasheet's `#` (comment) and `_` (digit separator) ruled out other marker characters.

**One parser, two scopes.** Hand-written tokenizer, strictly the grammar above, parameterised by symbol scope: `Scope::Live` (cell text: SI units only) and `Scope::Any` (column-unit setting / file format: any registry unit, still same notation). Names are used only for picker labels. Typographic alternates are explicit `alt` symbols in YAML (e.g. `μ`/`µ`, `Ω`/`Ω`); angle alternates (`º`, `'`, `"`) arrive in Stage 3.

**Affine units** (`offset` in schema). `°C` (alt `ºC`) is a core live unit alongside K: `20°C` is valid cell text. An affine unit is legal only as a whole unit, never inside a compound (`°C.s^-1`, `°C^2` rejected), and is not prefixable. `°F` and other affine units are `Scope::Any` only (picker). Conversion applies the offset to the value but not to sigma. Arithmetic on affine quantities converts to the base scale first (`°C` -> K), so `°C * x` is computed in K. Sums are the exception, so a temperature stays in the unit it began in: `°C - °C` is an interval in `°C` (`10°C - 5°C` is `5°C`), and `°C ± K` is a temperature in `°C` (`10°C + 5K` is `15°C`). `°C + °C` is not meaningful and still goes through K.

**Errors** are a typed enum (parse error with span, unknown unit, ambiguous symbol, dimension mismatch, affine-in-compound).

**Quantity and column subtypes.**
- One generic runtime type: `Quantity { value: f64, sigma: f64, unit: Unit }`. Dimensions travel with `unit`.
- `Length`, `Force`, `Energy`, ... are **refinements of Quantity, not separate Rust types**: a `QuantityType` row in `resources/quantity_types.yaml` is just `name -> Dim` (plus an optional preferred display unit). `Length` = `Quantity` where `unit.dims == dims(Length)`; `QuantityType::admits(&Unit)` is the whole check, so every `Length` is a `Quantity` (subtype) but not vice versa.
- Several names share dims (Energy/Torque, Frequency/Radioactivity, Momentum/Impulse, Dose/SpecificEnergy). A column's declared type is still the point of definition, and any type of the right dimension admits a value (`admits` checks dims only). A **calculated result** is named by `Registry::infer_type(dims)` (decided with omasheet's `add-duration-kind`, task 0.2): the only type with the dimension, or where several share it the fixed one of `INFERRED` (`Area`, `Energy`, `Frequency`, `SpecificEnergy`, `Momentum`, the same choices Raku's type-hint table makes, but fixed in code rather than a setting); `None` where no type has it, so the result stays a plain `Quantity`. The unit of a result is the one its operands give, not the type's preferred unit, so a spreadsheet does not silently rebase `km/h` to `m/s`. `quantity_types_for(dims)` still lists every type, for pickers and for "must be a unit of Energy/Torque" messages.
- Column-level unit: a column of type `Length` may set a display unit (picker lists registry units of matching dims, SI first, then non-SI). Bare numbers in such a column are interpreted in that unit; explicit SI-suffixed cells are converted to it on entry (matches the `add-units` spec). The omx syntax for declaring this is omasheet's decision (its open question Q11).
- `Quantity` ops in-crate (convert, add/sub with dimension check, mul/div/pow/root) use first-order independent quadrature for sigma, a few lines; omasheet-engine can later swap in its own `uncertain_arith` kernels.

**Host boundary.** The crate does not scan numbers: the host (omasheet lexer, which already lexes `10.5(3)e-3`) splits a cell into number and unit text, so exponent-`e` versus unit-prefix collisions (`1e3m` vs `1Em`) stay its concern. The crate API is `Unit::parse(text, Scope)`; tests use a trivial test-only splitter.

## Stages

### Stage 0 — scaffold
New standalone repo and crate `siquant` at `measures/siquant` (name free on crates.io as of 2026-11-11; no `rust-` prefix needed), edition 2024, crates.io-ready metadata (description, license, repository, keywords, categories, `include` for `resources/`), MIT, `// Copyright (c) 2026 Stephen Roe` + `//!` docs per file, `.gitignore` with `target/`. CI parity with omasheet: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test`. Pick a maintained YAML crate (serde_yaml is archived; verify `serde_yaml_ng` or similar at implementation time).

### Stage 1 — SI units + SI prefixes (live cell units only)
- **Done.** `dim.rs`, `error.rs`, `registry.rs`, `parse.rs`, `unit.rs`, `quantity.rs` (the formatter is `Display for Unit` in `unit.rs`).
- `resources/prefixes.yaml`: the 24 SI prefixes quetta..quecto (including `da`), as `{base: 10, exp}`.
- `resources/units.yaml` live set: SI base units g (kg), m, A, K, mol, cd, s; named derived units without angle: Hz N Pa J W C V F Ω S Wb T H Bq Gy Sv kat, plus `°C` (affine, `K + 273.15`, non-prefixable, alt `ºC`). Excluded for now: rad/sr/lm/lx (Stage 3) and the imperial and US non-SI units (picker, later). The metric non-SI units that are each an exact multiple of an SI unit are in (omasheet's metric stage): litre `l` (alt `L`), tonne `t`, hectare `ha`, `bar`, electronvolt `eV`, calorie `cal`, watt-hour `Wh` and dalton `Da`.
- `resources/quantity_types.yaml`: names -> dims for the dimensions available here (Length, Mass, Current, Temperature, AmountOfSubstance, LuminousIntensity, Area, Volume, Speed, Acceleration, Frequency, Force, Pressure, Energy, Power, Charge, Potential, Capacitance, Resistance, Conductance, MagneticFlux, MagneticField, Inductance, ...), sourced from Raku `dims.yaml` (whose `ThermalConductance` omitted the length exponent; the load-time dimension check of each type's preferred unit caught it). The type name `Time` itself is held back to Stage 4; the SI second `s` stays because most derived units and every rate compound need it.
- Tests: accept/reject table for the cell grammar (accept `5kg`, `300cm`, `9.81m.s^-2`, `5kg.m^2.s^-2`, `2kPa`; reject `5 kg`, `5km/s`, `5m2`, `5m s`, `5m^2.s^-1/kg`, `5ft`, `5L`, `5min`, `5°C.s^-1`, `5m.°C`, `5k°C`, plurals/names); accept `20°C` and `20ºC`; every unit def resolves; prefix-ambiguity validator over the live set; `a -> b -> a` conversion round trips including `°C` <-> K (offset on value, not sigma; Raku goldens `39°C` = `312.15K`); `°C - °C` gives K; `5m + 300cm == 8m`; dimension mismatch error; `QuantityType::admits`; goldens ported from Raku `U/t/02-dim` (derived-unit dims) and the SI-only cases of `M/t/05-cvt` and `06-com`.

### Stage 2 — data prefixes (done)
Activate `D`. Add `b` (bit) and `B` (byte = 8 b) to the live set with `prefixable: both`; add binary prefixes Ki Mi Gi Ti Pi Ei Zi Yi Ri Qi to `prefixes.yaml` as `{base: 2, exp: 10n}` (schema unchanged). `kB` = 1000 B and `KiB` = 1024 B coexist; rates (`kb.s^-1`, `MiB.s^-1`) fall out with no hand-listed `kbps` entries. Extend the ambiguity validator to the binary prefixes (case-sensitive). Add `Data`/`DataRate` quantity types.

### Stage 3 — Angles (siquant done)
Activate `A`. Add rad, sr, lm (cd.sr), lx to the live set; define `°` (and ′ ″) as units with factor pi/180 etc. Add the DMS entry form `45°30′30″` for Angle columns, accepting alternates `º`, `'`, `"` and a leading minus; the numeric parts are plain decimals in siquant (`Quantity::from_dms` / `to_dms`, minutes and seconds under 60, only the last part may have a point), and the host reads any uncertainty. `rad` is the base unit of `A`, `sr` is `rad^2`, `°` `′` `″` are live and not prefixable, and the new menu groups are `Angle` and, for `lm` and `lx`, `Radiation` (the `Light` group was dropped as empty). The omasheet lexer, `Angle` columns and trig functions are the next increment. Add `Angle`, `SolidAngle`, `AngularSpeed`, `LuminousFlux`, `Illuminance` types.

### Stage 4 — Time (done, as built with omasheet's `add-duration-kind`)
- **Type:** `Duration` is a quantity type with dims `T`, preferred unit `s`, group SI Base, a refinement of `Quantity` like any other. omasheet's `Time` stays the time of day; siquant has no type called `Time`.
- **Units:** `min`, `h`, `d` and `wk` are units of 60, 3600, 86400 and 604800 s. They are not prefixable; `h` (hecto) and `d` (deci) stay prefixes before another unit (`hPa`, `dm`, and `h.`, `d.` as scales), while `kmin` and `dd` are refused. The function `min` is also a unit, which omasheet keeps apart by the bracket.
- **Storage is the host's:** omasheet holds a `Duration` column as a whole number of microseconds (a Polars `Duration`) and keeps the unit as display metadata; a `Duration` with an uncertainty, which microseconds cannot hold, is held as a quantity of seconds and the column is then calculated by the engine. So the planned `Quantity::from_micros` / `to_micros` and the promote/demote ladder were not built in siquant; the host converts with the unit factor (`as_duration` / `from_duration` in `omasheet-engine/src/quantity.rs`).
- **Forms:** the `hh:mm:ss` form was first dropped here for colliding with `Time`; omasheet then added a signed form, `+hh:mm:ss.x`, where the sign is what makes it a duration. That, ISO 8601 and `139s` are omasheet's literals, not siquant's.

### Metric non-SI units (done, 0.1.3 and 0.1.4)
`l` (`L`), `t`, `ha`, `bar`, `eV`, `cal`, `Wh` and `Da`, each an exact multiple of an SI unit, none imperial; `u` is read as the prefix `μ`.

### More metric units (done, 0.1.6; `mmHg` dropped again in 0.1.7)
Reviewed against Physics::Unit's `general.yaml`, keeping exact or current values where Raku's are rounded (its `ly` is 9.46e15 m and its parsec 3.083e16 m). Added: `au`, `ly`, `ld`, `pc` (the `ly`, `pc` and `yr` take prefixes: `kpc`, `Mly`, `Gyr`), `Å`, `nmi`, `kn`, `yr` (the Julian year), `are`, `barn` (spelled out, as `b` is the bit), `ct`, `kt` and `Mt` (the tonne takes no prefix), `gon`, `rev`, `atm`, `torr`, `hp` (the metric horsepower; `PS` is a petasiemens, and the validator refuses it), `rem`, `Ci`, `R`, `Jy`, `tog` and `sverdrup` (`Sv` is the sievert). Left out on purpose: imperial and US units, cgs (`dyn`, `erg`, `G`, `Mx`, `Oe`, `Gal`, `P`, `St`), the rad of dose (the radian's symbol), `g0` (a constant, not a unit), `kgf`, `Langley`, solid-angle `deg²` and `spat`, the wine-bottle sizes, and the averaged calendar units (`month`, `century`).

### Result kinds (done, 0.1.5)
`Registry::infer_type` and `INFERRED`, as described under *Quantity and column subtypes*. `lm`, `lx`, `LuminousFlux` and `Illuminance` are in `Radiation`; the empty `Light` group is gone (`GROUPS` has nine).


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

### The menu (done, 0.2.0)
Where a unit or a quantity type is filed in a picker used to be a `group:` field on each row of the yaml, with the order inside a group left to code. Both now live in one file, `resources/menu.txt`: `== Heading` lines in order, and under each the units in order, one a line (`N.m`, `kg.m^-3`; anything after ` | ` is a note). A type is filed under the heading of its preferred unit, a type with no unit under `Other`. The registry checks at load that every live unit and every preferred unit is listed, once, and that each line is a unit, so a new unit cannot be forgotten. `GROUPS` is replaced by `Registry::groups()` and `Registry::menu()`, and `Registry::from_yaml_with_menu` takes the menu (`from_yaml` files everything under `Other`). This is a breaking change, hence 0.2.0. `rpm` (2 pi / 60 rad.s^-1) was added with it.
