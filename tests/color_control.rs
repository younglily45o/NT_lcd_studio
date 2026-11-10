use lcd_studio::{ColorControl, Project};

#[test]
fn neutral_and_disabled_color_control_preserve_rgb() {
    let neutral = ColorControl::neutral();
    let disabled = ColorControl {
        enabled: false,
        sat_ofs: 127,
        int_ofs: -128,
        ycon: 255,
        ccon: 0,
    };
    for value in 0..=255 {
        for rgb in [[value; 3], [value, 40, 190], [230, value, 17]] {
            assert_eq!(neutral.apply(rgb), rgb);
            assert_eq!(disabled.apply(rgb), rgb);
        }
    }
}

#[test]
fn intensity_and_luma_contrast_change_gray_levels() {
    let mut color = ColorControl::neutral();
    color.int_ofs = 10;
    assert_eq!(color.apply([96; 3]), [106; 3]);
    assert_eq!(color.apply([250; 3]), [255; 3]);
    color.int_ofs = -20;
    assert_eq!(color.apply([8; 3]), [0; 3]);
    color.int_ofs = 0;
    color.ycon = 192;
    assert_eq!(color.apply([96; 3]), [80; 3]);
    assert_eq!(color.apply([160; 3]), [176; 3]);
    color.ycon = 0;
    assert_eq!(color.apply([20; 3]), [128; 3]);
}

#[test]
fn chroma_gain_and_saturation_adjust_color_without_tinting_gray() {
    let mut color = ColorControl::neutral();
    color.ccon = 64;
    assert_eq!(color.apply([100, 40, 20]), [78, 48, 38]);
    color.ccon = 0;
    assert_eq!(color.apply([100, 40, 20]), [56; 3]);
    color.ccon = 128;
    color.sat_ofs = -128;
    assert_eq!(color.apply([100, 40, 20]), [56; 3]);
    color.sat_ofs = 64;
    assert_eq!(color.apply([100, 40, 20]), [122, 32, 2]);
    assert_eq!(color.apply([100; 3]), [100; 3]);
}

#[test]
fn project_preview_combines_gamma_and_current_color_without_moving_baseline() {
    let mut project = Project::identity();
    project.current.red[16] = 64;
    project.current_color.int_ofs = 10;
    assert_eq!(project.current_rgb([128; 3]), [74, 138, 138]);
    assert_eq!(project.baseline_rgb([128; 3]), [128; 3]);
}

#[test]
fn old_json_projects_migrate_to_neutral_color_and_keep_gamma() {
    let original = Project::dc162();
    let mut value = serde_json::to_value(&original).unwrap();
    value["format_version"] = 1.into();
    value.as_object_mut().unwrap().remove("baseline_color");
    value.as_object_mut().unwrap().remove("current_color");
    let migrated = Project::from_json(&value.to_string()).unwrap();
    assert_eq!(migrated.format_version, 2);
    assert_eq!(migrated.current, original.current);
    assert_eq!(migrated.current_color, ColorControl::neutral());
    assert_eq!(
        migrated.current_rgb([100, 40, 20]),
        original.current.apply([100, 40, 20])
    );
}

#[test]
fn new_json_requires_both_color_states_and_rejects_signed_overflow() {
    let good = serde_json::to_value(Project::identity()).unwrap();
    for field in ["baseline_color", "current_color"] {
        let mut value = good.clone();
        value.as_object_mut().unwrap().remove(field);
        assert!(Project::from_json(&value.to_string()).is_err());
    }
    for (field, bad) in [
        ("sat_ofs", 128),
        ("sat_ofs", -129),
        ("int_ofs", 128),
        ("int_ofs", -129),
        ("ycon", 256),
        ("ccon", -1),
    ] {
        let mut value = good.clone();
        value["current_color"][field] = bad.into();
        assert!(
            Project::from_json(&value.to_string()).is_err(),
            "{field}={bad}"
        );
    }
}
