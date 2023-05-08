use std::{
    io::{BufReader},
    fs::File,
    path::PathBuf,
    time::Instant
};
use rodio::{
    Decoder, OutputStream, source::Source
};
use dasp::{interpolate::sinc::Sinc, ring_buffer, signal, Sample, Signal};
use hound::{WavSpec, WavReader, WavWriter};
use clap::Parser;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Args {
    #[arg(short, long, default_value = "./examples/short_melody.wav")]
    file: String,
}

impl Args {
    pub fn get_input_filepath(&self) -> PathBuf {
        PathBuf::from(&self.file)
    }
}

pub struct Track {
    orig_fp: PathBuf,
    target_fp: PathBuf,
    orig_spec: WavSpec,
    target_spec: WavSpec
}

impl Track {
    fn from(orig_fp: PathBuf, target_fp: PathBuf, orig_spec: WavSpec, target_spec: WavSpec) -> Self {
        Self {
            orig_fp,
            target_fp,
            orig_spec,
            target_spec
        }
    }

    pub fn resample(&self) { // f_ratio is the ratio to change the sample rate by
        let reader = WavReader::open(&self.orig_fp).unwrap();
        let f_ratio = self.target_spec.sample_rate / self.orig_spec.sample_rate;
    
        // Read the interleaved samples and convert them to a signal.
        let samples = reader
            .into_samples()
            .filter_map(Result::ok)
            .map(i16::to_sample::<f64>);
        let signal = signal::from_interleaved_samples_iter(samples);
    
        // Convert the signal's sample rate using `Sinc` interpolation.
        let ring_buffer = ring_buffer::Fixed::from([[0.0]; 100]);
        let sinc = Sinc::new(ring_buffer);
        let start = Instant::now();
        let new_signal = signal.scale_hz(sinc, f_ratio as f64);
    
        // Write the result to a new file.
        let mut writer = WavWriter::create(&self.target_fp, self.target_spec).unwrap();
        for frame in new_signal.until_exhausted() {
            writer.write_sample(frame[0].to_sample::<i16>()).unwrap();
        }
    
        let duration = start.elapsed();
        println!("Took {:?} to resample", duration);
    }

    pub fn playback(&self, playtime: u64) {
        println!("Playing original track...");
        playback(self.orig_fp.clone(), playtime).unwrap();
    
        println!("Playing resampled track...");
        playback(self.target_fp.clone(), playtime).unwrap();
    }
}

impl Default for Track {
    fn default() -> Self {
        let args = Args::parse();
        dbg!("Input file: ", args.get_input_filepath());
        let reader = WavReader::open(args.get_input_filepath()).unwrap();
        let orig_spec = reader.spec();
        dbg!("Wav spec: ", reader.spec());
        let mut target_spec = orig_spec;
        let f_ratio = 1.35;
        target_spec.sample_rate = (orig_spec.sample_rate as f32 * f_ratio as f32) as u32;
        Track::from( 
            args.get_input_filepath(),
            PathBuf::from("output.wav"),
            orig_spec,
            target_spec
        )
    }
}

fn load_file(filepath: PathBuf) -> Option<File> {
    File::open(filepath).ok()
}

fn playback(filepath: PathBuf, playtime: u64) -> Result<String, rodio::PlayError> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn playback_non_existing_file() {
        let input_file = PathBuf::from("blah");
        let str_eq = String::from("Failed to load input file!");
        assert_eq!(str_eq, playback(input_file, 5).unwrap())
    }

    #[test]
    fn playback_existing_file() {
        let input_file = PathBuf::from("examples/short_melody.mp3");
        let str_eq = String::from("Finished playing track.");
        assert_eq!(str_eq, playback(input_file, 0).unwrap())
    }
}