//! Strict GeoJSON overlay ingestion and layer composition.
use geojson::{Feature, FeatureCollection, GeoJson, Geometry, Value};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value as JsonValue};
use std::str::FromStr;
use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum GeoError {
    #[error("invalid GeoJSON: {0}")]
    Parse(String),
    #[error("expected a GeoJSON FeatureCollection")]
    NotFeatureCollection,
    #[error("feature {feature}: {reason}")]
    InvalidFeature { feature: usize, reason: String },
    #[error("layer name must not be blank")]
    BlankLayerName,
    #[error("at least one layer is required")]
    NoLayers,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OverlayLayer {
    pub name: String,
    pub collection: FeatureCollection,
}

pub fn parse_feature_collection(input: &str) -> Result<FeatureCollection, GeoError> {
    if input.trim().is_empty() {
        return Err(GeoError::Parse("input is blank".into()));
    }
    let geojson = GeoJson::from_str(input).map_err(|e| GeoError::Parse(e.to_string()))?;
    let GeoJson::FeatureCollection(collection) = geojson else {
        return Err(GeoError::NotFeatureCollection);
    };
    validate_feature_collection(&collection)?;
    Ok(collection)
}

pub fn validate_feature_collection(collection: &FeatureCollection) -> Result<(), GeoError> {
    if let Some(bbox) = &collection.bbox {
        validate_bbox(bbox).map_err(|reason| GeoError::InvalidFeature {
            feature: 0,
            reason: format!("collection bbox {reason}"),
        })?;
    }
    for (index, feature) in collection.features.iter().enumerate() {
        validate_feature(feature).map_err(|reason| GeoError::InvalidFeature {
            feature: index,
            reason,
        })?;
    }
    Ok(())
}

pub fn combine_layers(layers: &[OverlayLayer]) -> Result<FeatureCollection, GeoError> {
    if layers.is_empty() {
        return Err(GeoError::NoLayers);
    }
    let mut features = Vec::new();
    for layer in layers {
        let name = layer.name.trim();
        if name.is_empty() {
            return Err(GeoError::BlankLayerName);
        }
        validate_feature_collection(&layer.collection)?;
        for feature in &layer.collection.features {
            let mut feature = feature.clone();
            feature
                .properties
                .get_or_insert_with(Map::new)
                .insert("_layer".into(), JsonValue::String(name.to_owned()));
            features.push(feature);
        }
    }
    let mut foreign_members = Map::new();
    foreign_members.insert(
        "layers".into(),
        JsonValue::Array(
            layers
                .iter()
                .map(|layer| JsonValue::String(layer.name.trim().to_owned()))
                .collect(),
        ),
    );
    Ok(FeatureCollection {
        bbox: None,
        features,
        foreign_members: Some(foreign_members),
    })
}

pub fn combine_layer_json<'a, I>(layers: I) -> Result<String, GeoError>
where
    I: IntoIterator<Item = (&'a str, &'a str)>,
{
    let parsed = layers
        .into_iter()
        .map(|(name, json)| {
            Ok(OverlayLayer {
                name: name.to_owned(),
                collection: parse_feature_collection(json)?,
            })
        })
        .collect::<Result<Vec<_>, GeoError>>()?;
    let combined = combine_layers(&parsed)?;
    serde_json::to_string(&combined).map_err(|e| GeoError::Parse(e.to_string()))
}

fn validate_feature(feature: &Feature) -> Result<(), String> {
    if let Some(bbox) = &feature.bbox {
        validate_bbox(bbox).map_err(|e| format!("bbox {e}"))?;
    }
    let geometry = feature
        .geometry
        .as_ref()
        .ok_or_else(|| "geometry must not be null".to_owned())?;
    validate_geometry(geometry)
}

fn validate_geometry(geometry: &Geometry) -> Result<(), String> {
    if let Some(bbox) = &geometry.bbox {
        validate_bbox(bbox).map_err(|e| format!("geometry bbox {e}"))?;
    }
    match &geometry.value {
        Value::Point(point) => position(point),
        Value::MultiPoint(points) => {
            nonempty(points, "MultiPoint")?;
            points.iter().try_for_each(|p| position(p))
        }
        Value::LineString(line) => line_string(line),
        Value::MultiLineString(lines) => {
            nonempty(lines, "MultiLineString")?;
            lines.iter().try_for_each(|line| line_string(line))
        }
        Value::Polygon(rings) => polygon(rings),
        Value::MultiPolygon(polygons) => {
            nonempty(polygons, "MultiPolygon")?;
            polygons.iter().try_for_each(|rings| polygon(rings))
        }
        Value::GeometryCollection(geometries) => {
            nonempty(geometries, "GeometryCollection")?;
            geometries.iter().try_for_each(validate_geometry)
        }
    }
}

fn nonempty<T>(items: &[T], kind: &str) -> Result<(), String> {
    if items.is_empty() {
        Err(format!("{kind} must not be empty"))
    } else {
        Ok(())
    }
}

fn position(position: &[f64]) -> Result<(), String> {
    if position.len() < 2 {
        return Err("position needs longitude and latitude".into());
    }
    if !position.iter().all(|v| v.is_finite()) {
        return Err("position contains a non-finite number".into());
    }
    if !(-180.0..=180.0).contains(&position[0]) || !(-90.0..=90.0).contains(&position[1]) {
        return Err("position is outside longitude/latitude bounds".into());
    }
    Ok(())
}

fn line_string(line: &[Vec<f64>]) -> Result<(), String> {
    if line.len() < 2 {
        return Err("LineString needs at least two positions".into());
    }
    line.iter().try_for_each(|p| position(p))
}

fn polygon(rings: &[Vec<Vec<f64>>]) -> Result<(), String> {
    nonempty(rings, "Polygon")?;
    for ring in rings {
        if ring.len() < 4 {
            return Err("polygon ring needs at least four positions".into());
        }
        ring.iter().try_for_each(|p| position(p))?;
        if ring.first() != ring.last() {
            return Err("polygon ring is not closed".into());
        }
    }
    Ok(())
}

fn validate_bbox(bbox: &[f64]) -> Result<(), String> {
    if !matches!(bbox.len(), 4 | 6) || !bbox.iter().all(|v| v.is_finite()) {
        return Err("must contain four or six finite numbers".into());
    }
    let dimensions = bbox.len() / 2;
    if bbox[..dimensions]
        .iter()
        .zip(&bbox[dimensions..])
        .any(|(min, max)| min > max)
    {
        return Err("minimum exceeds maximum".into());
    }
    if !(-180.0..=180.0).contains(&bbox[0])
        || !(-90.0..=90.0).contains(&bbox[1])
        || !(-180.0..=180.0).contains(&bbox[dimensions])
        || !(-90.0..=90.0).contains(&bbox[dimensions + 1])
    {
        return Err("is outside longitude/latitude bounds".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const POINTS: &str = r#"{"type":"FeatureCollection","features":[
      {"type":"Feature","properties":{"id":"c1"},"geometry":{"type":"Point","coordinates":[-83.1,42.2]}}
    ]}"#;
    const AREA: &str = r#"{"type":"FeatureCollection","features":[
      {"type":"Feature","properties":null,"geometry":{"type":"Polygon","coordinates":[[[-83,42],[-82,42],[-82,43],[-83,42]]]}}
    ]}"#;

    #[test]
    fn parses_valid_feature_collection() {
        let parsed = parse_feature_collection(POINTS).unwrap();
        assert_eq!(parsed.features.len(), 1);
    }

    #[test]
    fn rejects_wrong_root_null_geometry_and_bad_coordinates() {
        assert_eq!(
            parse_feature_collection(r#"{"type":"Point","coordinates":[0,0]}"#).unwrap_err(),
            GeoError::NotFeatureCollection
        );
        for invalid in [
            r#"{"type":"FeatureCollection","features":[{"type":"Feature","properties":{},"geometry":null}]}"#,
            r#"{"type":"FeatureCollection","features":[{"type":"Feature","properties":{},"geometry":{"type":"Point","coordinates":[181,0]}}]}"#,
            r#"{"type":"FeatureCollection","features":[{"type":"Feature","properties":{},"geometry":{"type":"LineString","coordinates":[[0,0]]}}]}"#,
            r#"{"type":"FeatureCollection","features":[{"type":"Feature","properties":{},"geometry":{"type":"Polygon","coordinates":[[[0,0],[1,0],[1,1],[0,1]]]}}]}"#,
        ] {
            assert!(matches!(
                parse_feature_collection(invalid),
                Err(GeoError::InvalidFeature { .. })
            ));
        }
    }

    #[test]
    fn combines_layers_and_labels_every_feature() {
        let json = combine_layer_json([("clues", POINTS), ("assignments", AREA)]).unwrap();
        let combined = parse_feature_collection(&json).unwrap();
        assert_eq!(combined.features.len(), 2);
        assert_eq!(
            combined.features[0].property("_layer"),
            Some(&serde_json::json!("clues"))
        );
        assert_eq!(
            combined.features[1].property("_layer"),
            Some(&serde_json::json!("assignments"))
        );
        assert_eq!(
            combined.foreign_members.unwrap()["layers"],
            serde_json::json!(["clues", "assignments"])
        );
    }

    #[test]
    fn rejects_empty_or_unnamed_layers() {
        assert_eq!(combine_layers(&[]).unwrap_err(), GeoError::NoLayers);
        let layer = OverlayLayer {
            name: " ".into(),
            collection: parse_feature_collection(POINTS).unwrap(),
        };
        assert_eq!(
            combine_layers(&[layer]).unwrap_err(),
            GeoError::BlankLayerName
        );
    }
}
