//! Radio configuration: parsed freq/bw/sf(+cr) and preset provenance.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresetSource {
    Observer,
    Node,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RadioConfig {
    pub freq_mhz: f64,
    pub bw_khz: f64,
    pub sf: u8,
    pub cr: Option<String>,
    pub source: PresetSource,
}

impl RadioConfig {
    pub fn parse(preset: &str, source: PresetSource) -> Option<Self> {
        // Forms: "freqMhz,bwKhz,sf" or with "/cr" suffix; see RadioPreset.preset.
        let (main, cr) = match preset.split_once('/') {
            Some((main, cr)) if !cr.is_empty() => (main, Some(cr.to_owned())),
            Some(_) => return None,
            None => (preset, None),
        };
        let mut parts = main.split(',');
        let freq_mhz: f64 = parts.next()?.parse().ok()?;
        let bw_khz: f64 = parts.next()?.parse().ok()?;
        let sf: u8 = parts.next()?.parse().ok()?;
        if parts.next().is_some() || !(7..=12).contains(&sf) || freq_mhz <= 0.0 || bw_khz <= 0.0 {
            return None;
        }
        Some(Self {
            freq_mhz,
            bw_khz,
            sf,
            cr,
            source,
        })
    }

    pub fn friendly(&self) -> String {
        match &self.cr {
            Some(cr) => format!("{},{},{} / {}", self.freq_mhz, self.bw_khz, self.sf, cr),
            None => format!("{},{},{}", self.freq_mhz, self.bw_khz, self.sf),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{PresetSource, RadioConfig};

    #[test]
    fn preset_round_trip_and_invalid_rejected() {
        let parsed = RadioConfig::parse("869.525,250,11", PresetSource::Observer).unwrap();
        assert_eq!(parsed.sf, 11);
        assert_eq!(parsed.source, PresetSource::Observer);
        assert!(parsed.friendly().contains("869.525"));
        assert!(RadioConfig::parse("869.525,250,13", PresetSource::Node).is_none());
        assert!(RadioConfig::parse("bad", PresetSource::Node).is_none());
    }
}
