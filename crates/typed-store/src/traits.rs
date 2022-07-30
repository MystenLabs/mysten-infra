use pre::pre;
use rocksdb::Options;
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use serde::{de::DeserializeOwned, Serialize};
use std::{borrow::Borrow, collections::BTreeMap, error::Error, path::PathBuf};

pub trait Map<'a, K, V>
where
    K: Serialize + DeserializeOwned + ?Sized,
    V: Serialize + DeserializeOwned,
{
    type Error: Error;
    type Iterator: Iterator<Item = (K, V)>;
    type Keys: Iterator<Item = K>;
    type Values: Iterator<Item = V>;

    /// Returns true if the map contains a value for the specified key.
    fn contains_key(&self, key: &K) -> Result<bool, Self::Error>;

    /// Returns the value for the given key from the map, if it exists.
    fn get(&self, key: &K) -> Result<Option<V>, Self::Error>;

    /// Returns the value for the given key from the map, if it exists
    /// or the given default value if it does not.
    fn get_or_insert<F: FnOnce() -> V>(&self, key: &K, default: F) -> Result<V, Self::Error> {
        self.get(key).and_then(|optv| match optv {
            Some(v) => Ok(v),
            None => {
                self.insert(key, &default())?;
                self.get(key).transpose().expect("default just inserted")
            }
        })
    }

    /// Inserts the given key-value pair into the map.
    fn insert(&self, key: &K, value: &V) -> Result<(), Self::Error>;

    /// Removes the entry for the given key from the map.
    fn remove(&self, key: &K) -> Result<(), Self::Error>;

    /// Removes every key-value pair from the map.
    fn clear(&self) -> Result<(), Self::Error>;

    /// Returns true if the map is empty, otherwise false.
    fn is_empty(&self) -> bool;

    /// Returns an iterator visiting each key-value pair in the map.
    fn iter(&'a self) -> Self::Iterator;

    /// Returns an iterator over each key in the map.
    fn keys(&'a self) -> Self::Keys;

    /// Returns an iterator over each value in the map.
    fn values(&'a self) -> Self::Values;

    /// Returns a vector of values corresponding to the keys provided.
    fn multi_get<J>(
        &self,
        keys: impl IntoIterator<Item = J>,
    ) -> Result<Vec<Option<V>>, Self::Error>
    where
        J: Borrow<K>;

    /// Inserts key-value pairs.
    fn multi_insert<J, U>(
        &self,
        key_val_pairs: impl IntoIterator<Item = (J, U)>,
    ) -> Result<(), Self::Error>
    where
        J: Borrow<K>,
        U: Borrow<V>;

    /// Removes keys.
    fn multi_remove<J>(&self, keys: impl IntoIterator<Item = J>) -> Result<(), Self::Error>
    where
        J: Borrow<K>;

    /// Try to catch up with primary when running as secondary
    fn try_catch_up_with_primary(&self) -> Result<(), Self::Error>;
}

/// Traits for DBMap table groups
/// Table needs to be opened to secondary (read only) mode for most features here to work
/// This trait is needed for #[derive(DBMapUtils)] on structs which have all members as DBMap<K, V>
pub trait DBMapTableUtil {
    fn open_tables_read_write(path: PathBuf, db_options: Option<Options>) -> Self;

    fn open_tables_read_only(
        path: PathBuf,
        with_secondary_path: Option<PathBuf>,
        db_options: Option<Options>,
    ) -> Self;

    fn open_tables_impl(
        path: PathBuf,
        with_secondary_path: Option<PathBuf>,
        db_options: Option<Options>,
    ) -> Self;

    fn adjusted_db_options(
        db_options: Option<Options>,
        cache_capacity: usize,
        point_lookup: bool,
    ) -> Options;

    /// Lists all the tables in a DBMap group
    #[pre("Must be called only after `open_tables_read_only`")]
    fn list_tables(path: PathBuf) -> anyhow::Result<Vec<String>>;

    /// Dumps all the entries in the page of the table
    #[pre("Must be called only after `open_tables_read_only`")]
    fn dump(
        &self,
        table_name: &str,
        page_size: u16,
        page_number: usize,
    ) -> anyhow::Result<BTreeMap<String, String>>;

    /// Counts the keys in the table
    #[pre("Must be called only after `open_tables_read_only`")]
    fn count_keys(&self, table_name: &str) -> anyhow::Result<usize>;
}
