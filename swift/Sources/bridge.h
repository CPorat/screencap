#ifndef SCREENCAP_BRIDGE_H
#define SCREENCAP_BRIDGE_H

#include <stdbool.h>
#include <stdint.h>

bool capture_screenshot(int64_t display_id, const char *output_path, uint8_t quality);
int32_t get_display_count(void);
int32_t copy_display_ids(uint32_t *buffer, int32_t max_count);

#endif
