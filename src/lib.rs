use std::{
    io::{BufReader},
    fs::File,
    path::PathBuf,
    time::Instant
};
use rodio::{
    Decoder, OutputStream, source::Source
};
use dasp::{
    interpolate::sinc::Sinc, 
    ring_buffer, 
    signal,
    Sample, 
    Signal
};
use hound::{SampleFormat, WavSpec, WavReader, WavWriter};
use clap::Parser;
use symphonia::core::{
    audio::SampleBuffer,
    codecs::{DecoderOptions, CODEC_TYPE_NULL},
    errors::Error,
    formats::FormatOptions,
    io::MediaSourceStream,
    meta::MetadataOptions,
    probe::Hint
};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value = "./examples/short_melody.wav")]
    input_file: String,

    #[arg(short, long, default_value = "output.wav")]
    output_file: String
}

impl Args {
    fn get_input_fp(&self) -> PathBuf {
        PathBuf::from(&self.input_file)
    }

    fn get_output_fp(&self) -> PathBuf {
        PathBuf::from(&self.output_file)
    }
}

pub struct Track {
    orig_fp: PathBuf,
    target_fp: PathBuf,
    spec: WavSpec,
    samples: Vec::<f64>
}

impl Track {
    fn from(orig_fp: PathBuf, target_fp: PathBuf, spec: WavSpec) -> Self {
        let samples = get_samples_from_fp(&orig_fp);
        Self {
            orig_fp,
            target_fp,
            spec,
            samples
        }
    }

    pub fn resample(&self, f_ratio: f32) { // f_ratio is the ratio to change the sample rate by
        dbg!(f_ratio);
        let signal = signal::from_interleaved_samples_iter(self.samples.clone());
    
        // Convert the signal's sample rate using `Sinc` interpolation.
        let ring_buffer = ring_buffer::Fixed::from([[0.0]; 100]);
        let sinc = Sinc::new(ring_buffer);
        let start = Instant::now();
        let new_signal = signal.scale_hz(sinc, f_ratio as f64);
        let mut target_spec = self.spec;
        target_spec.sample_rate = (self.spec.sample_rate as f32 * f_ratio) as u32;
    
        // Write the result to a new file.
        let mut writer = WavWriter::create(&self.target_fp, target_spec).unwrap();
        for frame in new_signal.until_exhausted() {
            writer.write_sample(frame[0].to_sample::<i16>()).unwrap();
        }
    
        let duration = start.elapsed();
        println!("Took {:?} to resample", duration);
    }

    pub fn playback(&self, playtime: u64) {
        println!("Playing original track...");
        playback(&self.orig_fp, playtime).unwrap();
    
        println!("Playing resampled track...");
        playback(&self.target_fp, playtime).unwrap();
    }
}

impl Default for Track {
    fn default() -> Self {
        let args = Args::parse();
        dbg!(args.get_input_fp());
        let spec = match WavReader::open(args.get_input_fp()).ok() {
            Some(reader) => reader.spec(),
            None => {
                WavSpec {
                    channels: 1,
                    sample_rate: 64_000,
                    bits_per_sample: 16,
                    sample_format: SampleFormat::Int
                }
            }
        };
        dbg!(spec);
        Track::from( 
            args.get_input_fp(),
            args.get_output_fp(),
            spec
        )
    }
}

fn load_file(filepath: &PathBuf) -> Option<File> {
    File::open(filepath).ok()
}

fn playback(filepath: &PathBuf, playtime: u64) -> Result<String, rodio::PlayError> {
    // Get a output stream handle to the default physical sound device
    let (_stream, stream_handle) = OutputStream::try_default().unwrap();
    // Load a sound from a file, using a path relative to Cargo.toml
    if let Some(file) = load_file(filepath) {
        let bufreader = BufReader::new(file);
        // Decode that sound file into a source
        let source = Decoder::new(bufreader).unwrap();
        // Play the sound directly on the device
        stream_handle.play_raw(source.convert_samples())?;
    
        // The sound plays in a separate audio thread,
        // so we need to keep the main thread alive while it's playing.
        std::thread::sleep(std::time::Duration::from_secs(playtime));
        Ok(String::from("Finished playing track."))
    } else {
        eprintln!("Failed to load input file, make sure path is correct and file exists!");
        Ok(String::from("Failed to load input file!"))
    }
}

fn get_samples_from_fp(filepath: &PathBuf) -> Vec::<f64> {
    // Open the media source.
    let src = std::fs::File::open(filepath).expect("failed to open media");

    // Create the media source stream.
    let mss = MediaSourceStream::new(Box::new(src), Default::default());

    // Create a probe hint using the file's extension. [Optional]
    let mut hint = Hint::new();
    hint.with_extension("mp3");

    // Use the default options for metadata and format readers.
    let meta_opts: MetadataOptions = Default::default();
    let fmt_opts: FormatOptions = Default::default();

    // Probe the media source.
    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &fmt_opts, &meta_opts)
        .expect("unsupported format");

    // Get the instantiated format reader.
    let mut format = probed.format;

    // Find the first audio track with a known (decodeable) codec.
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .expect("no supported audio tracks");

    // Use the default options for the decoder.
    let dec_opts: DecoderOptions = Default::default();

    // Create a decoder for the track.
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &dec_opts)
        .expect("unsupported codec");

    // Store the track identifier, it will be used to filter packets.
    let track_id = track.id;
    let mut samples = vec![];

    // The decode loop.
    loop {
        // Get the next packet from the media format.
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(Error::ResetRequired) => {
                // The track list has been changed. Re-examine it and create a new set of decoders,
                // then restart the decode loop. This is an advanced feature and it is not
                // unreasonable to consider this "the end." As of v0.5.0, the only usage of this is
                // for chained OGG physical streams.
                unimplemented!();
            }
            Err(Error::IoError(_)) => {
                println!("Reached end of stream");
                break
            }
            Err(err) => {
                // A unrecoverable error occured, halt decoding.
                panic!("{}", err);
            }
        };

        // Consume any new metadata that has been read since the last packet.
        while !format.metadata().is_latest() {
            // Pop the old head of the metadata queue.
            format.metadata().pop();

            // Consume the new metadata at the head of the metadata queue.
        }

        // If the packet does not belong to the selected track, skip over it.
        if packet.track_id() != track_id {
            continue;
        }

        let mut samples_buf = None;
        // Decode the packet into audio samples.
        match decoder.decode(&packet) {
            Ok(decoded) => {
                // Consume the decoded audio samples (see below).
                if samples_buf.is_none() {
                    // Get the audio buffer specification.
                    let spec = *decoded.spec();

                    // Get the capacity of the decoded buffer. Note: This is capacity, not length!
                    let duration = decoded.capacity() as u64;

                    // Create the f64 sample buffer.
                    samples_buf = Some(SampleBuffer::<f64>::new(duration, spec));
                }

                // Copy the decoded audio buffer into the sample buffer in an interleaved format.
                if let Some(buf) = &mut samples_buf {
                    buf.copy_interleaved_ref(decoded);
                    for sample in buf.samples() {
                        samples.push(sample.to_sample::<f64>());
                    }
                }
            }
            Err(Error::IoError(_)) => {
                // The packet failed to decode due to an IO error, skip the packet.
                continue;
            }
            Err(Error::DecodeError(_)) => {
                // The packet failed to decode due to invalid data, skip the packet.
                continue;
            }
            Err(err) => {
                // An unrecoverable error occured, halt decoding.
                panic!("{}", err);
            }
        }
    }
    samples
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn playback_non_existing_file() {
        let input_file = PathBuf::from("blah");
        let str_eq = String::from("Failed to load input file!");
        assert_eq!(str_eq, playback(&input_file, 5).unwrap())
    }

    #[test]
    fn playback_existing_file() {
        let input_file = PathBuf::from("examples/short_melody.mp3");
        let str_eq = String::from("Finished playing track.");
        assert_eq!(str_eq, playback(&input_file, 0).unwrap())
    }

    // to-do: add tests for resampling mp3 and wav
}