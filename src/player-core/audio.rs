use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub enum EffectKind {
    Gain,
    GraphicEqualizer,
    Equalizer,
    Compressor,
    Reverb,
}

pub const EFFECTS: [EffectKind; 5] = [
    EffectKind::Gain,
    EffectKind::GraphicEqualizer,
    EffectKind::Equalizer,
    EffectKind::Compressor,
    EffectKind::Reverb,
];
pub const MAX_EFFECTS: usize = 16;

pub struct Parameter {
    pub name: &'static str,
    pub min: f32,
    pub max: f32,
    pub default: f32,
    pub step: f32,
    pub unit: &'static str,
}
const fn param(
    name: &'static str,
    min: f32,
    max: f32,
    default: f32,
    step: f32,
    unit: &'static str,
) -> Parameter {
    Parameter {
        name,
        min,
        max,
        default,
        step,
        unit,
    }
}
const fn band(name: &'static str) -> Parameter {
    param(name, -12., 12., 0., 0.5, "dB")
}
pub const FREQUENCIES: [f32; 10] = [
    31., 63., 125., 250., 500., 1000., 2000., 4000., 8000., 16000.,
];
pub const PRESETS: [(&str, [f32; 10]); 4] = [
    ("平直", [0.; 10]),
    ("流行", [-1., 1., 2., 2., 0., -1., 1., 3., 3., 1.]),
    ("摇滚", [5., 3., 1., -1., -2., 0., 2., 3., 4., 4.]),
    ("人声", [-3., -2., -1., 0., 2., 4., 4., 2., 0., -2.]),
];
impl EffectKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Gain => "增益",
            Self::GraphicEqualizer => "均衡器",
            Self::Equalizer => "参数均衡器",
            Self::Compressor => "动态压缩",
            Self::Reverb => "混响",
        }
    }
    pub fn parameters(self) -> &'static [Parameter] {
        match self {
            Self::Gain => const { &[param("增益", -24., 12., 0., 0.1, "dB")] },
            Self::GraphicEqualizer => {
                const {
                    &[
                        band("31 Hz"),
                        band("63 Hz"),
                        band("125 Hz"),
                        band("250 Hz"),
                        band("500 Hz"),
                        band("1000 Hz"),
                        band("2000 Hz"),
                        band("4000 Hz"),
                        band("8000 Hz"),
                        band("16000 Hz"),
                        param("低音增强", 0., 12., 0., 0.5, "dB"),
                        param("高音增强", 0., 12., 0., 0.5, "dB"),
                    ]
                }
            }
            Self::Equalizer => {
                const {
                    &[
                        param("中心频率", 20., 20000., 1000., 1., "Hz"),
                        param("Q 值", 0.1, 18., 1., 0.1, ""),
                        param("增益", -24., 24., 0., 0.1, "dB"),
                    ]
                }
            }
            Self::Compressor => {
                const {
                    &[
                        param("阈值", -100., 0., -24., 1., "dB"),
                        param("拐点宽度", 0., 40., 30., 1., "dB"),
                        param("压缩比", 1., 20., 4., 0.1, ""),
                        param("启动时间", 0., 1., 0.003, 0.001, "s"),
                        param("释放时间", 0., 1., 0.25, 0.001, "s"),
                    ]
                }
            }
            Self::Reverb => {
                const {
                    &[
                        param("混响比例", 0., 1., 0.2, 0.01, ""),
                        param("衰减时间", 0.1, 6., 1.5, 0.1, "s"),
                        param("高频截止", 500., 20000., 6000., 100., "Hz"),
                    ]
                }
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct Effect {
    pub enabled: bool,
    pub kind: EffectKind,
    pub values: Vec<f32>,
}
impl Effect {
    pub fn new(kind: EffectKind) -> Self {
        Self {
            enabled: true,
            kind,
            values: kind.parameters().iter().map(|p| p.default).collect(),
        }
    }
    fn normalize(&mut self) {
        self.values = self
            .kind
            .parameters()
            .iter()
            .enumerate()
            .map(|(i, p)| {
                self.values
                    .get(i)
                    .copied()
                    .filter(|v| v.is_finite())
                    .unwrap_or(p.default)
                    .clamp(p.min, p.max)
            })
            .collect();
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct LocalDsp {
    pub enabled: bool,
    pub chain: Vec<Effect>,
}
impl Default for LocalDsp {
    fn default() -> Self {
        Self {
            enabled: true,
            chain: vec![],
        }
    }
}
impl LocalDsp {
    pub fn normalize(&mut self) {
        self.chain.truncate(MAX_EFFECTS);
        for effect in &mut self.chain {
            effect.normalize();
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
pub enum Layout {
    #[default]
    Source,
    Mono,
    Stereo,
    Surround51,
    Surround71,
}
impl Layout {
    pub fn channels(self, source: u16) -> u16 {
        match self {
            Self::Source => source,
            Self::Mono => 1,
            Self::Stereo => 2,
            Self::Surround51 => 6,
            Self::Surround71 => 8,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Source => "跟随音源",
            Self::Mono => "单声道",
            Self::Stereo => "立体声",
            Self::Surround51 => "5.1",
            Self::Surround71 => "7.1",
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
pub struct Routing {
    pub layout: Layout,
    pub expand_stereo: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct Processing {
    pub dsp: LocalDsp,
    pub routing: Routing,
    pub auto_volume: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct OutputStatus {
    pub max_channels: Option<u16>,
    pub source_channels: Option<u16>,
    pub output_channels: Option<u16>,
    pub limited: bool,
}
