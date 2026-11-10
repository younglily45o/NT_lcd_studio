use lcd_studio::{
    Gamma,
    pattern::{PatternRow, SampleRange, sample},
};

#[test]
fn identity_preserves_every_input_and_endpoint() {
    let gamma = Gamma::identity();
    for value in 0..=255 {
        assert_eq!(gamma.apply([value; 3]), [value; 3]);
    }
}

#[test]
fn interpolation_and_last_segment_use_actual_coordinates() {
    let mut gamma = Gamma::identity();
    gamma.red[1] = 16;
    gamma.red[32] = 250;
    assert_eq!(gamma.apply([4, 4, 4]), [8, 4, 4]);
    assert_eq!(gamma.apply([8, 8, 8]), [16, 8, 8]);
    assert_eq!(gamma.apply([255; 3]), [250, 255, 255]);
}

#[test]
fn disabled_gamma_bypasses_even_a_zero_table() {
    let mut gamma = Gamma::identity();
    gamma.generate(None, 1.0, 0.0).unwrap();
    gamma.enabled = false;
    assert_eq!(gamma.apply([19, 128, 250]), [19, 128, 250]);
}

#[test]
fn generator_is_channel_isolated_and_does_not_accumulate() {
    let mut gamma = Gamma::identity();
    gamma.generate(Some(2), 2.0, 1.0).unwrap();
    assert_eq!(gamma.apply([128; 3]), [128, 128, 64]);
    let first = gamma.clone();
    gamma.generate(Some(2), 2.0, 1.0).unwrap();
    assert_eq!(gamma, first);
}

#[test]
fn invalid_generator_leaves_original_untouched() {
    let mut gamma = Gamma::identity();
    let original = gamma.clone();
    for (channel, p, gain) in [
        (Some(3), 1.0, 1.0),
        (None, f32::NAN, 1.0),
        (None, 1.0, f32::INFINITY),
        (None, 0.0, 1.0),
    ] {
        assert!(gamma.generate(channel, p, gain).is_err());
        assert_eq!(gamma, original);
    }
}

#[test]
fn diagnostics_detect_reversal_and_merged_levels() {
    let mut gamma = Gamma::identity();
    gamma.red[12] = 0;
    gamma.blue.fill(0);
    let stats = gamma.stats();
    assert!(stats[0].reversals > 0);
    assert_eq!(stats[2].merged_levels, 255);
    assert_eq!(stats[2].clipped_low, 256);
    assert_eq!(stats[1].reversals, 0);
}

#[test]
fn dc162_snapshot_preserves_channel_difference() {
    let gamma = Gamma::dc162();
    assert_eq!(gamma.apply([128; 3]), [126, 128, 128]);
    assert_eq!(gamma.apply([255; 3]), [250, 255, 255]);
}

#[test]
fn patterns_include_range_endpoints_and_keep_primaries_separate() {
    assert_eq!(
        sample(PatternRow::Gray, 0.0, 16, SampleRange::Shadows),
        [0; 3]
    );
    assert_eq!(
        sample(PatternRow::Gray, 1.0, 16, SampleRange::Shadows),
        [32; 3]
    );
    assert_eq!(
        sample(PatternRow::Gray, 0.0, 32, SampleRange::Highlights),
        [223; 3]
    );
    assert_eq!(
        sample(PatternRow::Blue, 1.0, 256, SampleRange::Full),
        [0, 0, 255]
    );
    assert_eq!(
        sample(PatternRow::Colors, 0.0, 16, SampleRange::Full),
        [255; 3]
    );
}
