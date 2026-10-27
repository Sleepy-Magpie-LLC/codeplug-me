/*
 * codeplugme_core.h — C ABI of the codeplugme-core Rust static library.
 *
 * Hand-written to match core/src/ffi.rs (keep the two in sync).
 *
 * Conventions:
 *  - Every `char *` returned by these functions is heap-allocated and MUST be
 *    released with codeplugme_string_free().
 *  - String-returning functions return NULL on failure; status-returning
 *    functions return 0 on success and -1 on failure. On failure, *err_out
 *    (when err_out is non-NULL) is set to a heap-allocated error message the
 *    caller must free with codeplugme_string_free().
 *  - All input strings are NUL-terminated UTF-8.
 *  - Every entry point carries the `codeplugme_` prefix. They are model-agnostic;
 *    where the model matters they take or report a model id string (see
 *    codeplugme_models_json).
 */

#ifndef CODEPLUGME_CORE_H
#define CODEPLUGME_CORE_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Progress callback: (blocks_done, blocks_total, user_context). Invoked
 * synchronously on the thread that called the FFI function. */
typedef void (*codeplugme_progress_cb)(size_t done, size_t total, void *user);

/* Free a string previously returned by any function below. NULL is a no-op. */
void codeplugme_string_free(char *s);

/* Exact byte length of a full codeplug image for the AnyTone. Deprecated in
 * favour of codeplugme_codeplug_size_for(); retained because the app's file
 * compatibility check predates model awareness. Do not use it to size a QYT
 * image: pass "qyt-kt5000" to codeplugme_codeplug_size_for() instead. */
size_t codeplugme_codeplug_size(void);

/* Every radio this build supports, as a JSON array:
 * [{"id","display_name","image_size","capabilities":{...}}, ...].
 * "capabilities" is {"sections":[...],"channel_capacity","channel_name_length",
 * "supports_dmr","tone_encoding","power_options":[{"tag","label"}, ...],
 * "bandwidth_options":[{"tag","label"}, ...]}. */
char *codeplugme_models_json(char **err_out);

/* Identify which radio the image file `bin_path` belongs to; returns its model
 * id, or NULL when the file matches no supported radio. */
char *codeplugme_detect_model(const char *bin_path, char **err_out);

/* Exact byte length of a full codeplug image for `model`. Returns 0 and sets
 * *err_out when the model id is unknown. */
size_t codeplugme_codeplug_size_for(const char *model, char **err_out);

/* Enumerate serial ports as a JSON array:
 * [{"name","vid","pid","product","likely_radio","model"}, ...], where "model"
 * is a model id string or null and "likely_radio" is simply (model != null). */
char *codeplugme_ports_json(char **err_out);

/* Handshake with the radio on `port` and return {"model","identity"}. `model`
 * selects which protocol to speak and may be NULL to infer it from the port's
 * USB identity. Inference fails (it does not guess) when the port's VID/PID
 * matches no known radio; pass a model explicitly for those. */
char *codeplugme_identify_json(const char *port, const char *model,
                            char **err_out);

/* Identity string only, for callers that predate codeplugme_identify_json. The
 * model is always inferred, so on a port whose USB identity names no radio
 * this now fails rather than guessing: more than one model is supported. Use
 * codeplugme_identify_json() with an explicit model. */
char *codeplugme_identify(const char *port, char **err_out);

/* Read the full codeplug from the radio into the file `out_path`. `model` may
 * be NULL to infer it from the port's USB identity, with the same no-guessing
 * rule as codeplugme_identify_json(). */
int32_t codeplugme_backup(const char *port, const char *model,
                       const char *out_path, codeplugme_progress_cb progress,
                       void *user, char **err_out);

/* Write the codeplug file `in_path` back to the radio. The model is taken from
 * the FILE, and the connected radio's identity must match it or nothing is
 * written -- the check runs before the first write block is emitted, so a QYT
 * image can never reach an AnyTone or the reverse. Also refuses unless `force`
 * is true. The AnyTone path reads every written block back and verifies it; the
 * QYT reboots after a transfer, so its verification is a separate later
 * session. */
int32_t codeplugme_restore(const char *port, const char *in_path, bool force,
                        codeplugme_progress_cb progress, void *user,
                        char **err_out);

/* Parse a codeplug .bin offline; returns the tagged envelope
 * {"model","display_name","capabilities":{...},<model key>:{...}}.
 *
 * The model key is "anytone" for the AnyTone, whose value holds the active
 * records: {"channels":[...],"zones":[...],"scan_lists":[...],"contacts":[...],
 * "group_lists":[...],"radio_ids":[...],"aprs":{...}}.
 *
 * It is "qyt" for the QYT KT-5000, whose value is
 * {"channels":[...],"settings":{...},"bands":[...],"model_tag":"..."}. A QYT
 * channel carries index, name, rx_frequency_hz, tx_frequency_hz (null when the
 * channel is receive-only), rx_tone, tx_tone, the decoded flag fields, and
 * flags_raw (record bytes 12-15, exposed because two of their bits are not
 * understood yet). A tone is tagged: {"kind":"none"},
 * {"kind":"ctcss","hz_tenths":885}, or {"kind":"dcs","code":23,"inverted":false}.
 *
 * See the Rust models for each record's fields. */
char *codeplugme_dump_json(const char *bin_path, char **err_out);

/* Apply a batch of edits (JSON) to `bin_in`, writing `bin_out` (the paths may
 * be equal). The edits object is either the tagged envelope
 * {"anytone":{...}} / {"qyt":{...}} or, for backward compatibility, a bare spec
 * with no model key. A batch carrying the other model's key is rejected rather
 * than silently applied as a no-op.
 *
 * The QYT spec carries "channels"/"add_channels"/"remove_channels"/
 * "move_channels" with the same shape as below, plus "settings" (one object of
 * optional fields) and "bands" (update-by-index over the four records; no add,
 * remove, or move). A QYT channel edit accepts name, rx_frequency_hz,
 * tx_frequency_hz (explicit null makes the channel receive-only), rx_tone, and
 * tx_tone.
 * The spec carries, for each entity family, an update array
 * ("channels"/"zones"/"contacts"/"group_lists"/"radio_ids", each element an
 * index plus optional fields), an "add_*" array (field objects with no index;
 * a record is created and the fields applied), and a "remove_*" array of
 * indices. Operations run update → remove → add. Channel fields include name,
 * rx_frequency_hz, tx_frequency_hz, mode, power, bandwidth, color_code,
 * time_slot, contact_index, radio_id_index, group_list_index. See ffi.rs for
 * the full schema. Edits are verified by re-parsing the output. */
int32_t codeplugme_apply_edits(const char *bin_in, const char *edits_json,
                            const char *bin_out, char **err_out);

/* Repeater import (see core/src/geo). Fetch repeaters near a location for the
 * codeplug at bin_path: query_json is {"lat","lon","radius_km","country",
 * "state","sources":["hearham"|"radioid"|"repeaterbook"],"repeaterbook":
 * {"token","contact"}}. Returns a JSON array of repeaters, nearest first, each
 * with "duplicate" set when bin_path already has that channel. Blocks on the
 * network. Free with codeplugme_string_free. */
char *codeplugme_geo_fetch_json(const char *query_json, const char *bin_path,
                             const char *cache_dir, char **err_out);

/* How many repeaters may be picked for bin_path in mode "clean" or "merge":
 * {"available","max_selectable","zone_limit","scan_list_limit",
 * "blocked_reason"}. Free with codeplugme_string_free. */
char *codeplugme_geo_capacity_json(const char *bin_path, const char *mode,
                                char **err_out);

/* Write bin_in plus exactly the repeaters in repeaters_json (a subset of a
 * fetch result) to bin_out. options_json is {"mode":"clean"|"merge",
 * "zone_name"}. Too many repeaters is an error, never truncated. */
int32_t codeplugme_geo_build(const char *bin_in, const char *repeaters_json,
                          const char *options_json, const char *bin_out,
                          char **err_out);

#ifdef __cplusplus
}
#endif

#endif /* CODEPLUGME_CORE_H */
