//! One renderer per settings category.

pub(super) mod appearance;
pub(super) mod library;
pub(super) mod mixbus;
pub(super) mod output;
pub(super) mod playback;
pub(super) mod spatial;
pub(super) mod tools;

pub(super) use appearance::render_appearance_category;
pub(super) use library::render_library_category;
pub(super) use mixbus::render_mixbus_category;
pub(super) use output::render_output_category;
pub(super) use playback::render_playback_category;
pub(super) use spatial::render_spatial_category;
pub(super) use tools::render_tools_category;
