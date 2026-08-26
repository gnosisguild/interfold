// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

mod chain;
mod rounds;
mod state;
mod voting;

use actix_web::web;

pub fn setup_routes(config: &mut web::ServiceConfig) {
    state::setup_routes(config);
    voting::setup_routes(config);
    rounds::setup_routes(config);
    chain::setup_routes(config);
}
