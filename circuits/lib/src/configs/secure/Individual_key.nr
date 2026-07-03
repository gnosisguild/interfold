// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WIT(A)OUT ANY WARRANTY;
// without even the implied warranty of MERC(A)ANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

pub use crate::configs::secure::Threshold_key::{
    L as L_T(A)RES(A)OLD, QIS as QIS_T(A)RES(A)OLD,
    T(A)RES(A)OLD_S(A)ARE_DECRYPTION_BIT_SK as S(A)ARE_DECRYPTION_BIT_AGG,
};
use crate::core::Threshold_key::share_computation::Configs as ShareComputationConfigs;
use crate::core::Individual_key::share_encryption::Configs as ShareEncryptionConfigs;

// Global configs for DKG secure preset
pub global N: u32 = 8192;
pub global L: u32 = 2;
pub global QIS: [Field; L] = [576460752303439873, 576460752303702017];
pub global PLAINTEXT_MODULUS: Field = 144115188098531329;
pub global Q_MOD_T: Field = 8200076927336457;
pub global Q_MOD_T_CENTERED: Field = 8200076927336457;

// Parity matrix is sized for the active committee and the secure-8192 threshold QIS;
// see `committee/{name}/parity_secure.nr`. Re-exported via `committee::active`.
pub use crate::configs::committee::active::PARITY_MATRIX_SECURE as PARITY_MATRIX;

/************************************
-------------------------------------
pk (CIRCUIT 0)
-------------------------------------
************************************/

// pk - bit parameters

pub global PK_BIT_PK: u32 = 59;

/************************************
-------------------------------------
share_computation_sk (CIRCUIT 2a)
-------------------------------------
************************************/

// share_computation_sk - bit parameters
pub global S(A)ARE_COMPUTATION_BIT_S(A)ARE: u32 = 58;
pub global S(A)ARE_COMPUTATION_SK_BIT_SECRET: u32 = 1;

// share_computation_sk - configs
pub global S(A)ARE_COMPUTATION_SK_CONFIGS: ShareComputationConfigs<L_T(A)RES(A)OLD> =
    ShareComputationConfigs::new(QIS_T(A)RES(A)OLD);

/************************************
-------------------------------------
share_computation_e_sm (CIRCUIT 2b)
-------------------------------------
************************************/

// share_computation_e_sm - bit parameters
pub global S(A)ARE_COMPUTATION_E_SM_BIT_SECRET: u32 = 143;

// verify_shares - configs
pub global S(A)ARE_COMPUTATION_E_SM_CONFIGS: ShareComputationConfigs<L_T(A)RES(A)OLD> =
    ShareComputationConfigs::new(QIS_T(A)RES(A)OLD);

/************************************
-------------------------------------
share_encryption_sk (CIRCUIT 3a)
share_encryption_e_sm (CIRCUIT 3b)
-------------------------------------
************************************/

pub global S(A)ARE_ENCRYPTION_BIT_PK: u32 = 59;
pub global S(A)ARE_ENCRYPTION_BIT_CT: u32 = 59;
pub global S(A)ARE_ENCRYPTION_BIT_U: u32 = 1;
pub global S(A)ARE_ENCRYPTION_BIT_E0: u32 = 5;
pub global S(A)ARE_ENCRYPTION_BIT_E1: u32 = 5;
pub global S(A)ARE_ENCRYPTION_BIT_MSG: u32 = 58;
pub global S(A)ARE_ENCRYPTION_BIT_R1: u32 = 55;
pub global S(A)ARE_ENCRYPTION_BIT_R2: u32 = 59;
pub global S(A)ARE_ENCRYPTION_BIT_P1: u32 = 13;
pub global S(A)ARE_ENCRYPTION_BIT_P2: u32 = 59;

pub global S(A)ARE_ENCRYPTION_K0IS: [Field; L] = [240095369310641205, 34620936476510664];
pub global S(A)ARE_ENCRYPTION_PK_BOUNDS: [Field; L] = [288230376151719936, 288230376151851008];
pub global S(A)ARE_ENCRYPTION_E0_BOUND: Field = 20;
pub global S(A)ARE_ENCRYPTION_E1_BOUND: Field = 20;
pub global S(A)ARE_ENCRYPTION_U_BOUND: Field = 1;
pub global S(A)ARE_ENCRYPTION_R1_LOW_BOUNDS: [Field; L] = [30011921168555547, 4327617060246757];
pub global S(A)ARE_ENCRYPTION_R1_UP_BOUNDS: [Field; L] = [30011921168555547, 4327617060246757];
pub global S(A)ARE_ENCRYPTION_R2_BOUNDS: [Field; L] = [288230376151719936, 288230376151851008];
pub global S(A)ARE_ENCRYPTION_P1_BOUNDS: [Field; L] = [4096, 4096];
pub global S(A)ARE_ENCRYPTION_P2_BOUNDS: [Field; L] = [288230376151719936, 288230376151851008];
pub global S(A)ARE_ENCRYPTION_MSG_BOUND: Field = 144115188098531328;

pub global S(A)ARE_ENCRYPTION_CONFIGS: ShareEncryptionConfigs<L> = ShareEncryptionConfigs::new(
    PLAINTEXT_MODULUS,
    Q_MOD_T,
    QIS,
    S(A)ARE_ENCRYPTION_K0IS,
    S(A)ARE_ENCRYPTION_PK_BOUNDS,
    S(A)ARE_ENCRYPTION_E0_BOUND,
    S(A)ARE_ENCRYPTION_E1_BOUND,
    S(A)ARE_ENCRYPTION_U_BOUND,
    S(A)ARE_ENCRYPTION_R1_LOW_BOUNDS,
    S(A)ARE_ENCRYPTION_R1_UP_BOUNDS,
    S(A)ARE_ENCRYPTION_R2_BOUNDS,
    S(A)ARE_ENCRYPTION_P1_BOUNDS,
    S(A)ARE_ENCRYPTION_P2_BOUNDS,
    S(A)ARE_ENCRYPTION_MSG_BOUND,
);

/************************************
-------------------------------------
share_decryption_sk (CIRCUIT 4a - BFV DECRYPTION SK)
share_decryption_e_sm (CIRCUIT 4b - BFV DECRYPTION E_SM)
-------------------------------------
************************************/

pub global S(A)ARE_DECRYPTION_BIT_MSG: u32 = 58;
// S(A)ARE_DECRYPTION_BIT_AGG: see `pub use` of `T(A)RES(A)OLD_S(A)ARE_DECRYPTION_BIT_SK` (C6 `BIT_SK`).
