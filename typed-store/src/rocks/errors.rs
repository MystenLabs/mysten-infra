// Copyright(C) 2022, Mysten Labs
// SPDX-License-Identifier: Apache-2.0

use bincode::Error as BincodeError;
use rocksdb::Error as RocksError;
use thiserror::Error;

#[non_exhaustive]
#[derive(Error, Debug)]
pub enum TypedStoreError {
    #[error("rocksdb error")]
    RocksDBError(#[from] RocksError),
    #[error("(de)serialization error")]
    SerializationError(#[from] BincodeError),
    #[error("the column family {0} was not registered with the database")]
    UnregisteredColumn(String),
    #[error("a batch operation can't operate across databases")]
    CrossDBBatch,
}
