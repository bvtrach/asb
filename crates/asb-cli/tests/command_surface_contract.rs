// Copyright (C) Huawei Technologies Co., Ltd. 2026. All rights reserved.
// SPDX-License-Identifier: MIT
//! Mechanical drift check for the public CLI route inventory (AR-1805).

use serde::Deserialize;
use std::collections::BTreeSet;

#[derive(Deserialize)]
struct Manifest {
    schema_version: u16,
    routes: Vec<Route>,
}

#[derive(Deserialize)]
struct Route {
    path: String,
    human: bool,
    json: bool,
    help: bool,
    list: bool,
}

#[test]
fn manifest_is_unique_and_covers_every_public_family() {
    let manifest: Manifest =
        serde_json::from_str(include_str!("../fixtures/human/command-surface-v1.json"))
            .expect("command surface manifest is valid JSON");
    assert_eq!(manifest.schema_version, 1);
    let mut paths = BTreeSet::new();
    for route in &manifest.routes {
        assert!(paths.insert(&route.path), "duplicate route {}", route.path);
        assert!(
            route.human || route.json,
            "route has no output mode: {}",
            route.path
        );
        assert!(route.help, "route has no help contract: {}", route.path);
        if route.list {
            assert!(
                route.human || route.json,
                "list route has no output: {}",
                route.path
            );
        }
        assert!(!route.path.trim().is_empty());
    }
    let families = manifest
        .routes
        .iter()
        .map(|route| route.path.split_whitespace().next().unwrap())
        .collect::<BTreeSet<_>>();
    let public = asb_cli::public_command_inventory()
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    assert_eq!(families, public, "manifest and dispatch inventory drifted");
}
