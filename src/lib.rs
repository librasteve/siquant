// Copyright (c) 2026 Stephen Roe

//! SI units, prefixes and quantities with uncertainty.
//!
//! A unit is an immutable list of prefixed unit terms over a fixed vector of
//! base-dimension exponents; the units, the prefixes and the named quantity
//! types are defined in YAML files under `resources/`, embedded at build
//! time. A `Quantity` is a value, an uncertainty and a unit, and a named type
//! such as `Length` is that `Quantity` restricted to one dimension.
//!
//! This is the scaffold (Stage 0). The staged plan is in `docs/plan.md`:
//! SI units and prefixes first, then data prefixes, angles and durations.
