//! Constraint management for Catalog
//!
//! Handles storage and retrieval of database constraints (UNIQUE, EXISTS)

use crate::{Error, Result};
use heed::types::*;
use heed::{Database, Env, byteorder};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Constraint type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConstraintType {
    /// UNIQUE constraint - property value must be unique across all nodes with the label
    Unique,
    /// EXISTS constraint - property must exist (not null) on all nodes with the label
    Exists,
}

/// Constraint definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Constraint {
    /// Constraint type
    pub constraint_type: ConstraintType,
    /// Label ID this constraint applies to
    pub label_id: u32,
    /// Property key ID this constraint applies to
    pub property_key_id: u32,
}

/// Constraint manager
pub struct ConstraintManager {
    /// Constraints database: (label_id, property_key_id) -> Constraint
    constraints_db: Database<SerdeBincode<(u32, u32)>, SerdeBincode<Constraint>>,
    /// Reverse lookup: constraint_id -> (label_id, property_key_id)
    constraint_id_to_key: Database<U32<byteorder::NativeEndian>, SerdeBincode<(u32, u32)>>,
    /// Next constraint ID counter
    next_constraint_id: u32,
    /// LMDB environment
    env: Env,
}

impl ConstraintManager {
    /// Create a new constraint manager with existing databases
    pub fn new_with_databases(
        env: &Env,
        constraints_db: Database<SerdeBincode<(u32, u32)>, SerdeBincode<Constraint>>,
        constraint_id_to_key: Database<U32<byteorder::NativeEndian>, SerdeBincode<(u32, u32)>>,
    ) -> Result<Self> {
        // Initialize next constraint ID by scanning existing constraints
        let rtxn = env.read_txn()?;
        let next_constraint_id = constraint_id_to_key
            .iter(&rtxn)?
            .map(|r| r.map(|(id, _)| id))
            .collect::<std::result::Result<Vec<_>, _>>()?
            .into_iter()
            .max()
            .map(|max_id| max_id + 1)
            .unwrap_or(0);
        drop(rtxn);

        Ok(Self {
            constraints_db,
            constraint_id_to_key,
            next_constraint_id,
            env: env.clone(),
        })
    }

    /// Create a new constraint manager (creates databases)
    pub fn new(env: &Env) -> Result<Self> {
        let mut wtxn = env.write_txn()?;

        let constraints_db: Database<SerdeBincode<(u32, u32)>, SerdeBincode<Constraint>> =
            env.create_database(&mut wtxn, Some("constraints"))?;
        let constraint_id_to_key: Database<U32<byteorder::NativeEndian>, SerdeBincode<(u32, u32)>> =
            env.create_database(&mut wtxn, Some("constraint_id_to_key"))?;

        wtxn.commit()?;

        Self::new_with_databases(env, constraints_db, constraint_id_to_key)
    }

    /// Create a new constraint
    pub fn create_constraint(
        &mut self,
        constraint_type: ConstraintType,
        label_id: u32,
        property_key_id: u32,
    ) -> Result<u32> {
        let mut wtxn = self.env.write_txn()?;

        let key = (label_id, property_key_id);

        // Check if constraint already exists
        if self.constraints_db.get(&wtxn, &key)?.is_some() {
            return Err(Error::CypherExecution(format!(
                "Constraint already exists on :{} ({})",
                label_id, property_key_id
            )));
        }

        let constraint = Constraint {
            constraint_type,
            label_id,
            property_key_id,
        };

        // Store constraint
        self.constraints_db.put(&mut wtxn, &key, &constraint)?;

        // Store reverse mapping
        let constraint_id = self.next_constraint_id;
        self.constraint_id_to_key
            .put(&mut wtxn, &constraint_id, &key)?;
        self.next_constraint_id += 1;

        wtxn.commit()?;

        Ok(constraint_id)
    }

    /// Drop a constraint
    pub fn drop_constraint(
        &mut self,
        constraint_type: ConstraintType,
        label_id: u32,
        property_key_id: u32,
    ) -> Result<bool> {
        let mut wtxn = self.env.write_txn()?;

        let key = (label_id, property_key_id);

        // Check if constraint exists and matches type
        match self.constraints_db.get(&wtxn, &key)? {
            Some(constraint) if constraint.constraint_type == constraint_type => {
                // Remove constraint
                self.constraints_db.delete(&mut wtxn, &key)?;

                // Find and remove reverse mapping
                let constraint_id = {
                    let rtxn = self.env.read_txn()?;
                    self.constraint_id_to_key.iter(&rtxn)?.find_map(|r| {
                        r.ok()
                            .and_then(|(id, k)| if k == key { Some(id) } else { None })
                    })
                };
                if let Some(constraint_id) = constraint_id {
                    self.constraint_id_to_key
                        .delete(&mut wtxn, &constraint_id)?;
                }

                wtxn.commit()?;
                Ok(true)
            }
            Some(_) => {
                wtxn.commit()?;
                Err(Error::CypherExecution(format!(
                    "Constraint type mismatch on :{} ({})",
                    label_id, property_key_id
                )))
            }
            None => {
                wtxn.commit()?;
                Ok(false)
            }
        }
    }

    /// Get all constraints for a label
    pub fn get_constraints_for_label(&self, label_id: u32) -> Result<Vec<Constraint>> {
        let rtxn = self.env.read_txn()?;
        let constraints: Vec<Constraint> = self
            .constraints_db
            .iter(&rtxn)?
            .filter_map(|r| {
                r.ok().and_then(|((l_id, _), constraint)| {
                    if l_id == label_id {
                        Some(constraint)
                    } else {
                        None
                    }
                })
            })
            .collect();
        Ok(constraints)
    }

    /// Get all constraints
    pub fn get_all_constraints(&self) -> Result<HashMap<(u32, u32), Constraint>> {
        let rtxn = self.env.read_txn()?;
        let mut constraints = HashMap::new();
        for result in self.constraints_db.iter(&rtxn)? {
            let ((label_id, prop_id), constraint) = result?;
            constraints.insert((label_id, prop_id), constraint);
        }
        Ok(constraints)
    }

    /// Check if a constraint exists
    pub fn has_constraint(
        &self,
        constraint_type: ConstraintType,
        label_id: u32,
        property_key_id: u32,
    ) -> Result<bool> {
        let rtxn = self.env.read_txn()?;
        let key = (label_id, property_key_id);
        Ok(self
            .constraints_db
            .get(&rtxn, &key)?
            .map(|c| c.constraint_type == constraint_type)
            .unwrap_or(false))
    }

    /// Get a specific constraint
    pub fn get_constraint(
        &self,
        label_id: u32,
        property_key_id: u32,
    ) -> Result<Option<Constraint>> {
        let rtxn = self.env.read_txn()?;
        let key = (label_id, property_key_id);
        Ok(self.constraints_db.get(&rtxn, &key)?)
    }
}
