use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Gamma {
    pub enabled: bool,
    #[serde(with = "array33")]
    pub red: [u8; 33],
    #[serde(with = "array33")]
    pub green: [u8; 33],
    #[serde(with = "array33")]
    pub blue: [u8; 33],
}

#[derive(Default, Clone, Debug)]
pub struct ChannelStats {
    pub reversals: usize,
    pub merged_levels: usize,
    pub clipped_low: usize,
    pub clipped_high: usize,
}

impl Gamma {
    pub fn identity() -> Self {
        let values = std::array::from_fn(input_position);
        Self {
            enabled: true,
            red: values,
            green: values,
            blue: values,
        }
    }

    pub fn dc162() -> Self {
        serde_json::from_str(include_str!("../assets/dc162.json"))
            .expect("embedded DC162 Gamma snapshot is valid")
    }

    pub fn channel(&self, channel: usize) -> &[u8; 33] {
        match channel {
            0 => &self.red,
            1 => &self.green,
            2 => &self.blue,
            _ => panic!("invalid RGB channel"),
        }
    }

    pub fn channel_mut(&mut self, channel: usize) -> &mut [u8; 33] {
        match channel {
            0 => &mut self.red,
            1 => &mut self.green,
            2 => &mut self.blue,
            _ => panic!("invalid RGB channel"),
        }
    }

    pub fn lut(&self) -> [[u8; 256]; 3] {
        std::array::from_fn(|channel| {
            std::array::from_fn(|value| {
                if self.enabled {
                    interpolate(self.channel(channel), value as u8)
                } else {
                    value as u8
                }
            })
        })
    }

    pub fn apply(&self, rgb: [u8; 3]) -> [u8; 3] {
        std::array::from_fn(|channel| {
            if self.enabled {
                interpolate(self.channel(channel), rgb[channel])
            } else {
                rgb[channel]
            }
        })
    }

    pub fn generate(
        &mut self,
        channel: Option<usize>,
        exponent: f32,
        gain: f32,
    ) -> Result<(), String> {
        if channel.is_some_and(|c| c > 2)
            || !exponent.is_finite()
            || !(0.3..=2.5).contains(&exponent)
            || !gain.is_finite()
            || !(0.0..=1.5).contains(&gain)
        {
            return Err("曲线指数需为 0.3~2.5，增益需为 0~1.5，并选择有效的 RGB 通道".into());
        }
        let values = std::array::from_fn(|i| {
            (255.0 * gain * (f32::from(input_position(i)) / 255.0).powf(exponent))
                .clamp(0.0, 255.0)
                .round() as u8
        });
        for c in 0..3 {
            if channel.is_none_or(|selected| selected == c) {
                *self.channel_mut(c) = values;
            }
        }
        Ok(())
    }

    pub fn stats(&self) -> [ChannelStats; 3] {
        self.lut().map(|table| ChannelStats {
            reversals: table.windows(2).filter(|p| p[1] < p[0]).count(),
            merged_levels: table.windows(2).filter(|p| p[0] == p[1]).count(),
            clipped_low: table.iter().filter(|&&value| value == 0).count(),
            clipped_high: table.iter().filter(|&&value| value == 255).count(),
        })
    }
}

pub fn input_position(index: usize) -> u8 {
    (index * 8).min(255) as u8
}

fn interpolate(table: &[u8; 33], value: u8) -> u8 {
    if value == 255 {
        return table[32];
    }
    let index = usize::from(value) / 8;
    let start = u32::from(input_position(index));
    let span = u32::from(input_position(index + 1)) - start;
    let offset = u32::from(value) - start;
    ((u32::from(table[index]) * (span - offset) + u32::from(table[index + 1]) * offset + span / 2)
        / span) as u8
}

mod array33 {
    use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error};
    pub fn serialize<S: Serializer>(values: &[u8; 33], serializer: S) -> Result<S::Ok, S::Error> {
        values.as_slice().serialize(serializer)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<[u8; 33], D::Error> {
        let values = Vec::<u8>::deserialize(deserializer)?;
        values.try_into().map_err(|v: Vec<u8>| {
            D::Error::custom(format!("Gamma 通道需要 33 个点，收到 {} 个", v.len()))
        })
    }
}
