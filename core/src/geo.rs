//! Map-layer geography: coordinates, pin models and the ordering/filtering
//! the map screen relies on. Region-agnostic — nothing here assumes any
//! particular city; the shell supplies positions and a reference point
//! (usually the user's location, when granted).
//!
//! These types cross the FFI unchanged, so they are defined as UniFFI
//! records/enums directly.

/// Mean Earth radius used by [`haversine_m`], meters.
const EARTH_RADIUS_M: f64 = 6_371_000.0;

/// A WGS84 geographic coordinate, degrees.
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct GeoPointF {
    /// Latitude in degrees, -90..90.
    pub lat: f64,
    /// Longitude in degrees, -180..180.
    pub lon: f64,
}

/// What a pin on the map represents.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum MapSpotKindF {
    /// 公認ウォール — a spot where painting is permitted.
    LegalWall,
    /// One of the user's own saved works.
    WorkMine,
    /// Another user's work (served by the phase-2 works index).
    WorkOther,
}

/// One pin on the map. The shell builds these from the bundled legal-wall
/// seed, the work-pin provider (other users' works) and the local works
/// store (own works); the core decides which of them show and in what order.
#[derive(Clone, Debug, uniffi::Record)]
pub struct MapSpotF {
    /// Stable identifier (seed id, provider id, or local work id).
    pub id: String,
    pub kind: MapSpotKindF,
    pub position: GeoPointF,
    /// Display name — spot name or work title.
    pub title: String,
    /// Secondary line — author name, spot description, date, …
    pub subtitle: Option<String>,
    /// External SNS link for the author (work pins only).
    pub author_url: Option<String>,
}

/// Great-circle distance between two coordinates in meters (haversine).
#[uniffi::export]
pub fn haversine_m(a: GeoPointF, b: GeoPointF) -> f64 {
    let lat1 = a.lat.to_radians();
    let lat2 = b.lat.to_radians();
    let dlat = lat2 - lat1;
    let dlon = (b.lon - a.lon).to_radians();

    let sin_dlat = (dlat / 2.0).sin();
    let sin_dlon = (dlon / 2.0).sin();
    let h = sin_dlat * sin_dlat + lat1.cos() * lat2.cos() * sin_dlon * sin_dlon;
    2.0 * EARTH_RADIUS_M * h.sqrt().asin()
}

/// The pins the map should display: filtered to `kinds` and (when both are
/// given) within `radius_m` of `center`, then sorted nearest-first.
///
/// With no `center` — e.g. location permission denied — the radius filter is
/// skipped and the input order is kept; the map still works, just unranked.
#[uniffi::export]
pub fn visible_map_spots(
    spots: Vec<MapSpotF>,
    kinds: Vec<MapSpotKindF>,
    center: Option<GeoPointF>,
    radius_m: Option<f64>,
) -> Vec<MapSpotF> {
    let mut out: Vec<MapSpotF> = spots
        .into_iter()
        .filter(|s| kinds.contains(&s.kind))
        .filter(|s| match (center, radius_m) {
            (Some(c), Some(r)) => haversine_m(c, s.position) <= r,
            _ => true,
        })
        .collect();
    if let Some(c) = center {
        out.sort_by(|a, b| {
            haversine_m(c, a.position).total_cmp(&haversine_m(c, b.position))
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pt(lat: f64, lon: f64) -> GeoPointF {
        GeoPointF { lat, lon }
    }

    fn spot(id: &str, kind: MapSpotKindF, lat: f64, lon: f64) -> MapSpotF {
        MapSpotF {
            id: id.to_string(),
            kind,
            position: pt(lat, lon),
            title: id.to_string(),
            subtitle: None,
            author_url: None,
        }
    }

    // City pair with a well-known distance: Shibuya (Tokyo) to Osaka Station
    // is ~396 km by great circle.
    const SHIBUYA: GeoPointF = GeoPointF { lat: 35.6580, lon: 139.7016 };
    const OSAKA: GeoPointF = GeoPointF { lat: 34.7024, lon: 135.4959 };

    #[test]
    fn haversine_zero_for_same_point() {
        assert_eq!(haversine_m(SHIBUYA, SHIBUYA), 0.0);
    }

    #[test]
    fn haversine_known_city_pair() {
        let d = haversine_m(SHIBUYA, OSAKA);
        assert!(
            (d - 396_000.0).abs() < 10_000.0,
            "expected ~396 km, got {d} m"
        );
    }

    #[test]
    fn haversine_is_symmetric_and_scales() {
        let ab = haversine_m(SHIBUYA, OSAKA);
        let ba = haversine_m(OSAKA, SHIBUYA);
        assert!((ab - ba).abs() < 1e-6);
        // ~1 degree of latitude ≈ 111 km anywhere on the globe.
        let deg = haversine_m(pt(0.0, 0.0), pt(1.0, 0.0));
        assert!((deg - 111_195.0).abs() < 500.0);
    }

    #[test]
    fn filters_by_kind() {
        let spots = vec![
            spot("wall", MapSpotKindF::LegalWall, 35.0, 139.0),
            spot("mine", MapSpotKindF::WorkMine, 35.0, 139.1),
            spot("other", MapSpotKindF::WorkOther, 35.0, 139.2),
        ];
        let out = visible_map_spots(
            spots,
            vec![MapSpotKindF::LegalWall, MapSpotKindF::WorkOther],
            None,
            None,
        );
        assert_eq!(out.len(), 2);
        assert!(out.iter().all(|s| s.kind != MapSpotKindF::WorkMine));
    }

    #[test]
    fn radius_filters_around_center() {
        let near = spot("near", MapSpotKindF::LegalWall, 35.6581, 139.7017);
        let far = spot("far", MapSpotKindF::LegalWall, 35.7, 139.8);
        let out = visible_map_spots(
            vec![near, far],
            vec![MapSpotKindF::LegalWall],
            Some(SHIBUYA),
            Some(200.0),
        );
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].id, "near");
    }

    #[test]
    fn sorts_nearest_first() {
        // Input is deliberately unordered; output must be distance-ranked.
        let a = spot("far", MapSpotKindF::WorkMine, 36.0, 140.0);
        let b = spot("near", MapSpotKindF::WorkMine, 35.6581, 139.7017);
        let c = spot("mid", MapSpotKindF::WorkMine, 35.75, 139.75);
        let out = visible_map_spots(
            vec![a, b, c],
            vec![MapSpotKindF::WorkMine],
            Some(SHIBUYA),
            None,
        );
        let ids: Vec<&str> = out.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, ["near", "mid", "far"]);
    }

    #[test]
    fn no_center_keeps_input_order_and_ignores_radius() {
        let a = spot("a", MapSpotKindF::LegalWall, 40.0, 140.0);
        let b = spot("b", MapSpotKindF::LegalWall, 35.0, 139.0);
        // Denied-location path: everything passes through in input order.
        let out = visible_map_spots(
            vec![a, b],
            vec![MapSpotKindF::LegalWall],
            None,
            Some(1.0),
        );
        let ids: Vec<&str> = out.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, ["a", "b"]);
    }
}
