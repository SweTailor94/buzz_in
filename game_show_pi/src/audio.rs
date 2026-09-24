use std::fs::File;
use std::io::BufReader;

use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink};

pub struct AudioPlayer {
    _stream: OutputStream,
    stream_handle: OutputStreamHandle,
    managed_sink: Option<Sink>,
}

impl AudioPlayer {
    pub fn try_new() -> Option<Self> {
        let (stream, stream_handle) = OutputStream::try_default().ok()?;
        Some(Self {
            _stream: stream,
            stream_handle,
            managed_sink: None,
        })
    }

    pub fn play_one_shot_sound(&self, filename: &str) {
        if let Ok(file) = File::open(filename) {
            let buf_reader = BufReader::new(file);
            if let Ok(source) = Decoder::new(buf_reader) {
                if let Ok(sink) = Sink::try_new(&self.stream_handle) {
                    sink.append(source);
                    sink.detach();
                } else {
                    eprintln!("Failed to create audio sink for: {}", filename);
                }
            } else {
                eprintln!("Failed to decode audio file: {}", filename);
            }
        } else {
            eprintln!("Failed to open audio file: {}", filename);
        }
    }

    pub fn play_managed_sound(&mut self, filename: &str) {
        self.stop_managed_sound();
        if let Ok(file) = File::open(filename) {
            let buf_reader = BufReader::new(file);
            if let Ok(source) = Decoder::new(buf_reader) {
                if let Ok(sink) = Sink::try_new(&self.stream_handle) {
                    sink.append(source);
                    self.managed_sink = Some(sink);
                } else {
                    eprintln!("Failed to create managed audio sink for: {}", filename);
                }
            } else {
                eprintln!("Failed to decode audio file: {}", filename);
            }
        } else {
            eprintln!("Failed to open audio file: {}", filename);
        }
    }

    pub fn pause_managed_sound(&self) {
        if let Some(sink) = &self.managed_sink {
            sink.pause();
        }
    }

    pub fn resume_managed_sound(&self) {
        if let Some(sink) = &self.managed_sink {
            sink.play();
        }
    }

    pub fn stop_managed_sound(&mut self) {
        if let Some(sink) = self.managed_sink.take() {
            sink.stop();
        }
    }
}
