//! Great-circle distance between two coordinates.

/// Mean Earth radius in kilometres.
const EARTH_RADIUS_KM: f64 = 6371.0;

/// Haversine distance in kilometres between two points given in degrees.
pub fn haversine_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let (p1, p2) = (lat1.to_radians(), lat2.to_radians());
    let dp = (lat2 - lat1).to_radians();
    let dl = (lon2 - lon1).to_radians();
    let a = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    2.0 * EARTH_RADIUS_KM * a.sqrt().asin()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_point_is_zero() {
        assert_eq!(haversine_km(39.7, -105.0, 39.7, -105.0), 0.0);
    }

    #[test]
    fn denver_to_boulder_is_about_39_km() {
        let d = haversine_km(39.7392, -104.9903, 40.0150, -105.2705);
        assert!((d - 38.8).abs() < 1.0, "{d}");
    }

    #[test]
    fn one_degree_of_latitude_is_about_111_km() {
        let d = haversine_km(0.0, 0.0, 1.0, 0.0);
        assert!((d - 111.19).abs() < 0.1, "{d}");
    }
}
