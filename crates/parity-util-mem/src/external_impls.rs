// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{MallocShallowSizeOf, MallocSizeOf};

// fastcrypto
malloc_size_of_is_0!(fastcrypto::ed25519::Ed25519PublicKey);
malloc_size_of_is_0!(fastcrypto::ed25519::Ed25519Signature);

// hash_map
malloc_size_of_is_0!(std::collections::hash_map::RandomState);

// indexmap
impl<K: MallocSizeOf, V: MallocSizeOf, S> MallocShallowSizeOf for indexmap::IndexMap<K, V, S> {
    fn shallow_size_of(&self, _ops: &mut crate::MallocSizeOfOps) -> usize {
        self.capacity()
            * (std::mem::size_of::<K>()
                + std::mem::size_of::<V>()
                + (2 * std::mem::size_of::<usize>()))
    }
}
impl<K: MallocSizeOf, V: MallocSizeOf, S> MallocSizeOf for indexmap::IndexMap<K, V, S> {
    // This only produces a rough estimate of IndexMap size, because we cannot access private
    // fields to measure them precisely.
    fn size_of(&self, ops: &mut crate::MallocSizeOfOps) -> usize {
        let mut n = self.shallow_size_of(ops);
        if let (Some(k), Some(v)) = (K::constant_size(), V::constant_size()) {
            n += self.len() * (k + v)
        } else {
            n += self
                .iter()
                .fold(n, |acc, (k, v)| acc + k.size_of(ops) + v.size_of(ops))
        }
        n
    }
}
