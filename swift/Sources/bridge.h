#ifndef SCREENCAP_BRIDGE_H
#define SCREENCAP_BRIDGE_H

#include <stdbool.h>
#include <stdint.h>

bool capture_screenshot(int64_t display_id, const char *output_path, uint8_t quality);
int32_t get_display_count(void);
int32_t copy_display_ids(uint32_t *buffer, int32_t max_count);
bool get_active_window(char **out_app_name, char **out_window_title, char **out_bundle_id);
bool start_native_event_listener(void (*callback)(uint32_t event_kind, double x, double y));
void stop_native_event_listener(void);
void free_bridge_string(char *value);

#endif
