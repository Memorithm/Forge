//! Domains d'évaluation pour forge-core.
//!
//! Chaque module implémente le trait [`forge_core::Domain`] pour un terrain
//! d'optimisation spécifique.
//!
//! ## Domaines disponibles
//! - [`tensor_train`] : Compression Tensor Train (Low-Rank) — optimise les rangs
//!   pour maximiser le ratio de compression tout en minimisant l'erreur de
//!   reconstruction.
//! - [`sml_topology`] : recherche bornée de DAG booléens pour un oracle SML
//!   fourni par le dépôt consommateur, avec split développement/holdout disjoint.

pub mod tensor_train;

pub mod sml_topology;
