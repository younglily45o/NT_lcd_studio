#ifndef LCD_STUDIO_PROTOCOL_H
#define LCD_STUDIO_PROTOCOL_H
#include <stdint.h>
#include <stddef.h>

/* Override with the target board's ASCII identifier when compiling this core. */
#ifndef LCS_DEVICE_MODEL
#define LCS_DEVICE_MODEL "LCD_GENERIC"
#endif

typedef struct {
	uint8_t enabled, red[33], green[33], blue[33];
} LcsGamma;
typedef struct {
	uint8_t enabled;
	int8_t sat_ofs, int_ofs;
	uint8_t ycon, ccon;
} LcsColor;
typedef struct {
	LcsGamma gamma;
	LcsColor color;
} LcsSettings;
typedef struct {
	LcsSettings staged;
	uint32_t sequence;
	uint8_t active, mask;
} LcsEngine;
/* read_settings returns 1 on success, -2 for unrepresentable CC enable bits,
 * and 0 on unavailable/timeout. No callback may wait indefinitely. */
typedef struct {
	void *context;
	int (*read_settings)(void *, LcsSettings *);
	int (*apply_settings)(void *, const LcsSettings *);
	void (*emit)(void *, const char *);
} LcsIo;

void lcs_handle(LcsEngine *engine, int argc, const char *const *argv,
				const LcsIo *io);

#endif
