//! Approximate RGB preview for the SDK's cc_t controls; not an IDE emulator.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColorControl {
    pub enabled: bool,
    pub sat_ofs: i8,
    pub int_ofs: i8,
    pub ycon: u8,
    pub ccon: u8,
}
impl ColorControl {
    pub const fn neutral() -> Self {
        Self {
            enabled: true,
            sat_ofs: 0,
            int_ofs: 0,
            ycon: 128,
            ccon: 128,
        }
    }
    pub const fn reference() -> Self {
        Self {
            sat_ofs: 10,
            ccon: 135,
            ..Self::neutral()
        }
    }
    pub fn apply(&self, rgb: [u8; 3]) -> [u8; 3] {
        if !self.enabled {
            return rgb;
        }
        let [r, g, b] = rgb.map(f64::from);
        let y = 0.299 * r + 0.587 * g + 0.114 * b;
        let output_y = (y - 128.0) * f64::from(self.ycon) / 128.0 + 128.0 + f64::from(self.int_ofs);
        let saturation =
            (1.0 + f64::from(self.sat_ofs) / 128.0).max(0.0) * f64::from(self.ccon) / 128.0;
        [r, g, b].map(|channel| {
            (output_y + (channel - y) * saturation)
                .round()
                .clamp(0.0, 255.0) as u8
        })
    }
}
impl Default for ColorControl {
    fn default() -> Self {
        Self::neutral()
    }
}
