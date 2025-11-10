//! Streaming utilities for Quill RPC

use crate::framing::Frame;
use bytes::Bytes;
use std::pin::Pin;
use std::task::{Context, Poll};

/// A stream of frames
pub trait FrameStream: Send {
    /// Poll for the next frame
    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame, crate::QuillError>>>;
}

/// Stream writer for sending frames
pub struct StreamWriter {
    frames: Vec<Frame>,
}

impl StreamWriter {
    /// Create a new stream writer
    pub fn new() -> Self {
        Self { frames: Vec::new() }
    }

    /// Send a data frame
    pub fn send(&mut self, data: Bytes) {
        self.frames.push(Frame::data(data));
    }

    /// End the stream
    pub fn end(&mut self) {
        self.frames.push(Frame::end_stream());
    }

    /// Get all frames
    pub fn into_frames(mut self) -> Vec<Frame> {
        // Ensure stream is ended
        if !self.frames.iter().any(|f| f.flags.is_end_stream()) {
            self.end();
        }
        self.frames
    }
}

impl Default for StreamWriter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stream_writer() {
        let mut writer = StreamWriter::new();
        writer.send(Bytes::from("hello"));
        writer.send(Bytes::from("world"));

        let frames = writer.into_frames();
        assert_eq!(frames.len(), 3); // 2 data + 1 end
        assert!(frames[0].flags.is_data());
        assert!(frames[1].flags.is_data());
        assert!(frames[2].flags.is_end_stream());
    }
}
