// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use serde_lib::bcs_safe_btreeset::BcsSafeBTreeSet;
use std::collections::BTreeSet;

#[test]
fn smoke_test() {
    let mut s = BTreeSet::new();
    s.insert(10);
    s.insert(20);
    let mut s = BcsSafeBTreeSet::new_from_btreeset(s);
    assert_eq!(s.len(), 2);
    assert!(s.contains(&10));
    assert!(s.insert(30));
    assert_eq!(s.len(), 3);
    assert!(s.contains(&30));
    assert!(!s.insert(20));
    assert_eq!(s.len(), 3);
    assert!(!s.remove(&40));
    assert!(s.remove(&20));
    assert_eq!(s.len(), 2);
    assert!(!s.is_empty());
}
