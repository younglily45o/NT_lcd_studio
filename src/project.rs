use crate::{ColorControl, Gamma};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    sync::atomic::{AtomicU64, Ordering},
};

pub const MAX_FILE_SIZE: usize = 4 * 1024 * 1024;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "WireProject")]
pub struct Project {
    pub format_version: u32,
    pub name: String,
    pub source: String,
    pub baseline: Gamma,
    pub current: Gamma,
    pub baseline_color: ColorControl,
    pub current_color: ColorControl,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireProject {
    format_version: u32,
    name: String,
    source: String,
    baseline: Gamma,
    current: Gamma,
    baseline_color: Option<ColorControl>,
    current_color: Option<ColorControl>,
}
impl TryFrom<WireProject> for Project {
    type Error = String;
    fn try_from(wire: WireProject) -> Result<Self, String> {
        let (baseline_color, current_color) =
            match (wire.format_version, wire.baseline_color, wire.current_color) {
                (1, None, None) => (ColorControl::neutral(), ColorControl::neutral()),
                (2, Some(baseline), Some(current)) => (baseline, current),
                (1 | 2, _, _) => {
                    return Err(
                        "方案颜色字段不完整；格式 2 需要 baseline_color 和 current_color".into(),
                    );
                }
                (version, _, _) => {
                    return Err(format!("不支持的方案版本 {version}，支持格式 1 和 2"));
                }
            };
        let project = Self {
            format_version: 2,
            name: wire.name,
            source: wire.source,
            baseline: wire.baseline,
            current: wire.current,
            baseline_color,
            current_color,
        };
        project.validate()?;
        Ok(project)
    }
}
impl Project {
    pub fn dc162() -> Self {
        let mut project = Self::from_gamma(
            Gamma::dc162(),
            "DC162 · 调屏参考".into(),
            "DC162 Gamma 快照 · 颜色控制采用用户提供参数".into(),
        );
        project.baseline_color = ColorControl::reference();
        project.current_color = ColorControl::reference();
        project
    }
    pub fn identity() -> Self {
        Self::from_gamma(
            Gamma::identity(),
            "线性 Gamma".into(),
            "软件线性曲线".into(),
        )
    }
    pub fn from_gamma(gamma: Gamma, name: String, source: String) -> Self {
        Self {
            format_version: 2,
            name,
            source,
            baseline: gamma.clone(),
            current: gamma,
            baseline_color: ColorControl::neutral(),
            current_color: ColorControl::neutral(),
        }
    }
    pub fn baseline_rgb(&self, rgb: [u8; 3]) -> [u8; 3] {
        self.baseline_color.apply(self.baseline.apply(rgb))
    }
    pub fn current_rgb(&self, rgb: [u8; 3]) -> [u8; 3] {
        self.current_color.apply(self.current.apply(rgb))
    }
    pub fn from_json(source: &str) -> Result<Self, String> {
        if source.len() > MAX_FILE_SIZE {
            return Err("方案文件超过 4 MiB".into());
        }
        let project: Self =
            serde_json::from_str(source).map_err(|e| format!("JSON 方案无效：{e}"))?;
        project.validate()?;
        Ok(project)
    }
    pub fn to_json(&self) -> Result<String, String> {
        self.validate()?;
        serde_json::to_string_pretty(self)
            .map(|s| s + "\n")
            .map_err(|e| e.to_string())
    }
    fn validate(&self) -> Result<(), String> {
        if self.format_version != 2 {
            return Err(format!(
                "不支持的方案版本 {}，保存使用格式 2",
                self.format_version
            ));
        }
        if self.name.trim().is_empty()
            || self.name.chars().count() > 256
            || self.source.chars().count() > 4096
        {
            return Err("方案名须为 1~256 个字符，来源说明须不超过 4096 个字符".into());
        }
        Ok(())
    }
}
pub fn load_project(path: &Path) -> Result<Project, String> {
    let file = File::open(path).map_err(|e| format!("无法打开 {}：{e}", path.display()))?;
    if file.metadata().map_err(|e| e.to_string())?.len() > MAX_FILE_SIZE as u64 {
        return Err("文件超过 4 MiB".into());
    }
    let mut bytes = Vec::new();
    file.take((MAX_FILE_SIZE + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > MAX_FILE_SIZE {
        return Err("文件超过 4 MiB".into());
    }
    let source = String::from_utf8(bytes).map_err(|_| "文件需要使用 UTF-8 编码")?;
    let source = source.trim_start_matches('\u{feff}');
    match path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "json" => Project::from_json(source),
        "c" | "h" => {
            let (gamma, color) = crate::c_format::import_config(source)?;
            let mut project = Project::from_gamma(
                gamma,
                path.file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
                path.display().to_string(),
            );
            project.baseline_color = color;
            project.current_color = color;
            Ok(project)
        }
        _ => Err("请选择 JSON 方案或包含 Gamma / 颜色控制定义的 C/H 文件".into()),
    }
}

pub fn save_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let filename = path.file_name().ok_or("保存路径需要包含文件名")?;
    let temp = path.with_file_name(format!(
        ".{}.{}-{}.tmp",
        filename.to_string_lossy(),
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temp)
        .map_err(|e| format!("无法创建临时文件：{e}"))?;
    let result = (|| {
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        // std::fs::rename replaces the destination atomically on the same filesystem,
        // including Windows' MoveFileExW(REPLACE_EXISTING) implementation.
        std::fs::rename(&temp, path)
    })();
    if let Err(error) = result {
        let _ = std::fs::remove_file(&temp);
        return Err(format!("保存失败，原文件未被删除：{error}"));
    }
    Ok(())
}
