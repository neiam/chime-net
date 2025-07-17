use crate::types::notes::{chord_notes, frequency_for_note};
use crate::types::Result;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, Host, SampleFormat, Stream, StreamConfig};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

unsafe impl Send for AudioPlayer {}
unsafe impl Sync for AudioPlayer {}

pub struct AudioPlayer {
    _host: Host,
    _device: Device,
    _stream: Stream,
    sender: mpsc::Sender<AudioCommand>,
}

#[derive(Debug, Clone)]
enum AudioCommand {
    PlayNote { frequency: f32, duration_ms: u64 },
    Stop,
}

impl AudioPlayer {
    pub fn new() -> Result<Self> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| anyhow::anyhow!("No output device available"))?;

        let config = device.default_output_config()?;
        let sample_rate = config.sample_rate().0;
        let _channels = config.channels();

        let (sender, receiver) = mpsc::channel::<AudioCommand>();

        // Shared state for the audio generator
        let audio_state = Arc::new(Mutex::new(AudioState::new()));
        let audio_state_clone = Arc::clone(&audio_state);

        // Spawn a thread to handle audio commands
        let audio_state_cmd = Arc::clone(&audio_state);
        thread::spawn(move || {
            while let Ok(command) = receiver.recv() {
                match command {
                    AudioCommand::PlayNote {
                        frequency,
                        duration_ms,
                    } => {
                        let mut state = audio_state_cmd.lock().unwrap();
                        state.add_note(frequency, duration_ms, sample_rate);
                    }
                    AudioCommand::Stop => {
                        let mut state = audio_state_cmd.lock().unwrap();
                        state.stop();
                    }
                }
            }
        });

        let stream = match config.sample_format() {
            SampleFormat::F32 => build_stream::<f32>(&device, &config.into(), audio_state_clone)?,
            SampleFormat::I16 => build_stream::<i16>(&device, &config.into(), audio_state_clone)?,
            SampleFormat::U16 => build_stream::<u16>(&device, &config.into(), audio_state_clone)?,
            _ => return Err(anyhow::anyhow!("Unsupported sample format").into()),
        };

        stream.play()?;

        Ok(Self {
            _host: host,
            _device: device,
            _stream: stream,
            sender,
        })
    }

    pub fn play_note(&self, note: &str, duration_ms: u64) -> Result<()> {
        if let Some(frequency) = frequency_for_note(note) {
            self.sender.send(AudioCommand::PlayNote {
                frequency,
                duration_ms,
            })?;
        }
        Ok(())
    }

    pub fn play_chord(&self, chord: &str, duration_ms: u64) -> Result<()> {
        let notes = chord_notes(chord);
        for note in notes {
            self.play_note(&note, duration_ms)?;
        }
        Ok(())
    }

    pub fn play_notes(&self, notes: &[String], duration_ms: u64) -> Result<()> {
        for note in notes {
            self.play_note(note, duration_ms)?;
        }
        Ok(())
    }

    pub fn play_chords(&self, chords: &[String], duration_ms: u64) -> Result<()> {
        for chord in chords {
            self.play_chord(chord, duration_ms)?;
        }
        Ok(())
    }

    pub fn stop(&self) {
        let _ = self.sender.send(AudioCommand::Stop);
    }

    pub fn wait_for_completion(&self) {
        // For simplicity, we'll sleep for a short duration
        // In a real implementation, you might want to track active notes
        thread::sleep(Duration::from_millis(100));
    }
}

struct AudioState {
    notes: Vec<Note>,
    current_sample: usize,
}

struct Note {
    frequency: f32,
    duration_samples: usize,
    current_sample: usize,
    amplitude: f32,
}

impl AudioState {
    fn new() -> Self {
        Self {
            notes: Vec::new(),
            current_sample: 0,
        }
    }

    fn add_note(&mut self, frequency: f32, duration_ms: u64, sample_rate: u32) {
        let duration_samples = (duration_ms as f32 * sample_rate as f32 / 1000.0) as usize;
        self.notes.push(Note {
            frequency,
            duration_samples,
            current_sample: 0,
            amplitude: 0.3, // Lower volume
        });
    }

    fn stop(&mut self) {
        self.notes.clear();
    }

    fn next_sample(&mut self, sample_rate: u32) -> f32 {
        let mut sample = 0.0;
        let mut notes_to_remove = Vec::new();

        for (i, note) in self.notes.iter_mut().enumerate() {
            if note.current_sample >= note.duration_samples {
                notes_to_remove.push(i);
                continue;
            }

            let t = note.current_sample as f32 / sample_rate as f32;
            let note_sample =
                (t * note.frequency * 2.0 * std::f32::consts::PI).sin() * note.amplitude;
            sample += note_sample;
            note.current_sample += 1;
        }

        // Remove completed notes (in reverse order to maintain indices)
        for &i in notes_to_remove.iter().rev() {
            self.notes.remove(i);
        }

        self.current_sample += 1;
        sample
    }
}

fn build_stream<T>(
    device: &Device,
    config: &StreamConfig,
    audio_state: Arc<Mutex<AudioState>>,
) -> Result<Stream>
where
    T: cpal::Sample + cpal::SizedSample + cpal::FromSample<f32>,
{
    let sample_rate = config.sample_rate.0;
    let channels = config.channels as usize;

    let stream = device.build_output_stream(
        config,
        move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
            let mut state = audio_state.lock().unwrap();
            for frame in data.chunks_mut(channels) {
                let sample = state.next_sample(sample_rate);
                for channel in frame.iter_mut() {
                    *channel = cpal::Sample::from_sample(sample);
                }
            }
        },
        move |err| {
            eprintln!("Audio stream error: {}", err);
        },
        None,
    )?;

    Ok(stream)
}

pub struct ChimePlayer {
    audio_player: Arc<AudioPlayer>,
}

impl Clone for ChimePlayer {
    fn clone(&self) -> Self {
        Self {
            audio_player: Arc::clone(&self.audio_player),
        }
    }
}

impl ChimePlayer {
    pub fn new() -> Result<Self> {
        Ok(Self {
            audio_player: Arc::new(AudioPlayer::new()?),
        })
    }

    pub fn play_chime(
        &self,
        notes: Option<&[String]>,
        chords: Option<&[String]>,
        duration_ms: Option<u64>,
    ) -> Result<()> {
        let duration = duration_ms.unwrap_or(500);

        if let Some(notes) = notes {
            self.audio_player.play_notes(notes, duration)?;
        }

        if let Some(chords) = chords {
            self.audio_player.play_chords(chords, duration)?;
        }

        // If no notes or chords specified, play a default chime
        if notes.is_none() && chords.is_none() {
            self.audio_player.play_note("C4", duration)?;
            self.audio_player.play_note("E4", duration)?;
            self.audio_player.play_note("G4", duration)?;
        }

        Ok(())
    }

    pub fn stop(&self) {
        self.audio_player.stop();
    }

    pub fn wait_for_completion(&self) {
        self.audio_player.wait_for_completion();
    }
}
