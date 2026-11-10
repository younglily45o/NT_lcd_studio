#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PatternRow {
    Gray,
    Red,
    Green,
    Blue,
    Colors,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SampleRange {
    Full,
    Shadows,
    Highlights,
}
impl PatternRow {
    pub const ALL: [Self; 5] = [Self::Gray, Self::Red, Self::Green, Self::Blue, Self::Colors];
    pub fn label(self) -> &'static str {
        match self {
            Self::Gray => "灰阶",
            Self::Red => "R",
            Self::Green => "G",
            Self::Blue => "B",
            Self::Colors => "彩色",
        }
    }
}

impl SampleRange {
    pub fn bounds(self) -> (u8, u8) {
        match self {
            Self::Full => (0, 255),
            Self::Shadows => (0, 32),
            Self::Highlights => (223, 255),
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Full => "完整色阶",
            Self::Shadows => "暗部 0~32",
            Self::Highlights => "高光 223~255",
        }
    }
}

pub fn sample(row: PatternRow, position: f32, steps: usize, range: SampleRange) -> [u8; 3] {
    let position = if position.is_finite() {
        position.clamp(0.0, 1.0)
    } else {
        0.0
    };
    if row == PatternRow::Colors {
        const COLORS: [[u8; 3]; 8] = [
            [255, 255, 255],
            [255, 255, 0],
            [0, 255, 255],
            [0, 255, 0],
            [255, 0, 255],
            [255, 0, 0],
            [0, 0, 255],
            [0, 0, 0],
        ];
        return COLORS[((position * 8.0) as usize).min(7)];
    }
    let steps = steps.clamp(2, 256);
    let index = ((position * steps as f32) as usize).min(steps - 1);
    let (low, high) = range.bounds();
    let value =
        (f32::from(low) + f32::from(high - low) * index as f32 / (steps - 1) as f32).round() as u8;
    match row {
        PatternRow::Gray => [value; 3],
        PatternRow::Red => [value, 0, 0],
        PatternRow::Green => [0, value, 0],
        PatternRow::Blue => [0, 0, value],
        PatternRow::Colors => unreachable!(),
    }
}
