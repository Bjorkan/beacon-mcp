//! Evidence engine: facts vs calculations vs coverage vs unsupported.
//! Advert/neighbor/relay/poll/status are distinct; no cross-kind inference.

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EvidenceKind {
    Advert,
    NeighborList,
    RelayObservation,
    PollReading,
    ObserverStatus,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Conclusion {
    Supported,
    Unsupported(String),
}

pub struct Evidence {
    pub kind: EvidenceKind,
    pub provenance: &'static str,
    pub at: &'static str,
}

impl Evidence {
    pub fn supports_bidirectional_link(&self) -> Conclusion {
        Conclusion::Unsupported("advert is not a bidirectional-link proof".to_owned())
    }

    pub fn supports_reachability(&self) -> Conclusion {
        Conclusion::Unsupported(format!(
            "{:?} does not prove current reachability",
            self.kind
        ))
    }

    pub fn supports_delivery(&self) -> Conclusion {
        match self.kind {
            EvidenceKind::RelayObservation => {
                Conclusion::Unsupported("relay is not delivery/path-quality proof".to_owned())
            }
            _ => Conclusion::Unsupported("no delivery proof from this evidence".to_owned()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RouteShape {
    Segment,
    FullRoute,
}

pub fn route_shape(is_trimmed_to_span: bool) -> RouteShape {
    if is_trimmed_to_span {
        RouteShape::Segment
    } else {
        RouteShape::FullRoute
    }
}

/// Observer-comparison union: onlyA + onlyB + both = total distinct packets.
pub fn comparison_union(only_a: u64, only_b: u64, both: u64) -> u64 {
    only_a + only_b + both
}

#[cfg(test)]
mod tests {
    use super::{Conclusion, Evidence, EvidenceKind, comparison_union, route_shape};

    #[test]
    fn no_cross_kind_inference() {
        let advert = Evidence {
            kind: EvidenceKind::Advert,
            provenance: "observer",
            at: "t",
        };
        assert!(matches!(
            advert.supports_bidirectional_link(),
            Conclusion::Unsupported(_)
        ));
        assert!(matches!(
            advert.supports_reachability(),
            Conclusion::Unsupported(_)
        ));
        let status = Evidence {
            kind: EvidenceKind::ObserverStatus,
            provenance: "server",
            at: "t",
        };
        assert!(matches!(
            status.supports_reachability(),
            Conclusion::Unsupported(_)
        ));
    }

    #[test]
    fn segments_are_not_full_routes_and_counts_stay_distinct() {
        assert_eq!(route_shape(true), super::RouteShape::Segment);
        assert_eq!(route_shape(false), super::RouteShape::FullRoute);
        assert_eq!(comparison_union(2, 3, 5), 10);
    }
}
