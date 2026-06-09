//! Engine-level integration tests, split by feature area.
//!
//! Each submodule opens with `use super::*;` which resolves to this
//! `mod.rs`, pulling in the shared imports below.

#![allow(unused_imports)]

use super::*;
use crate::testing::setup_isolated_test_engine;

pub mod basics;
pub mod constraints;
pub mod crud;
pub mod errors;
pub mod fulltext;
pub mod indexes;
pub mod query;
pub mod transactions;
pub mod write;
