use crate::Gamma;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    sync::atomic::{AtomicU64, Ordering},
};

pub const MAX_FILE_SIZE: usize = 4 * 1024 * 1024;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Project {
    pub format_version: u32,
    pub name: String,
    pub source: String,
    pub baseline: Gamma,
    pub current: Gamma,
}
impl Project {
    pub fn dc162() -> Self {
        Self::from_gamma(
            Gamma::dc162(),
            "DC162 · 原始 Gamma".into(),
            "DC162 panel.c · 2026-11-10 快照".into(),
        )
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
            format_version: 1,
            name,
            source,
            baseline: gamma.clone(),
            current: gamma,
        }
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
        if self.format_version != 1 {
            return Err(format!(
                "不支持的方案版本 {}，当前支持版本 1",
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
        "c" | "h" => Ok(Project::from_gamma(
            crate::c_format::import_c(source)?,
            path.file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            path.display().to_string(),
        )),
        _ => Err("请选择 JSON 方案或包含 gamma_config 定义的 C/H 文件".into()),
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
