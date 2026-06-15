//! [EN] Game module — core engine, state management, local player discovery, and position tracking.
//! [FA] ماژول بازی — موتور اصلی، مدیریت وضعیت، کشف بازیکن محلی و ردیابی موقعیت.

/// [EN] Game engine logic: tick loop, memory reads, address resolution.
/// [FA] منطق موتور بازی: حلقه tick، خواندن حافظه، حل آدرس.
mod engine;

/// [EN] Local player pointer discovery across CS 1.6 build variants.
/// [FA] کشف اشاره‌گر بازیکن محلی در نسخه‌های مختلف CS 1.6.
mod local_player;

/// [EN] Position (vec3 origin) reading and movement-based discovery.
/// [FA] خواندن موقعیت (mismatch vec3 origin) و کشف مبتنی بر حرکت.
mod position;

/// [EN] Re-export discover function as discover_local_player for external callers.
/// [FA] بازصادرات تابع discover به‌عنوان discover_local_player برای فراخوانندگان خارجی.
pub use local_player::discover as discover_local_player;

/// [EN] Re-export all position-related types and functions for external access.
/// [FA] بازصادرات تمام انواع و توابع مرتبط با موقعیت برای دسترسی خارجی.
pub use position::{
    collect_global_origin_bases, collect_player_candidates, collect_position_candidates,
    discover_by_movement, discover_offset as discover_position_offset, discover_player_and_offset,
    discover_position_live, is_usable_world_position, looks_like_view_aux, looks_like_world_origin,
    peek_vec3, prepare_walk_test, print_position_diagnostics, read_configured_global_position,
    read_global_world_at_rva, read_hw_entity_world_origin, read_runtime_world_vec3, read_vec3,
    read_view_aux, read_world_vec3, resolve_hw_local_player_position,
    scan_module_globals_for_movement, PlayerCandidate, PositionDiscovery, POS_OFFSET_CANDIDATES,
};

/// [EN] Re-export the connect function and GameEngine type.
/// [FA] بازصادرات تابع connect و نوع GameEngine.
pub use engine::{connect, GameEngine};

/// [EN] Game state structures shared between threads.
/// [FA] ساختارهای وضعیت بازی که بین threadها به اشتراک گذاشته می‌شوند.
pub mod state;

/// [EN] Re-export all state types for external access.
/// [FA] بازصادرت تمام انواع state برای دسترسی خارجی.
pub use state::{DebugSnapshot, GameState, ResolvedInfo, StatusDisplay};
