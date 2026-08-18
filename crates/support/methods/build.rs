// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

// Copyright 2023 RISC Zero, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::{
    collections::HashMap,
    env, fs,
    path::{Path, PathBuf},
};

use risc0_build::{embed_methods_with_options, DockerOptionsBuilder, GuestOptionsBuilder};
use risc0_build_ethereum::generate_solidity_files;

// Paths where the generated Solidity files will be written.
const SOLIDITY_IMAGE_ID_PATH: &str = "../contracts/ImageID.sol";
const SOLIDITY_ELF_PATH: &str = "../tests/Elf.sol";

/// Reports whether the reproducible Docker guest build is selected.
///
/// The variable is read for its value, not its presence, so `RISC0_USE_DOCKER=0` selects the
/// local build.
fn use_docker() -> bool {
    matches!(
        env::var("RISC0_USE_DOCKER").unwrap_or_default().as_str(),
        "1" | "true" | "TRUE" | "yes" | "YES"
    )
}

/// The guest builder image tag, derived from `ARG RISC0_TOOLCHAIN` in `crates/support/Dockerfile`.
///
/// risc0-build does not read that Dockerfile — it generates its own and, left alone, uses its
/// compiled-in default tag. For risc0-build 3.0.3 that default is `r0.1.88.0`, which carries rustc
/// 1.88, while the guest's dependency tree pins fhe.rs at an MSRV of 1.91.1. The guest then fails
/// to compile inside the container with an MSRV error, and the Dockerfile that says 1.91.1 has no
/// bearing on it.
///
/// Reading the tag from `ARG RISC0_TOOLCHAIN` is what ties the two together: the toolchain the
/// Dockerfile declares becomes the toolchain the ELF is actually built with, rather than the two
/// being independent values that happen to agree.
fn guest_builder_tag(support_dir: &Path) -> String {
    let dockerfile = support_dir.join("Dockerfile");
    println!("cargo:rerun-if-changed={}", dockerfile.display());

    let source = fs::read_to_string(&dockerfile).unwrap_or_else(|e| {
        panic!(
            "cannot read {} to resolve the guest toolchain: {e}",
            dockerfile.display()
        )
    });

    let toolchain = source
        .lines()
        .find_map(|line| line.trim().strip_prefix("ARG RISC0_TOOLCHAIN="))
        .unwrap_or_else(|| panic!("no ARG RISC0_TOOLCHAIN in {}", dockerfile.display()))
        .trim();

    assert!(
        !toolchain.is_empty(),
        "ARG RISC0_TOOLCHAIN in {} is empty",
        dockerfile.display()
    );

    format!("r0.{toolchain}")
}

fn main() {
    // Builds can be made deterministic, and thereby reproducible, by using Docker to build the
    // guest. Set RISC0_USE_DOCKER to 1 (or true) to select the reproducible Docker build. Any
    // other value, and an unset variable, select the local build.
    println!("cargo:rerun-if-env-changed=RISC0_USE_DOCKER");
    println!("cargo:rerun-if-changed=build.rs");
    let manifest_dir = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let mut builder = GuestOptionsBuilder::default();
    if use_docker() {
        let support_dir = manifest_dir.join("../");
        let docker_options = DockerOptionsBuilder::default()
            .root_dir(support_dir.clone())
            .docker_container_tag(guest_builder_tag(&support_dir))
            .build()
            .unwrap();
        builder.use_docker(docker_options);
    }
    let guest_options = builder.build().unwrap();

    // Generate Rust source files for the methods crate.
    let guests = embed_methods_with_options(HashMap::from([("guests", guest_options)]));

    if std::env::var("SKIP_SOLIDITY").unwrap_or_default() != "1" {
        // Generate Solidity source files for use with Forge.
        let solidity_opts = risc0_build_ethereum::Options::default()
            .with_image_id_sol_path(SOLIDITY_IMAGE_ID_PATH)
            .with_elf_sol_path(SOLIDITY_ELF_PATH);
        generate_solidity_files(guests.as_slice(), &solidity_opts).unwrap();
    } else {
        println!("cargo:warning=Skipping solidity codegen (SKIP_SOLIDITY set)");
    }
}
