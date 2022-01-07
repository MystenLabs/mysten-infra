// Copyright(C) 2021, Mysten Labs
// SPDX-License-Identifier: Apache-2.0
use std::marker::PhantomData;

use bincode::Options;

use eyre::Result;
use serde::{de::DeserializeOwned, Serialize};

use super::DBRawIteratorMultiThreaded;

/// An iterator over all key-value pairs in a data map.
pub struct Iter<'a, K, V> {
    db_iter: DBRawIteratorMultiThreaded<'a>,
    _phantom: PhantomData<(K, V)>,
}

impl<'a, K: DeserializeOwned, V: DeserializeOwned> Iter<'a, K, V> {
    pub(super) fn new(db_iter: DBRawIteratorMultiThreaded<'a>) -> Self {
        Self {
            db_iter,
            _phantom: PhantomData,
        }
    }
}

impl<'a, K: DeserializeOwned, V: DeserializeOwned> Iterator for Iter<'a, K, V> {
    type Item = (K, V);

    fn next(&mut self) -> Option<Self::Item> {
        if self.db_iter.valid() {
            let config = bincode::DefaultOptions::new()
                .with_big_endian()
                .with_fixint_encoding();
            let key = self.db_iter.key().and_then(|k| config.deserialize(k).ok());
            let value = self
                .db_iter
                .value()
                .and_then(|v| bincode::deserialize(v).ok());

            self.db_iter.next();
            key.and_then(|k| value.map(|v| (k, v)))
        } else {
            None
        }
    }
}

impl<'a, K: Serialize, V> Iter<'a, K, V> {
    pub fn seek(mut self, key: &K) -> Result<Self> {
        let config = bincode::DefaultOptions::new()
            .with_big_endian()
            .with_fixint_encoding();
        self.db_iter.seek(config.serialize(key)?);
        Ok(self)
    }
}
