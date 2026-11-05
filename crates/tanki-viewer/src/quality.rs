// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! How hard the stage works the GPU. Weaker graphics draw a turning model at a lower resolution
//! and without anti-aliasing, then the full picture once it stops; nothing is drawn at all while
//! the stage is still.

use egui_wgpu::wgpu;

/// A level of drawing quality.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quality {
    /// Software rendering, OpenGL fallbacks and old integrated graphics.
    Economy,
    /// Integrated graphics.
    Balanced,
    /// Discrete graphics.
    High,
}

/// What a quality level means in numbers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tier {
    /// Anti-aliasing samples of the still picture (1 is none).
    pub samples: u32,
    /// Anti-aliasing samples while the model turns.
    pub moving_samples: u32,
    /// Resolution while the model turns, as a share of the full one.
    pub moving_scale: f32,
    /// The longest side of a picture in pixels.
    pub max_side: u32,
}

impl Quality {
    pub fn tier(self) -> Tier {
        match self {
            Quality::Economy => Tier {
                samples: 1,
                moving_samples: 1,
                moving_scale: 0.6,
                max_side: 1600,
            },
            Quality::Balanced => Tier {
                samples: 4,
                moving_samples: 1,
                moving_scale: 0.75,
                max_side: 2400,
            },
            Quality::High => Tier {
                samples: 4,
                moving_samples: 4,
                moving_scale: 1.0,
                max_side: 3200,
            },
        }
    }

    /// The level that suits the graphics adapter the window draws with.
    pub fn detect(adapter: &wgpu::AdapterInfo) -> Quality {
        if adapter.backend == wgpu::Backend::Gl {
            return Quality::Economy;
        }
        match adapter.device_type {
            wgpu::DeviceType::DiscreteGpu => Quality::High,
            wgpu::DeviceType::IntegratedGpu => Quality::Balanced,
            wgpu::DeviceType::Cpu | wgpu::DeviceType::VirtualGpu | wgpu::DeviceType::Other => {
                Quality::Economy
            }
        }
    }
}

/// What the user chose: a fixed level, or whatever suits the adapter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum QualityPreference {
    #[default]
    Auto,
    Economy,
    Balanced,
    High,
}

impl QualityPreference {
    pub const ALL: [QualityPreference; 4] = [Self::Auto, Self::Economy, Self::Balanced, Self::High];

    /// The stored value.
    pub fn code(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Economy => "economy",
            Self::Balanced => "balanced",
            Self::High => "high",
        }
    }

    /// Parse a stored value; nothing stored (or anything unknown) is automatic.
    pub fn from_pref(pref: Option<&str>) -> Self {
        match pref {
            Some("economy") => Self::Economy,
            Some("balanced") => Self::Balanced,
            Some("high") => Self::High,
            _ => Self::Auto,
        }
    }

    /// The level to draw with, given the one detected for the adapter.
    pub fn resolve(self, detected: Quality) -> Quality {
        match self {
            Self::Auto => detected,
            Self::Economy => Quality::Economy,
            Self::Balanced => Quality::Balanced,
            Self::High => Quality::High,
        }
    }
}

#[cfg(test)]
mod tests;
