use lcd_studio::{
    ColorControl, Gamma,
    c_format::{export_config, import_config},
};

#[test]
fn user_commented_positional_color_control_imports_without_gamma() {
    let source = "static cc_t color_control_config = { /*.enable=*/ 1,//0\n /*.sat_ofs=*/10, /*.int_ofs=*/0, /*.ycon=*/128, /*.ccon=*/135 };";
    let (gamma, color) = import_config(source).unwrap();
    assert_eq!(gamma, Gamma::identity());
    assert_eq!(color, ColorControl::reference());
}
#[test]
fn combined_c_roundtrip_preserves_signed_offsets_and_both_enable_bits() {
    let mut gamma = Gamma::dc162();
    gamma.enabled = false;
    let color = ColorControl {
        enabled: false,
        sat_ofs: -128,
        int_ofs: 127,
        ycon: 0,
        ccon: 255,
    };
    let source = export_config(&gamma, &color);
    assert!(source.contains("static cc_t color_control_config"));
    assert_eq!(import_config(&source).unwrap(), (gamma, color));
}
#[test]
fn color_import_rejects_ambiguous_out_of_range_and_unsupported_definitions() {
    let good = "static cc_t color_control_config = {.enable=1,.sat_ofs=-20,.int_ofs=0,.ycon=128,.ccon=135};";
    for source in [
        format!("{good}\n{good}"),
        good.replace("-20", "128"),
        good.replace("-20", "-129"),
        good.replace("ycon=128", "ycon=256"),
        good.replace("ccon=135", "ccon=-1"),
        good.replace(".int_ofs=0", ".int_ofs=2+3"),
        good.replace(".enable=1", ".enable=2"),
        good.replace(".int_ofs=0", ".int_ofs=010"),
        good.replace(".int_ofs", ".unknown"),
        good.replace(".int_ofs=0", ".sat_ofs=0"),
    ] {
        assert!(import_config(&source).is_err(), "accepted {source}");
    }
}
#[test]
fn gamma_only_files_keep_neutral_color_and_negative_gamma_is_invalid() {
    let gamma = Gamma::identity();
    let source = lcd_studio::c_format::export_c(&gamma);
    assert_eq!(
        import_config(&source).unwrap(),
        (gamma, ColorControl::neutral())
    );
    assert!(import_config(&source.replacen("0x08", "-1", 1)).is_err());
}
