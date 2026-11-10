use lcd_studio::{Project, c_format::export_config, project::save_atomic};
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples");
    let dc162 = Project::dc162();
    let linear = Project::identity();
    let mut blue = dc162.clone();
    blue.name = "蓝通道中间调提亮示例".into();
    blue.current.generate(Some(2), 0.8, 1.0)?;
    for (name, project) in [
        ("dc162.json", dc162.clone()),
        ("linear.json", linear),
        ("blue-lift.json", blue),
    ] {
        save_atomic(&root.join(name), project.to_json()?.as_bytes())?;
    }
    save_atomic(
        &root.join("dc162-gamma.h"),
        export_config(&dc162.current, &dc162.current_color).as_bytes(),
    )?;
    println!(
        "Generated example JSON and C configuration in {}",
        root.display()
    );
    Ok(())
}
