// Stub implementation of the WASM engine module for builds without the compiled WASM
// This allows the build to succeed even when the Rust WASM module hasn't been compiled

const stubError = () => {
  throw new Error('WASM engine module not available. Music Galaxy feature requires the Rust WASM module to be built.');
};

export const create_artist_map_ctx = stubError;
export const decode_and_record_packed_artist_positions = stubError;
export const get_all_artist_data = stubError;
export const handle_new_position = stubError;
export const handle_received_artist_names = stubError;
export const on_music_finished_playing = stubError;
export const get_connections_buffer_ptr = stubError;
export const get_connections_buffer_length = stubError;
export const get_memory = stubError;
export const get_connections_color_buffer_ptr = stubError;
export const get_connections_color_buffer_length = stubError;
export const handle_artist_relationship_data = stubError;
export const handle_set_highlighted_artists = stubError;
export const handle_artist_manual_play = stubError;
export const get_connections_for_artists = stubError;
export const transition_to_orbit_mode = stubError;
export const force_render_artist_label = stubError;
export const set_quality = stubError;
export const play_last_artist = stubError;
export const get_artist_colors_buffer_ptr = stubError;
export const get_artist_colors_buffer_length = stubError;
