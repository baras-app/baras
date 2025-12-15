use crate::log_ids::effect_type_id;
use crate::parse_line;
use hashbrown::HashMap;
use std::fs;
use std::fs::File;
use std::io::{BufRead, BufReader, Result};
use std::path::{Path, PathBuf};
use time::format_description::well_known::Iso8601;
use time::{Date, PrimitiveDateTime, Time};

pub struct LogFileMetaData {
    pub path: PathBuf,
    pub filename: String,
    pub date: Date,
    pub created_at: PrimitiveDateTime,
    pub character_name: Option<String>,
    pub session_number: u32,
    pub is_empty: bool,
}

impl LogFileMetaData {
    pub fn display_name(&self) -> String {
        match &self.character_name {
            Some(name) => format!("{} - {} - Session {}", self.date, name, self.session_number),
            None => format!("{} - Unknown - Session {}", self.date, self.session_number),
        }
    }
}

#[derive(Default)]
pub struct LogFileIndex {
    entries: HashMap<PathBuf, LogFileMetaData>,
    session_counts: HashMap<(String, Date), u32>,
}

impl LogFileIndex {
    pub fn new() -> Self {
        Self {
            ..Default::default()
        }
    }

    pub fn build_index(dir: &Path) -> Result<Self> {
        let mut index = Self::new();

        if !dir.exists() {
            println!("provided directory path not found");
            return Ok(index);
        }
        //get all files starting with combat and sort
        let mut files: Vec<_> = fs::read_dir(dir)?
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .file_name()
                    .and_then(|f| f.to_str())
                    .map(|f| f.starts_with("combat_"))
                    .unwrap_or(false)
            })
            .collect();
        files.sort_by_key(|e| e.file_name());
        for entry in files {
            let path = entry.path();
            if let Some(log_file) = index.create_entry(&path) {
                index.add_entry(log_file);
            }
        }
        Ok(index)
    }

    pub fn create_entry(&mut self, path: &Path) -> Option<LogFileMetaData> {
        let filename = path.file_name()?.to_str()?.to_string();
        let (date, created_at) = parse_log_filename(&filename)?;
        let metatdata = fs::metadata(path).ok()?;
        let is_empty = metatdata.len() == 0;

        let character_name = if !is_empty {
            extract_character_name(path).ok().flatten()
        } else {
            None
        };

        let session_number =
            self.compute_session_number(character_name.as_deref().unwrap_or("Unknown"), date);

        Some(LogFileMetaData {
            path: path.to_path_buf(),
            filename,
            date,
            created_at,
            character_name,
            session_number,
            is_empty,
        })
    }

    fn add_entry(&mut self, entry: LogFileMetaData) {
        self.entries.insert(entry.path.clone(), entry);
    }

    pub fn add_file(&mut self, path: &Path) -> Option<()> {
        let entry = self.create_entry(path)?;
        let path_key = entry.path.clone();
        self.entries.insert(path_key, entry);
        Some(())
    }

    pub fn remove_file(&mut self, path: &Path) {
        self.entries.remove(path);
    }

    fn compute_session_number(&mut self, character: &str, date: Date) -> u32 {
        let key = (character.to_string(), date);
        let count = self.session_counts.entry(key).or_insert(0);
        *count += 1;
        *count
    }

    // Accessor methods

    //Return all entries sorted ascending by created_at
    pub fn entries(&self) -> Vec<&LogFileMetaData> {
        let mut entries: Vec<_> = self.entries.values().collect();
        entries.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        entries
    }

    pub fn character_entries(&self, name: &str) -> Vec<&LogFileMetaData> {
        let name_lower = name.to_lowercase();
        let mut entries: Vec<_> = self
            .entries
            .values()
            .filter(|e| {
                e.character_name
                    .as_ref()
                    .map(|n| n.to_lowercase().contains(&name_lower))
                    .unwrap_or(false)
            })
            .collect();
        entries.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        entries
    }

    pub fn entries_older_than(&self, days: u32, reference_date: Date) -> Vec<&LogFileMetaData> {
        self.entries
            .values()
            .filter(|e| {
                let diff = reference_date - e.date;
                diff.whole_days() > days as i64
            })
            .collect()
    }

    /// Get all empty files (excluding the most recent)
    pub fn empty_files(&self) -> Vec<&LogFileMetaData> {
        let newest = self.newest_file().map(|e| &e.path);
        self.entries
            .values()
            .filter(|e| e.is_empty && Some(&e.path) != newest)
            .collect()
    }

    pub fn newest_file(&self) -> Option<&LogFileMetaData> {
        self.entries.values().max_by_key(|e| e.created_at)
    }

    /// Get entry count
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

pub fn parse_log_filename(filename: &str) -> Option<(Date, PrimitiveDateTime)> {
    let stem = filename.strip_suffix(".txt").unwrap_or(filename);
    let parts: Vec<&str> = stem.split('_').collect();

    if parts.len() != 6 || parts[0] != "combat" {
        println!("attempted to parse invalid file name {}", filename);
        return None;
    }

    let date = Date::parse(parts[1], &Iso8601::DATE).ok()?;
    let hour: u8 = parts[2].parse().ok()?;
    let min: u8 = parts[3].parse().ok()?;
    let sec: u8 = parts[4].parse().ok()?;
    let microsec: u32 = parts[5].parse().ok()?;

    let time = Time::from_hms_micro(hour, min, sec, microsec).ok()?;
    Some((date, PrimitiveDateTime::new(date, time)))
}

pub fn extract_character_name(path: &Path) -> Result<Option<String>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);

    //take first 25 lines. If not in first 25 something is probably wrong
    for (idx, line) in reader.lines().take(25).enumerate() {
        let line = line?;
        if let Some(event) = parse_line(idx as u64, &line)
            && event.effect.type_id == effect_type_id::DISCIPLINECHANGED
        {
            return Ok(Some(event.source_entity.name));
        }
    }
    Ok(None)
}
