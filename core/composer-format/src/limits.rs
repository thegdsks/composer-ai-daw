/// Caps applied while reading untrusted bytes. Every field can be raised or
/// lowered by the caller; the defaults suit a desktop or browser tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Limits {
    /// Whole input.
    pub max_file_bytes: u64,
    /// Entries in the section table.
    pub max_sections: u32,
    /// The `proj` section.
    pub max_project_bytes: u64,
    /// The `asst` section.
    pub max_assets_bytes: u64,
    /// The `meta` section.
    pub max_meta_bytes: u64,
    /// Any section this build does not know (it is only checksummed, never copied).
    pub max_other_section_bytes: u64,
    pub max_tracks: u64,
    pub max_regions: u64,
    pub max_notes: u64,
    pub max_assets: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_file_bytes: 1 << 30,
            max_sections: 64,
            max_project_bytes: 512 << 20,
            max_assets_bytes: 64 << 20,
            max_meta_bytes: 64 << 10,
            max_other_section_bytes: 64 << 20,
            max_tracks: 4_096,
            max_regions: 2_000_000,
            max_notes: 20_000_000,
            max_assets: 200_000,
        }
    }
}
