use crate::LogParser;
use crate::context::resolve;
use crate::game_data::effect_type_id;
use chrono::{NaiveDate, NaiveDateTime};
use hashbrown::HashMap;
use std::fs;
use std::fs::File;
use std::io::{BufRead, BufReader, Result};
use std::path::{Path, PathBuf};

pub struct LogFileMetaData {
    pub path: PathBuf,
    pub filename: String,
    pub date: NaiveDate,
    pub created_at: NaiveDateTime,
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
pub struct DirectoryIndex {
    entries: HashMap<PathBuf, LogFileMetaData>,
    session_counts: HashMap<(String, NaiveDate), u32>,
}

impl DirectoryIndex {
    pub fn new() -> Self {
        Self {
            ..Default::default()
        }
    }

    pub fn build_index(dir: &Path) -> Result<Self> {
        let mut index = Self::new();

        if !dir.exists() {
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
        let metadata = fs::metadata(path).ok()?;
        let is_empty = metadata.len() == 0;

        let character_name = if !is_empty {
            extract_character_name(path, created_at).ok().flatten()
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

    fn compute_session_number(&mut self, character: &str, date: NaiveDate) -> u32 {
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

    pub fn entries_older_than(
        &self,
        days: u32,
        reference_date: NaiveDate,
    ) -> Vec<&LogFileMetaData> {
        self.entries
            .values()
            .filter(|e| {
                let diff = reference_date - e.date;
                diff.num_days() > days as i64
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

pub fn parse_log_filename(filename: &str) -> Option<(NaiveDate, NaiveDateTime)> {
    let stem = filename.strip_suffix(".txt").unwrap_or(filename);
    let filedate = stem.strip_prefix("combat_");

    let date_stamp = NaiveDateTime::parse_from_str(filedate?, "%Y-%m-%d_%H_%M_%S_%f").ok()?;

    Some((date_stamp.date(), date_stamp))
}

const CHECK_N_LINES: usize = 25;

pub fn extract_character_name(path: &Path, session_date: NaiveDateTime) -> Result<Option<String>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let parser = LogParser::new(session_date);

    //take first 25 lines. If not in first 25 something is probably wrong
    for (idx, line) in reader.lines().take(CHECK_N_LINES).enumerate() {
        let line = line?;
        if let Some(event) = &parser.parse_line(idx as u64, &line)
            && event.effect.type_id == effect_type_id::DISCIPLINECHANGED
        {
            return Ok(Some(resolve(event.source_entity.name).to_string()));
        }
    }
    Ok(None)
}
