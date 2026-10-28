//! Adding torrents with `torrents/add`.

use reqwest::multipart::{Form, Part};

/// How the files of a torrent are laid out in the save folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ContentLayout {
    /// Keep the torrent's own layout
    #[default]
    Original,
    /// Always put the files in a folder named after the torrent
    Subfolder,
    /// Never create a top-level folder
    NoSubfolder,
}

impl ContentLayout {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Original => "Original",
            Self::Subfolder => "Subfolder",
            Self::NoSubfolder => "NoSubfolder",
        }
    }
}

/// A `.torrent` file to upload.
#[derive(Clone, PartialEq, Eq)]
pub struct TorrentFileUpload {
    pub file_name: String,
    pub data: Vec<u8>,
}

impl std::fmt::Debug for TorrentFileUpload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TorrentFileUpload")
            .field("file_name", &self.file_name)
            .field("len", &self.data.len())
            .finish()
    }
}

/// What to add and how. Server defaults apply to the `None` fields.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AddTorrents {
    /// Magnet links or HTTP(S) URLs of `.torrent` files
    pub urls: Vec<String>,
    pub files: Vec<TorrentFileUpload>,
    /// Folder on the server
    pub save_path: Option<String>,
    pub category: Option<String>,
    pub tags: Vec<String>,
    /// Add without starting (the `paused` parameter before qBittorrent 5.0)
    pub stopped: bool,
    pub content_layout: Option<ContentLayout>,
}

impl AddTorrents {
    pub fn is_empty(&self) -> bool {
        self.urls.is_empty() && self.files.is_empty()
    }

    /// Builds the multipart form. Called once per attempt, since a form
    /// can't be reused.
    pub(crate) fn to_form(&self) -> Form {
        let mut form = Form::new();
        if !self.urls.is_empty() {
            form = form.text("urls", self.urls.join("\n"));
        }
        for file in &self.files {
            let part = Part::bytes(file.data.clone())
                .file_name(file.file_name.clone())
                .mime_str("application/x-bittorrent")
                .expect("valid MIME type");
            form = form.part("torrents", part);
        }
        if let Some(path) = self.save_path.as_ref().filter(|p| !p.is_empty()) {
            form = form.text("savepath", path.clone());
        }
        if let Some(category) = self.category.as_ref().filter(|c| !c.is_empty()) {
            form = form.text("category", category.clone());
        }
        if !self.tags.is_empty() {
            form = form.text("tags", self.tags.join(","));
        }
        form = form.text("stopped", self.stopped.to_string());
        if let Some(layout) = self.content_layout {
            form = form.text("contentLayout", layout.as_str());
        }
        form
    }
}
