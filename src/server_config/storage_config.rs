pub struct LocalStorage {
    root: std::path::PathBuf,
}

impl LocalStorage {
    pub fn root(&self) -> &std::path::Path {
        &self.root
    }
    pub fn open_for_read(&self, p: &std::path::Path) -> Result<tokio::fs::File, tokio::io::Error> {
        std::fs::File::open(self.root.join(p)).map(tokio::fs::File::from_std)
    }
    pub fn open_for_write(&self, p: &std::path::Path) -> Result<tokio::fs::File, tokio::io::Error> {
        std::fs::File::create(self.root.join(p)).map(tokio::fs::File::from_std)
    }
}
