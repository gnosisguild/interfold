// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use alloy::primitives::U256;

pub(crate) fn format_amount(amount: U256, decimals: u8) -> String {
    let scale = U256::from(10u64).pow(U256::from(decimals as u64));
    let int_part = amount / scale;
    let frac_part = amount % scale;

    if frac_part == U256::from(0) {
        int_part.to_string()
    } else {
        let frac_str = frac_part.to_string();
        let frac_padded = format!("{:0>width$}", frac_str, width = decimals as usize);
        let frac_trimmed = frac_padded.trim_end_matches('0');
        if frac_trimmed.is_empty() {
            int_part.to_string()
        } else {
            format!("{}.{}", int_part, frac_trimmed)
        }
    }
}
