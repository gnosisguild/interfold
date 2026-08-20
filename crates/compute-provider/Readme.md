# FHE Compute Manager

This project provides a framework for managing Secure Programs (SP) of the
[Interfold Protocol](https://theinterfold.com), with the ability to integrate various compute
providers.

## Features

- Flexible integration of different compute providers
- Merkle tree generation for input verification
- Ciphertext hashing for output verification
- Per-program input policies that decide the leaf layout and which inputs the computation sees

## Installation

To use this library, add it to your `Cargo.toml`:

```toml
[dependencies]
e3-compute-provider = { git = "https://github.com/theinterfold/interfold.git" }
```

## Usage

To use the library, follow these steps:

1. Create an instance of the `ComputeManager` with your compute provider and inputs.
2. Call the `start` method with your E3 program's `InputPolicy`.
3. The method returns the provider output together with the computed ciphertext bytes.

```rust
use e3_compute_provider::{ComputeError, ComputeManager, ComputeProvider, FHEInputs, InputPolicy};
use my_program::fhe_processor;

pub fn run_compute<P>(params: FHEInputs, provider: P) -> Result<(P::Output, Vec<u8>), ComputeError>
where
    P: ComputeProvider + Send + Sync,
{
    let mut manager = ComputeManager::new(provider, params, fhe_processor);
    manager.start(InputPolicy::default())
}
```

`fhe_processor` is your own function. It must match the exported `FHEProcessor` alias,
`fn(&FHEInputs) -> Vec<u8>`.

## Input policies

`InputPolicy` carries the two answers that differ between E3 programs:

- `leaf` builds a tree leaf. It must equal what the E3 program builds on chain for the same input.
- `select` chooses which inputs the computation runs over, by index.

`InputPolicy::default()` is the behaviour every E3 program had before policies existed. The leaf is
the ciphertext's own SAFE commitment, and every input is computed over. A program whose contract
inserts something else, or that treats a second input from one participant as a replacement,
supplies its own.

A policy cannot supply a root or drop an input from the tree. Every published ciphertext gets a leaf
built from its own bytes, whatever `select` then decides to compute over.

When your E3 program publishes a commitment or other data alongside each ciphertext, build the
manager with `with_published` so the policy can read it:

```rust
let mut manager = ComputeManager::with_published(provider, params, published, fhe_processor);
```

## Implementing a provider

`ComputeProvider` has one method and one associated type. Everything else is yours to choose:

```rust
use e3_compute_provider::{ComputeInput, ComputeProvider, InputPolicy};

pub struct MyProvider;

pub struct MyOutput {
    pub proof: Vec<u8>,
}

impl ComputeProvider for MyProvider {
    type Output = MyOutput;

    fn prove(&self, input: &ComputeInput, policy: InputPolicy) -> Self::Output {
        // Prove that `input` produced its committed result under `policy`, however your
        // backend does that, and return whatever the caller needs.
        MyOutput { proof: Vec::new() }
    }
}
```

`prove` receives the policy rather than choosing one. A prover that picked its own would select a
different input set from the one `start` returned the ciphertext for.

The repository's RISC Zero and Boundless providers live in `e3-support-host`. That crate is in a
separate workspace, so the dependency above does not pull it in. Inside an Interfold checkout, its
`run_risc0_compute` and `run_compute` entry points wrap the two backends, and
`crates/support/host/src/lib.rs` is the reference implementation to read.

## Configuration

`ComputeManager::new()` takes three parameters:

- `provider`: An instance of your compute provider (e.g., `MyProvider`)
- `fhe_inputs`: The FHE inputs for the computation
- `fhe_processor`: A function to process the FHE inputs

`ComputeManager::with_published()` takes the same three, plus `published`: one `PublishedData` entry
per ciphertext, in the same order as `fhe_inputs.ciphertexts`.
