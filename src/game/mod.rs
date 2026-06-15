//! موتور اصلی — multi-strategy address resolution + tick loop.

mod engine;
mod local_player;
mod position;

pub use local_player::discover as discover_local_player;
pub use position::{
    collect_global_origin_bases, collect_player_candidates, collect_position_candidates,
    discover_by_movement, discover_offset as discover_position_offset,
    discover_player_and_offset, discover_position_live, prepare_walk_test, peek_vec3,
    print_position_diagnostics, read_configured_global_position, read_global_world_at_rva,
    read_hw_entity_world_origin, resolve_hw_local_player_position, read_runtime_world_vec3,
    read_view_aux, read_vec3, read_world_vec3, is_usable_world_position, looks_like_view_aux,
    looks_like_world_origin, scan_module_globals_for_movement, PlayerCandidate, PositionDiscovery,
    POS_OFFSET_CANDIDATES,
};

pub use engine::{connect, GameEngine};
pub use state::{DebugSnapshot, GameState, ResolvedInfo, StatusDisplay};

mod state;
