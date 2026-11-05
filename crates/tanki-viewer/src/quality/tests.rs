// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

fn adapter(backend: wgpu::Backend, device_type: wgpu::DeviceType) -> wgpu::AdapterInfo {
    wgpu::AdapterInfo {
        backend,
        device_type,
        ..wgpu::AdapterInfo::new(device_type, backend)
    }
}

#[test]
fn the_adapter_decides_the_automatic_level() {
    use wgpu::{Backend, DeviceType};
    let cases = [
        (Backend::Metal, DeviceType::DiscreteGpu, Quality::High),
        (Backend::Dx12, DeviceType::IntegratedGpu, Quality::Balanced),
        (Backend::Dx12, DeviceType::Cpu, Quality::Economy),
        (Backend::Vulkan, DeviceType::VirtualGpu, Quality::Economy),
        // OpenGL is the fallback of old drivers: always the lightest level.
        (Backend::Gl, DeviceType::DiscreteGpu, Quality::Economy),
    ];
    for (backend, device_type, expected) in cases {
        assert_eq!(
            Quality::detect(&adapter(backend, device_type)),
            expected,
            "{backend:?} {device_type:?}"
        );
    }
}

#[test]
fn lighter_levels_never_draw_more_than_heavier_ones() {
    let tiers: Vec<Tier> = [Quality::Economy, Quality::Balanced, Quality::High]
        .map(Quality::tier)
        .to_vec();
    for pair in tiers.windows(2) {
        assert!(pair[0].samples <= pair[1].samples);
        assert!(pair[0].moving_scale <= pair[1].moving_scale);
        assert!(pair[0].max_side <= pair[1].max_side);
    }
    for tier in tiers {
        assert!(tier.moving_samples <= tier.samples);
        assert!(tier.moving_scale > 0.0 && tier.moving_scale <= 1.0);
    }
}

#[test]
fn preferences_round_trip_and_unknown_values_are_automatic() {
    for pref in QualityPreference::ALL {
        assert_eq!(QualityPreference::from_pref(Some(pref.code())), pref);
    }
    assert_eq!(QualityPreference::from_pref(None), QualityPreference::Auto);
    assert_eq!(
        QualityPreference::from_pref(Some("ultra")),
        QualityPreference::Auto
    );
    assert_eq!(
        QualityPreference::Auto.resolve(Quality::Balanced),
        Quality::Balanced
    );
    assert_eq!(
        QualityPreference::Economy.resolve(Quality::High),
        Quality::Economy
    );
}
