/// Animation curve applied to overlay slide progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Easing {
    Linear,
    EaseIn,
    #[default]
    EaseOut,
    EaseInOut,
}

impl Easing {
    /// Pure function mapping linear progress `t ∈ [0, 1]` to eased output.
    pub fn apply(self, t: f32) -> f32 {
        match self {
            Self::Linear => t,
            Self::EaseIn => t * t,
            Self::EaseOut => (1.0 - t).mul_add(-(1.0 - t), 1.0),
            Self::EaseInOut => (2.0 * t * t).mul_add(-t, 3.0 * t * t),
        }
    }
}
