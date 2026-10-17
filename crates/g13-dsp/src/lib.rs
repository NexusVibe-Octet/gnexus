//! Audio processing of G13 Studio's audio effects, with no OS code: a graphic
//! equalizer made of peaking biquad filters (RBJ "Audio EQ Cookbook"), and the
//! settings it is driven by.
//!
//! The Windows audio effect (APO, crate `g13-apo`) runs [`Equalizer::process`] in the
//! audio engine; on Linux the same settings will be turned into a PipeWire filter chain.
//! [`Equalizer::process`] never allocates: it may run on a real-time audio thread.

/// Up to this many bands and channels (fixed arrays: no allocation while processing).
pub const MAX_BANDS: usize = 16;
pub const MAX_CHANNELS: usize = 8;

/// The usual graphic equalizer bands (Hz), one octave apart.
pub const DEFAULT_FREQS: [f32; 10] = [32.0, 64.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0];
/// Bandwidth of each band: about one octave.
pub const DEFAULT_Q: f32 = 1.41;
pub const MIN_DB: f32 = -12.0;
pub const MAX_DB: f32 = 12.0;

/// What the equalizer does. Stored as a small binary file read by the audio effect
/// ([`EqSettings::to_bytes`]).
#[derive(Debug, Clone, PartialEq)]
pub struct EqSettings {
    pub enabled: bool,
    /// (centre frequency Hz, gain dB) per band.
    pub bands: Vec<(f32, f32)>,
    pub q: f32,
}

impl Default for EqSettings {
    fn default() -> Self {
        Self { enabled: true, bands: DEFAULT_FREQS.iter().map(|&f| (f, 0.0)).collect(), q: DEFAULT_Q }
    }
}

const MAGIC: &[u8; 6] = b"G13EQ1";

impl EqSettings {
    /// Every gain is 0 dB (or the equalizer is off): the sound is left untouched.
    pub fn is_flat(&self) -> bool {
        !self.enabled || self.bands.iter().all(|&(_, g)| g == 0.0)
    }

    /// Headroom: the sound is lowered by the largest boost, so boosting a band never clips.
    pub fn preamp_db(&self) -> f32 {
        -self.bands.iter().map(|&(_, g)| g).fold(0.0, f32::max)
    }

    /// `G13EQ1`, enabled (u8), band count (u8), q (f32), then (freq, gain) f32 pairs, little-endian.
    pub fn to_bytes(&self) -> Vec<u8> {
        let n = self.bands.len().min(MAX_BANDS);
        let mut v = Vec::with_capacity(12 + 8 * n);
        v.extend_from_slice(MAGIC);
        v.push(u8::from(self.enabled));
        v.push(n as u8);
        v.extend_from_slice(&self.q.to_le_bytes());
        for &(f, g) in &self.bands[..n] {
            v.extend_from_slice(&f.to_le_bytes());
            v.extend_from_slice(&g.to_le_bytes());
        }
        v
    }

    /// Read settings back; `None` if the bytes are not valid settings (the effect then does nothing).
    pub fn from_bytes(b: &[u8]) -> Option<Self> {
        if b.len() < 12 || &b[..6] != MAGIC {
            return None;
        }
        let n = usize::from(b[7]);
        if n > MAX_BANDS || b.len() < 12 + 8 * n {
            return None;
        }
        let f = |i: usize| f32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
        let q = f(8);
        let bands: Vec<(f32, f32)> = (0..n).map(|k| (f(12 + 8 * k), f(16 + 8 * k))).collect();
        let sane = q.is_finite()
            && q > 0.05
            && bands.iter().all(|&(fr, g)| fr.is_finite() && fr > 10.0 && g.is_finite());
        sane.then(|| Self {
            enabled: b[6] != 0,
            bands: bands.into_iter().map(|(fr, g)| (fr, g.clamp(MIN_DB, MAX_DB))).collect(),
            q,
        })
    }
}

/// Normalised biquad coefficients (a0 = 1).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Biquad {
    pub b0: f32,
    pub b1: f32,
    pub b2: f32,
    pub a1: f32,
    pub a2: f32,
}

impl Biquad {
    pub const IDENTITY: Self = Self { b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0 };

    /// Peaking EQ filter (RBJ cookbook), computed in f64 for accuracy at low frequencies.
    pub fn peaking(rate: f32, freq: f32, gain_db: f32, q: f32) -> Self {
        let nyquist = f64::from(rate) * 0.5;
        let f0 = f64::from(freq).clamp(10.0, nyquist * 0.95);
        if gain_db == 0.0 {
            return Self::IDENTITY;
        }
        let a = 10f64.powf(f64::from(gain_db) / 40.0);
        let w0 = 2.0 * std::f64::consts::PI * f0 / f64::from(rate);
        let alpha = w0.sin() / (2.0 * f64::from(q));
        let cos = w0.cos();
        let a0 = 1.0 + alpha / a;
        Self {
            b0: ((1.0 + alpha * a) / a0) as f32,
            b1: ((-2.0 * cos) / a0) as f32,
            b2: ((1.0 - alpha * a) / a0) as f32,
            a1: ((-2.0 * cos) / a0) as f32,
            a2: ((1.0 - alpha / a) / a0) as f32,
        }
    }

    /// Gain (linear) of the filter at `freq`.
    pub fn magnitude(&self, rate: f32, freq: f32) -> f32 {
        let w = 2.0 * std::f64::consts::PI * f64::from(freq) / f64::from(rate);
        let (c1, s1, c2, s2) = (w.cos(), w.sin(), (2.0 * w).cos(), (2.0 * w).sin());
        let (b0, b1, b2, a1, a2) = (f64::from(self.b0), f64::from(self.b1), f64::from(self.b2), f64::from(self.a1), f64::from(self.a2));
        let nr = b0 + b1 * c1 + b2 * c2;
        let ni = -(b1 * s1 + b2 * s2);
        let dr = 1.0 + a1 * c1 + a2 * c2;
        let di = -(a1 * s1 + a2 * s2);
        (((nr * nr + ni * ni) / (dr * dr + di * di)).sqrt()) as f32
    }
}

/// The equalizer for one stream: interleaved f32 samples, `channels` × frames.
pub struct Equalizer {
    rate: f32,
    channels: usize,
    bands: usize,
    coefs: [Biquad; MAX_BANDS],
    /// Transposed direct form II state: (z1, z2) per channel and band.
    state: [[[f32; 2]; MAX_BANDS]; MAX_CHANNELS],
    preamp: f32,
    bypass: bool,
}

impl Equalizer {
    pub fn new(rate: f32, channels: usize) -> Self {
        Self {
            rate: rate.max(8000.0),
            channels: channels.clamp(1, MAX_CHANNELS),
            bands: 0,
            coefs: [Biquad::IDENTITY; MAX_BANDS],
            state: [[[0.0; 2]; MAX_BANDS]; MAX_CHANNELS],
            preamp: 1.0,
            bypass: true,
        }
    }

    pub fn channels(&self) -> usize {
        self.channels
    }

    /// New settings (no allocation; the filter memory is kept so the change does not click).
    pub fn set(&mut self, s: &EqSettings) {
        self.bypass = s.is_flat();
        self.bands = s.bands.len().min(MAX_BANDS);
        for (k, &(f, g)) in s.bands.iter().take(MAX_BANDS).enumerate() {
            self.coefs[k] = Biquad::peaking(self.rate, f, g.clamp(MIN_DB, MAX_DB), s.q);
        }
        self.preamp = 10f32.powf(s.preamp_db() / 20.0);
        if self.bypass {
            self.state = [[[0.0; 2]; MAX_BANDS]; MAX_CHANNELS];
        }
    }

    /// Equalize interleaved samples in place.
    pub fn process(&mut self, data: &mut [f32]) {
        if self.bypass {
            return;
        }
        let ch = self.channels;
        for frame in data.chunks_exact_mut(ch) {
            for (c, x) in frame.iter_mut().enumerate() {
                let mut v = *x * self.preamp;
                for k in 0..self.bands {
                    let q = &self.coefs[k];
                    let z = &mut self.state[c][k];
                    let y = q.b0 * v + z[0];
                    z[0] = q.b1 * v - q.a1 * y + z[1];
                    z[1] = q.b2 * v - q.a2 * y;
                    v = y;
                }
                // denormals and runaway values never reach the output
                *x = if v.is_finite() { v.clamp(-4.0, 4.0) } else { 0.0 };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db(x: f32) -> f32 {
        20.0 * x.log10()
    }

    #[test]
    fn peaking_gain_at_centre_and_far_away() {
        let b = Biquad::peaking(48000.0, 1000.0, 6.0, DEFAULT_Q);
        assert!((db(b.magnitude(48000.0, 1000.0)) - 6.0).abs() < 0.05);
        assert!(db(b.magnitude(48000.0, 20.0)).abs() < 0.2);
        let low = Biquad::peaking(48000.0, 32.0, -12.0, DEFAULT_Q);
        assert!((db(low.magnitude(48000.0, 32.0)) + 12.0).abs() < 0.05);
    }

    /// Steady-state level of a sine through the equalizer.
    fn level(eq: &mut Equalizer, freq: f32) -> f32 {
        let rate = 48000.0;
        let mut buf: Vec<f32> = (0..48000 * 2).map(|i| (2.0 * std::f32::consts::PI * freq * (i / 2) as f32 / rate).sin() * 0.25).collect();
        eq.process(&mut buf);
        let tail = &buf[48000..];
        tail.iter().map(|x| x.abs()).fold(0.0, f32::max) / 0.25
    }

    #[test]
    fn equalizer_follows_the_bands_with_headroom() {
        let mut s = EqSettings::default();
        s.bands[0].1 = 12.0; // 32 Hz
        s.bands[7].1 = -12.0; // 4 kHz
        let mut eq = Equalizer::new(48000.0, 2);
        eq.set(&s);
        // headroom: -12 dB everywhere, then +12 at 32 Hz → 0 dB, -12 at 4 kHz → -24 dB
        assert!(db(level(&mut eq, 32.0)).abs() < 1.0);
        assert!((db(level(&mut eq, 4000.0)) + 24.0).abs() < 1.0);
        assert!((db(level(&mut eq, 500.0)) + 12.0).abs() < 1.0);
    }

    #[test]
    fn flat_is_untouched_and_settings_round_trip() {
        let mut eq = Equalizer::new(44100.0, 2);
        eq.set(&EqSettings::default());
        let mut buf = vec![0.5f32, -0.5, 0.25, -0.25];
        eq.process(&mut buf);
        assert_eq!(buf, [0.5, -0.5, 0.25, -0.25]);
        let mut s = EqSettings::default();
        s.bands[3].1 = -4.5;
        assert_eq!(EqSettings::from_bytes(&s.to_bytes()), Some(s));
        assert_eq!(EqSettings::from_bytes(b"garbage"), None);
    }
}
