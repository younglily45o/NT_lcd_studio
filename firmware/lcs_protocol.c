#include "lcs_protocol.h"

#include <stdio.h>
#include <string.h>

static uint32_t lcs_crc32(const uint8_t *data, size_t length)
{
	uint32_t crc = 0xFFFFFFFFu;
	size_t i;
	int bit;
	for (i = 0; i < length; i++) {
		crc ^= data[i];
		for (bit = 0; bit < 8; bit++)
			crc = (crc >> 1) ^ ((crc & 1u) ? 0xEDB88320u : 0u);
	}
	return ~crc;
}
static uint32_t lcs_settings_crc(const LcsSettings *settings)
{
	uint8_t bytes[105];
	bytes[0] = settings->gamma.enabled;
	memcpy(bytes + 1, settings->gamma.red, 33);
	memcpy(bytes + 34, settings->gamma.green, 33);
	memcpy(bytes + 67, settings->gamma.blue, 33);
	bytes[100] = settings->color.enabled;
	bytes[101] = (uint8_t)settings->color.sat_ofs;
	bytes[102] = (uint8_t)settings->color.int_ofs;
	bytes[103] = settings->color.ycon;
	bytes[104] = settings->color.ccon;
	return lcs_crc32(bytes, sizeof(bytes));
}
static size_t lcs_length(const char *text, size_t bound)
{
	size_t n = 0;
	if (!text)
		return bound;
	while (n < bound && text[n])
		n++;
	return n;
}
static int lcs_digit(char value)
{
	if (value >= '0' && value <= '9')
		return value - '0';
	if (value >= 'A' && value <= 'F')
		return value - 'A' + 10;
	if (value >= 'a' && value <= 'f')
		return value - 'a' + 10;
	return -1;
}
static int lcs_valid_model(void)
{
    const char *model = LCS_DEVICE_MODEL;
    size_t i, length = lcs_length(model, 33);
    if (!length || length > 32 || length != sizeof(LCS_DEVICE_MODEL) - 1)
        return 0;
    for (i = 0; i < length; i++) {
        char c = model[i];
        if (!((c >= 'A' && c <= 'Z') || (c >= 'a' && c <= 'z') ||
              (c >= '0' && c <= '9') || c == '_' || c == '-' || c == '.'))
            return 0;
    }
    return 1;
}
static int lcs_hex_u32(const char *text, uint32_t *out)
{
	size_t i;
	uint32_t value = 0;
	if (lcs_length(text, 9) != 8)
		return 0;
	for (i = 0; i < 8; i++) {
		int digit = lcs_digit(text[i]);
		if (digit < 0)
			return 0;
		value = (value << 4) | (uint32_t)digit;
	}
	*out = value;
	return 1;
}
static int lcs_decode(const char *text, uint8_t *out, size_t length)
{
	size_t i;
	if (lcs_length(text, length * 2 + 1) != length * 2)
		return 0;
	for (i = 0; i < length; i++) {
		int a = lcs_digit(text[i * 2]), b = lcs_digit(text[i * 2 + 1]);
		if (a < 0 || b < 0)
			return 0;
		out[i] = (uint8_t)(a * 16 + b);
	}
	return 1;
}
static void lcs_encode(const uint8_t *data, char *out, size_t length)
{
	static const char hex[] = "0123456789ABCDEF";
	size_t i;
	for (i = 0; i < length; i++) {
		out[i * 2] = hex[data[i] >> 4];
		out[i * 2 + 1] = hex[data[i] & 15];
	}
	out[length * 2] = 0;
}
static void lcs_encode_color(const LcsColor *color, char out[11])
{
	const uint8_t bytes[5] = {color->enabled, (uint8_t)color->sat_ofs,
							  (uint8_t)color->int_ofs, color->ycon,
							  color->ccon};
	lcs_encode(bytes, out, 5);
}
static int lcs_decode_color(const char *text, LcsColor *color)
{
	uint8_t bytes[5];
	if (!lcs_decode(text, bytes, 5) || bytes[0] > 1)
		return 0;
	color->enabled = bytes[0];
	color->sat_ofs =
		(int8_t)(bytes[1] < 128 ? (int)bytes[1] : (int)bytes[1] - 256);
	color->int_ofs =
		(int8_t)(bytes[2] < 128 ? (int)bytes[2] : (int)bytes[2] - 256);
	color->ycon = bytes[3];
	color->ccon = bytes[4];
	return 1;
}
static int lcs_same(const LcsSettings *a, const LcsSettings *b)
{
	return a->gamma.enabled == b->gamma.enabled &&
		   !memcmp(a->gamma.red, b->gamma.red, 33) &&
		   !memcmp(a->gamma.green, b->gamma.green, 33) &&
		   !memcmp(a->gamma.blue, b->gamma.blue, 33) &&
		   a->color.enabled == b->color.enabled &&
		   a->color.sat_ofs == b->color.sat_ofs &&
		   a->color.int_ofs == b->color.int_ofs &&
		   a->color.ycon == b->color.ycon && a->color.ccon == b->color.ccon;
}
static void lcs_message(const LcsIo *io, uint32_t sequence, const char *kind,
						const char *argument)
{
	char line[112];
	snprintf(line, sizeof(line), "@LCS2 %08lX %s %s", (unsigned long)sequence,
			 kind, argument);
	io->emit(io->context, line);
}
static void lcs_error(LcsEngine *engine, const LcsIo *io, uint32_t sequence,
					  const char *code)
{
	engine->active = 0;
	lcs_message(io, sequence, "err", code);
}
void lcs_handle(LcsEngine *engine, int argc, const char *const *argv,
					   const LcsIo *io)
{
	uint32_t sequence = 0, crc;
	const char *op;
	LcsSettings actual;
	char encoded[67], number[12];
	int result;
	if (!engine || !io || !io->read_settings || !io->apply_settings ||
		!io->emit)
		return;
	if (!argv || argc < 2 || argc > 3 || !lcs_hex_u32(argv[0], &sequence) ||
		lcs_length(argv[1], 16) >= 16) {
		lcs_error(engine, io, sequence, "FORMAT");
		return;
	}
	op = argv[1];
	if (!strcmp(op, "hello") && argc == 2) {
		engine->active = 0;
        if (!lcs_valid_model()) {
            lcs_error(engine, io, sequence, "MODEL");
            return;
        }
        lcs_message(io, sequence, "hello", LCS_DEVICE_MODEL " 2");
		return;
	}
	if (!strcmp(op, "get") && argc == 2) {
		engine->active = 0;
		result = io->read_settings(io->context, &actual);
		if (result != 1 || actual.gamma.enabled > 1 ||
			actual.color.enabled > 1) {
			lcs_error(engine, io, sequence, result == -2 ? "CC_FLAGS" : "READ");
			return;
		}
		lcs_message(io, sequence, "begin", actual.gamma.enabled ? "1" : "0");
		lcs_encode(actual.gamma.red, encoded, 33);
		lcs_message(io, sequence, "r", encoded);
		lcs_encode(actual.gamma.green, encoded, 33);
		lcs_message(io, sequence, "g", encoded);
		lcs_encode(actual.gamma.blue, encoded, 33);
		lcs_message(io, sequence, "b", encoded);
		lcs_encode_color(&actual.color, encoded);
		lcs_message(io, sequence, "cc", encoded);
		snprintf(number, sizeof(number), "%08lX",
				 (unsigned long)lcs_settings_crc(&actual));
		lcs_message(io, sequence, "end", number);
		return;
	}
	if (!strcmp(op, "begin") && argc == 3) {
		if (lcs_length(argv[2], 2) != 1 ||
			(argv[2][0] != '0' && argv[2][0] != '1')) {
			lcs_error(engine, io, sequence, "ENABLE");
			return;
		}
		memset(&engine->staged, 0, sizeof(engine->staged));
		engine->staged.gamma.enabled = (uint8_t)(argv[2][0] - '0');
		engine->sequence = sequence;
		engine->mask = 0;
		engine->active = 1;
		lcs_message(io, sequence, "ack", "begin");
		return;
	}
	if (argc != 3 || !engine->active || engine->sequence != sequence) {
		lcs_error(engine, io, sequence, "TRANSACTION");
		return;
	}
	if (!strcmp(op, "r") || !strcmp(op, "g") || !strcmp(op, "b")) {
		const uint8_t before = (uint8_t)(op[0] == 'r'	? 0
										 : op[0] == 'g' ? 1
														: 3);
		const uint8_t bit = (uint8_t)(op[0] == 'r' ? 1 : op[0] == 'g' ? 2 : 4);
		uint8_t *values = op[0] == 'r'	 ? engine->staged.gamma.red
						  : op[0] == 'g' ? engine->staged.gamma.green
										 : engine->staged.gamma.blue;
		if (engine->mask != before || !lcs_decode(argv[2], values, 33)) {
			lcs_error(engine, io, sequence, "CHANNEL");
			return;
		}
		engine->mask |= bit;
		lcs_message(io, sequence, "ack", op);
		return;
	}
	if (!strcmp(op, "cc")) {
		if (engine->mask != 7 ||
			!lcs_decode_color(argv[2], &engine->staged.color)) {
			lcs_error(engine, io, sequence, "COLOR");
			return;
		}
		engine->mask = 15;
		lcs_message(io, sequence, "ack", "cc");
		return;
	}
	if (!strcmp(op, "apply")) {
		if (engine->mask != 15 || !lcs_hex_u32(argv[2], &crc) ||
			crc != lcs_settings_crc(&engine->staged)) {
			lcs_error(engine, io, sequence, "CRC_OR_INCOMPLETE");
			return;
		}
		engine->active = 0;
		if (io->apply_settings(io->context, &engine->staged) != 1) {
			lcs_error(engine, io, sequence, "APPLY");
			return;
		}
		if (io->read_settings(io->context, &actual) != 1 ||
			!lcs_same(&actual, &engine->staged)) {
			lcs_error(engine, io, sequence, "READBACK");
			return;
		}
		lcs_message(io, sequence, "ack", "apply");
		return;
	}
	lcs_error(engine, io, sequence, "COMMAND");
}
