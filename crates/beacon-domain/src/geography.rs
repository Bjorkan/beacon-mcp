//! Geography: IATA partition codes, case-sensitive scopes, regions, positions.
//! IATA is a Beacon network partition, not necessarily an airport.

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Iata(String);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Scope(String);

impl Iata {
    pub fn parse(input: &str) -> Option<Self> {
        let trimmed = input.trim();
        if trimmed.len() != 3 || !trimmed.bytes().all(|b| b.is_ascii_alphanumeric()) {
            return None;
        }
        Some(Self(trimmed.to_ascii_uppercase()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Scope {
    pub fn parse(input: &str) -> Option<Self> {
        if input.is_empty()
            || input.len() > 128
            || input.chars().any(char::is_control)
            || matches!(input, "." | "..")
            || input.contains('/')
            || input.contains('\\')
            || input.contains('%')
        {
            return None;
        }
        Some(Self(input.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocationSource {
    Advert,
    Manual,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Position {
    pub latitude: f64,
    pub longitude: f64,
    pub source: LocationSource,
    pub possibly_foreign: bool,
}

impl Position {
    pub fn trusted(&self) -> bool {
        !self.possibly_foreign
    }
}

#[cfg(test)]
mod tests {
    use super::{Iata, LocationSource, Position, Scope};

    #[test]
    fn iata_uppercase_and_scope_case_sensitive() {
        assert_eq!(Iata::parse("arn").unwrap().as_str(), "ARN");
        assert_eq!(Iata::parse("ARN").unwrap().as_str(), "ARN");
        assert!(Iata::parse("AR/").is_none());
        assert!(Iata::parse("ARNO").is_none());
        assert_eq!(Scope::parse("#bc").unwrap().as_str(), "#bc");
        assert_ne!(
            Scope::parse("#bc").unwrap().as_str(),
            Scope::parse("#BC").unwrap().as_str()
        );
        assert!(Scope::parse("").is_none());
    }

    #[test]
    fn foreign_positions_are_not_trusted() {
        let foreign = Position {
            latitude: 59.3,
            longitude: 18.0,
            source: LocationSource::Advert,
            possibly_foreign: true,
        };
        assert!(!foreign.trusted());
        let manual = Position {
            latitude: 59.3,
            longitude: 18.0,
            source: LocationSource::Manual,
            possibly_foreign: false,
        };
        assert!(manual.trusted());
    }
}
