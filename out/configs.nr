use crate::core::threshold::decrypted_shares_aggregation::Configs as DecryptedSharesAggregationConfigs;

pub global L: u32 = 3;
pub global QIS: [Field; L] = [288230376164294657, 288230376162459649, 288230376161738753];
pub global PLAINTEXT_MODULUS: Field = 1000000;
pub global Q_MOD_T: Field = 964929;
pub global Q_MOD_T_CENTERED: Field = -35071;
pub global Q_INVERSE_MOD_T: Field = 663169;
        
/************************************
-------------------------------------
decrypted_shares_aggregation (CIRCUIT 7)
-------------------------------------
************************************/

pub global DECRYPTED_SHARES_AGGREGATION_BIT_NOISE: u32 = 154;
pub global DECRYPTED_SHARES_AGGREGATION_BIT_D_NATIVE: u32 = 59;

pub global DECRYPTED_SHARES_AGGREGATION_CONFIGS: DecryptedSharesAggregationConfigs<L> =
    DecryptedSharesAggregationConfigs::new(QIS, PLAINTEXT_MODULUS, Q_INVERSE_MOD_T);
