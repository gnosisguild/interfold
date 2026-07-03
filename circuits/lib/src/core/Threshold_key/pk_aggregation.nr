// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WIT(A)OUT ANY WARRANTY;
// without even the implied warranty of MERC(A)ANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use crate::math::commitments::{compute_pk_aggregation_commitment, compute_threshold_pk_commitment};
use crate::math::modulo::U64::ModU64;
use crate::math::polynomial::Polynomial;

/// Parameters for threshold public key aggregation (C5).
pub struct Configs<let L: u32> {
    /// CRT moduli for each basis: [q_0, q_1, ..., q_{L-1}]
    pub qis: [Field; L],
}

impl<let L: u32> Configs<L> {
    pub fn new(qis: [Field; L]) -> Self {
        Configs { qis }
    }
}

/// Threshold public key aggregation (C5).
///
/// **Role:** Sum authorized parties' threshold PK limbs into `pk0_agg`, pin `pk1_agg` to the
/// circuit-constant CRS `a`, and commit the aggregated key for P3.
///
/// **Consumes:** `commit(pk0[h])` from C1 per authorized party `h`.
///
/// **Produces:** `commit(pk0_agg, crp)` for user-data encryption (P3).
///
/// **Verifies:** For each limb and coefficient, `pk0_agg` matches the mod-q_l sum of party
/// pk0 (centered coefficients). pk1_agg is not a witness; the CRS `crp` is used directly,
/// eliminating (A)*L*N per-party pk1 witness elements and the pk1 consistency loop.
pub struct PkAggregation<let N: u32, let A: u32, let L: u32, let BIT_PK: u32> {
    /// Circuit parameters including CRT moduli
    configs: Configs<L>,

    /// Common Reference String polynomials (circuit constant, one per CRT basis).
    /// Used directly as pk1_agg in the final commitment; no per-party pk1 witness needed.
    crp: [Polynomial<N>; L],

    /// Expected commitments to each authorized party's threshold public key share.
    /// commit(pk0[h]) produced by C1 for each h in (A).
    /// (public witnesses)
    expected_threshold_pk_commitments: [Field; A],

    /// Individual threshold public key first components from (A) authorized parties.
    /// pk0[party_idx][basis_idx] for each party and CRT basis.
    /// (committed witnesses)
    pk0: [[Polynomial<N>; L]; A],

    /// Claimed aggregated public key first component for each CRT basis.
    /// Must equal sum(pk0[h][l]) mod q_l for each basis l.
    /// (committed witness)
    pk0_agg: [Polynomial<N>; L],
}

impl<let N: u32, let A: u32, let L: u32, let BIT_PK: u32> PkAggregation<N, A, L, BIT_PK> {
    pub fn new(
        configs: Configs<L>,
        crp: [Polynomial<N>; L],
        expected_threshold_pk_commitments: [Field; A],
        pk0: [[Polynomial<N>; L]; A],
        pk0_agg: [Polynomial<N>; L],
    ) -> Self {
        PkAggregation { configs, crp, expected_threshold_pk_commitments, pk0, pk0_agg }
    }

    /// Verifies that each authorized party's pk0 hashes to its expected commitment from C1.
    ///
    /// C1 commits only to pk0 (not pk1) because pk1 = CRS is a public constant.
    /// Binding pk0 per-party is sufficient: pk1 is pinned globally via the crp field.
    fn verify_pk_commitments(self) {
        for i in 0..A {
            assert(
                compute_threshold_pk_commitment::<N, L, BIT_PK>(self.pk0[i])
                    == self.expected_threshold_pk_commitments[i],
                "PK commitment mismatch",
            );
        }
    }

    /// Verifies aggregation correctness for one component (pk0 or pk1) at a given CRT basis.
    ///
    /// For each coefficient position i, checks:
    ///   pk_agg[basis_idx][i] = sum(pk[h][basis_idx][i]) mod q_l  for h in 0..A
    ///
    /// The modular reduction ensures the aggregated coefficients stay within
    /// the valid range for the CRT modulus q_l.
    fn verify_pk_for_basis(
        self,
        pk: [[Polynomial<N>; L]; A],
        pk_agg: [Polynomial<N>; L],
        basis_idx: u32,
    ) {
        let q_l = self.configs.qis[basis_idx];
        // sum_shifted < (A)*2*q_l; quotient < 2(A) (small) -- safe for ModU64.
        let mod_q_l = ModU64::new(q_l as u64);

        let half_qi: Field = (q_l - 1) / 2;
        let h_half_qi: Field = A as Field * half_qi;
        // Offset keeps (sum_shifted - expected_unred + offset) non-negative in the integer
        // sense (not Field-wrapped). sum_shifted in [0, (A)*q_l), expected_unred in
        // [((A)-1)*q_l/2, ((A)+1)*q_l/2) -> delta + (A)*q_l >= ((A)-1)*q_l/2 >= 0; max quotient <= (A)+2.
        let offset: Field = A as Field * q_l;

        for coeff_idx in 0..N {
            let mut sum_shifted: Field = 0;
            for party_idx in 0..A {
                sum_shifted =
                    sum_shifted + pk[party_idx][basis_idx].coefficients[coeff_idx] + half_qi;
            }
            // Single divisibility check replaces two reduce_mod calls + equality assert:
            // saves ~37 gates per position (~48 gates -> ~12 gates for assert_zero_mod).
            mod_q_l.assert_zero_mod(
                sum_shifted + offset - pk_agg[basis_idx].coefficients[coeff_idx] - h_half_qi,
            );
        }
    }

    /// Returns `commit(pk0_agg, crp)` for P3 (`user_data_encryption_ct0` / `ct1`).
    pub fn execute(self) -> Field {
        // Step 1: Bind each party pk0 to its C1 commitment.
        self.verify_pk_commitments();

        // Step 2: Check `pk0_agg` is the mod-q_l sum of party pk0 limbs (per CRT basis).
        for basis_idx in 0..L {
            self.verify_pk_for_basis(self.pk0, self.pk0_agg, basis_idx);
        }

        // Step 3: Commit to (pk0_agg, crp). crp is the circuit-constant CRS a used as pk1_agg.
        compute_pk_aggregation_commitment::<N, L, BIT_PK>(self.pk0_agg, self.crp)
    }
}
