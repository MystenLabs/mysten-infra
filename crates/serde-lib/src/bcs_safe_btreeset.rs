// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Cryptographic applications using BCS cannot use BTreeSet as it does not enforce canonicity.
/// Here we introduce a BCS-safe version of BTreeSet that's implemented using BTreeMap under
/// the hood.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BcsSafeBTreeSet<T: Ord>(BTreeMap<T, ()>);

impl<T: Ord> Default for BcsSafeBTreeSet<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Ord> BcsSafeBTreeSet<T> {
    pub fn new() -> Self {
        Self(BTreeMap::new())
    }

    pub fn new_from_btreeset(data: BTreeSet<T>) -> Self {
        Self(data.into_iter().map(|v| (v, ())).collect())
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn contains(&self, value: &T) -> bool {
        self.0.contains_key(value)
    }

    pub fn insert(&mut self, value: T) -> bool {
        self.0.insert(value, ()).is_none()
    }

    pub fn remove(&mut self, value: &T) -> bool {
        self.0.remove(value).is_some()
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.0.keys()
    }

    pub fn into_btreeset(self) -> BTreeSet<T> {
        self.0.into_keys().collect()
    }

    pub fn clear(&mut self) {
        self.0.clear()
    }
}
