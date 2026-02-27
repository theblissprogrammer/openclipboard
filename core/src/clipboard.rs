//! Clipboard abstraction.

use anyhow::Result;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, PartialEq)]
pub enum ClipboardContent {
    Empty,
    Text(String),
    Image { mime: String, width: u32, height: u32, bytes: Vec<u8> },
}

pub trait ClipboardProvider: Send + Sync {
    fn read(&self) -> Result<ClipboardContent>;
    fn write(&self, content: ClipboardContent) -> Result<()>;
    fn on_change(&self, callback: Box<dyn Fn(ClipboardContent) + Send + Sync>) -> Result<()>;
}

/// Mock clipboard for testing.
pub struct MockClipboard {
    content: Arc<Mutex<ClipboardContent>>,
    callbacks: Arc<Mutex<Vec<Box<dyn Fn(ClipboardContent) + Send + Sync>>>>,
}

impl MockClipboard {
    pub fn new() -> Self {
        Self {
            content: Arc::new(Mutex::new(ClipboardContent::Empty)),
            callbacks: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Simulate a user copy action (triggers callbacks).
    pub fn simulate_copy(&self, content: ClipboardContent) {
        {
            let mut c = self.content.lock().unwrap();
            *c = content.clone();
        }
        let cbs = self.callbacks.lock().unwrap();
        for cb in cbs.iter() {
            cb(content.clone());
        }
    }
}

impl Default for MockClipboard {
    fn default() -> Self {
        Self::new()
    }
}

impl ClipboardProvider for MockClipboard {
    fn read(&self) -> Result<ClipboardContent> {
        Ok(self.content.lock().unwrap().clone())
    }

    fn write(&self, content: ClipboardContent) -> Result<()> {
        let mut c = self.content.lock().unwrap();
        *c = content;
        Ok(())
    }

    fn on_change(&self, callback: Box<dyn Fn(ClipboardContent) + Send + Sync>) -> Result<()> {
        self.callbacks.lock().unwrap().push(callback);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_read_write() {
        let cb = MockClipboard::new();
        assert_eq!(cb.read().unwrap(), ClipboardContent::Empty);
        cb.write(ClipboardContent::Text("hello".into())).unwrap();
        assert_eq!(cb.read().unwrap(), ClipboardContent::Text("hello".into()));
    }

    #[test]
    fn mock_image_read_write() {
        let cb = MockClipboard::new();
        let img = ClipboardContent::Image { mime: "image/png".into(), width: 2, height: 2, bytes: vec![1, 2, 3] };
        cb.write(img.clone()).unwrap();
        assert_eq!(cb.read().unwrap(), img);
    }

    #[test]
    fn mock_on_change() {
        let cb = MockClipboard::new();
        let received = Arc::new(Mutex::new(Vec::new()));
        let r = received.clone();
        cb.on_change(Box::new(move |c| { r.lock().unwrap().push(c); })).unwrap();
        cb.simulate_copy(ClipboardContent::Text("test".into()));
        assert_eq!(received.lock().unwrap().len(), 1);
    }
}
