// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

// Flight handlers split into three cohesive sub-modules:
//   * crud:    list / get / create / update / delete / stats
//   * import:  DJI log (binary + CSV) parsing + preview
//   * commit:  EASA debrief → committed flight + auto-register aircraft/battery/controller
//
// main.rs references `handlers::flights::fn_name` unchanged — each sub-module's
// public fns are re-exported here.

pub mod crud;
pub mod detail;
pub mod import;
pub mod commit;

pub use crud::{
    create_flight, delete_flight, filter_options, get_flight, get_stats, list_flights,
    update_flight,
};
pub use detail::get_flight_detail;
pub use import::{import_ardupilot_log, import_dji_log};
pub use commit::commit_imported_flight;
