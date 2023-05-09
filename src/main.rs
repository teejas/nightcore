use nightcore::*;

fn main() {
    let wav = Track::default();
    wav.resample(1.35);
    wav.playback(10); // plays 10s of the original and resampled track
}