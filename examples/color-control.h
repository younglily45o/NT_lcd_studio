/* User-provided color controls. Requires SDK display.h cc_t.
 * sat_ofs / int_ofs are INT8 (-128..127), ycon / ccon are UINT8.
 * A CC-only import uses linear Gamma in LCD Studio.
 */
static cc_t color_control_config = {
    .enable = 1,
    .sat_ofs = 10,
    .int_ofs = 0,
    .ycon = 128,
    .ccon = 135
};
