use anyhow::Result;
use serde_json::Value as JsonValue;
use std::cmp::Ordering;

use crate::storage::BlobStorage;

/// JSON filter for advanced querying with conditions, sorting, and pagination
pub struct JsonFilter<'a> {
    storage: &'a BlobStorage,
}

impl<'a> JsonFilter<'a> {
    pub fn new(storage: &'a BlobStorage) -> Self {
        Self { storage }
    }

    /// Filter JSON data across multiple keys using pattern
    /// Supports conditions, sorting, and pagination
    pub async fn filter_across_keys(
        &self,
        pattern: &str,
        conditions: &JsonFilterConditions,
        options: &FilterOptions,
    ) -> Result<Vec<JsonValue>> {
        // Scan keys matching pattern
        let keys = self.storage.scan_keys(pattern).await?;

        if keys.is_empty() {
            return Ok(Vec::new());
        }

        let mut collected = Vec::new();

        // Process keys and collect matching documents
        for key in keys {
            if let Ok(Some(json_str)) = self.storage.get_json(&key, None).await {
                if let Ok(mut doc) = serde_json::from_str::<JsonValue>(&json_str) {
                    // Apply conditions
                    if self.matches_conditions(&doc, conditions) {
                        // Add _key field for reference
                        if let Some(obj) = doc.as_object_mut() {
                            obj.insert("_key".to_string(), JsonValue::String(key.clone()));
                        }
                        collected.push(doc);
                    }
                }
            }
        }

        // Sort if required
        if let Some(sort_by) = &options.sort_by {
            let sort_order = &options.sort_order;
            collected.sort_by(|a, b| self.compare_values(a, b, sort_by, sort_order));
        }

        // Apply pagination
        let offset = options.offset.unwrap_or(0);
        let limit = options.limit.unwrap_or(usize::MAX);

        Ok(collected.into_iter().skip(offset).take(limit).collect())
    }

    /// Check if a JSON document matches the filter conditions
    fn matches_conditions(&self, doc: &JsonValue, conditions: &JsonFilterConditions) -> bool {
        if conditions.is_empty() {
            return true;
        }

        for (field, condition) in &conditions.conditions {
            let field_value = self.get_nested_field(doc, field);

            if !self.matches_condition(&field_value, condition) {
                return false;
            }
        }

        true
    }

    /// Get nested field from JSON document using dot notation
    fn get_nested_field(&self, doc: &JsonValue, field: &str) -> Option<JsonValue> {
        let parts: Vec<&str> = field.split('.').collect();
        let mut current = doc;

        for part in parts {
            match current.get(part) {
                Some(value) => current = value,
                None => return None,
            }
        }

        Some(current.clone())
    }

    /// Check if a value matches a condition
    fn matches_condition(&self, value: &Option<JsonValue>, condition: &FilterCondition) -> bool {
        let val = match value {
            Some(v) => v,
            None => return false,
        };

        match condition {
            FilterCondition::Eq(target) => val == target,
            FilterCondition::Ne(target) => val != target,
            FilterCondition::Gt(target) => self.compare_json(val, target) == Ordering::Greater,
            FilterCondition::Gte(target) => {
                let cmp = self.compare_json(val, target);
                cmp == Ordering::Greater || cmp == Ordering::Equal
            }
            FilterCondition::Lt(target) => self.compare_json(val, target) == Ordering::Less,
            FilterCondition::Lte(target) => {
                let cmp = self.compare_json(val, target);
                cmp == Ordering::Less || cmp == Ordering::Equal
            }
            FilterCondition::Contains(s) => {
                if let Some(str_val) = val.as_str() {
                    str_val.contains(s)
                } else {
                    false
                }
            }
            FilterCondition::In(values) => values.contains(val),
        }
    }

    /// Compare two JSON values
    fn compare_json(&self, a: &JsonValue, b: &JsonValue) -> Ordering {
        match (a, b) {
            (JsonValue::Number(n1), JsonValue::Number(n2)) => {
                let f1 = n1.as_f64().unwrap_or(0.0);
                let f2 = n2.as_f64().unwrap_or(0.0);
                f1.partial_cmp(&f2).unwrap_or(Ordering::Equal)
            }
            (JsonValue::String(s1), JsonValue::String(s2)) => s1.cmp(s2),
            (JsonValue::Bool(b1), JsonValue::Bool(b2)) => b1.cmp(b2),
            _ => Ordering::Equal,
        }
    }

    /// Compare values for sorting
    fn compare_values(
        &self,
        a: &JsonValue,
        b: &JsonValue,
        sort_by: &str,
        sort_order: &SortOrder,
    ) -> Ordering {
        let val_a = self.get_nested_field(a, sort_by);
        let val_b = self.get_nested_field(b, sort_by);

        let cmp = match (val_a, val_b) {
            (Some(v1), Some(v2)) => self.compare_json(&v1, &v2),
            (Some(_), None) => Ordering::Greater,
            (None, Some(_)) => Ordering::Less,
            (None, None) => Ordering::Equal,
        };

        match sort_order {
            SortOrder::Asc => cmp,
            SortOrder::Desc => cmp.reverse(),
        }
    }
}

/// Stream filter for querying stream entries
pub struct StreamFilter<'a> {
    storage: &'a BlobStorage,
}

impl<'a> StreamFilter<'a> {
    pub fn new(storage: &'a BlobStorage) -> Self {
        Self { storage }
    }

    /// Get entries from a stream with range
    pub async fn get_entries(
        &self,
        db_addr: &str,
        stream_name: &str,
        from: &str,
        to: &str,
    ) -> Result<Vec<(String, Vec<(String, String)>)>> {
        let full_key = format!("{}:{}", db_addr, stream_name);
        self.storage.xrange(&full_key, from, to, None).await
    }

    /// Get last N entries from a stream (reverse order)
    pub async fn get_last_n_entries(
        &self,
        db_addr: &str,
        stream_name: &str,
        count: usize,
    ) -> Result<Vec<(String, Vec<(String, String)>)>> {
        let full_key = format!("{}:{}", db_addr, stream_name);
        self.storage
            .xrevrange(&full_key, "+", "-", Some(count))
            .await
    }

    /// Filter stream entries by field pattern
    pub async fn filter_by_pattern(
        &self,
        db_addr: &str,
        stream_name: &str,
        pattern: &str,
    ) -> Result<Vec<(String, Vec<(String, String)>)>> {
        let full_key = format!("{}:{}", db_addr, stream_name);
        self.storage
            .filter_stream(&full_key, "-", "+", Some(pattern))
            .await
    }
}

/// Sorted set filter for querying sorted set data
pub struct SortedSetFilter<'a> {
    storage: &'a BlobStorage,
}

impl<'a> SortedSetFilter<'a> {
    pub fn new(storage: &'a BlobStorage) -> Self {
        Self { storage }
    }

    /// Get entries from sorted set with score range
    pub async fn get_entries_by_score(
        &self,
        key: &str,
        min_score: f64,
        max_score: f64,
    ) -> Result<Vec<(JsonValue, f64)>> {
        let entries = self.storage.get_sorted_set_json(key, 0, -1).await?;

        Ok(entries
            .into_iter()
            .filter(|entry| entry.score >= min_score && entry.score <= max_score)
            .map(|entry| (entry.data, entry.score))
            .collect())
    }

    /// Get entries from sorted set with index range
    pub async fn get_entries(
        &self,
        key: &str,
        start: isize,
        stop: isize,
    ) -> Result<Vec<(JsonValue, f64)>> {
        let entries = self.storage.get_sorted_set_json(key, start, stop).await?;

        Ok(entries
            .into_iter()
            .map(|entry| (entry.data, entry.score))
            .collect())
    }
}

/// Time series filter for querying time series data
pub struct TimeSeriesFilter<'a> {
    storage: &'a BlobStorage,
}

impl<'a> TimeSeriesFilter<'a> {
    pub fn new(storage: &'a BlobStorage) -> Self {
        Self { storage }
    }

    /// Query time series data with optional filtering and aggregation
    pub async fn query(
        &self,
        key: &str,
        from_timestamp: i64,
        to_timestamp: i64,
        options: &TimeSeriesOptions,
    ) -> Result<Vec<(i64, f64)>> {
        let mut points = self
            .storage
            .ts_range(key, from_timestamp, to_timestamp)
            .await?;

        // Apply value filters
        if let Some(min_value) = options.min_value {
            points.retain(|(_, val)| *val >= min_value);
        }
        if let Some(max_value) = options.max_value {
            points.retain(|(_, val)| *val <= max_value);
        }

        // Apply timestamp filters
        if let Some(ref filter_ts) = options.filter_by_ts {
            points.retain(|(ts, _)| filter_ts.contains(ts));
        }

        // Apply count limit
        if let Some(count) = options.count {
            points.truncate(count);
        }

        // Apply aggregation
        if let Some(ref agg) = options.aggregation {
            points = self.aggregate_points(&points, agg)?;
        }

        Ok(points)
    }

    /// Aggregate time series points
    fn aggregate_points(
        &self,
        points: &[(i64, f64)],
        agg: &Aggregation,
    ) -> Result<Vec<(i64, f64)>> {
        if points.is_empty() {
            return Ok(Vec::new());
        }

        let mut buckets: std::collections::HashMap<i64, Vec<f64>> =
            std::collections::HashMap::new();

        // Group points into time buckets
        for (ts, val) in points {
            let bucket_ts = (ts / agg.time_bucket) * agg.time_bucket;
            buckets.entry(bucket_ts).or_insert_with(Vec::new).push(*val);
        }

        // Aggregate each bucket
        let mut result: Vec<(i64, f64)> = buckets
            .into_iter()
            .map(|(ts, values)| {
                let agg_value = match agg.agg_type {
                    AggregationType::Avg => values.iter().sum::<f64>() / values.len() as f64,
                    AggregationType::Sum => values.iter().sum::<f64>(),
                    AggregationType::Min => values.iter().cloned().fold(f64::INFINITY, f64::min),
                    AggregationType::Max => {
                        values.iter().cloned().fold(f64::NEG_INFINITY, f64::max)
                    }
                    AggregationType::Count => values.len() as f64,
                    AggregationType::First => values.first().cloned().unwrap_or(0.0),
                    AggregationType::Last => values.last().cloned().unwrap_or(0.0),
                };
                (ts, agg_value)
            })
            .collect();

        result.sort_by_key(|(ts, _)| *ts);
        Ok(result)
    }
}

/// Geospatial filter for querying location data
pub struct GeospatialFilter<'a> {
    storage: &'a BlobStorage,
}

impl<'a> GeospatialFilter<'a> {
    pub fn new(storage: &'a BlobStorage) -> Self {
        Self { storage }
    }

    /// Get distance between two members
    pub async fn get_distance(
        &self,
        key: &str,
        member1: &str,
        member2: &str,
        unit: &str,
    ) -> Result<Option<f64>> {
        self.storage
            .geodist(key, member1, member2, Some(unit))
            .await
    }

    /// Search locations within radius
    pub async fn search_radius(
        &self,
        key: &str,
        longitude: f64,
        latitude: f64,
        radius: f64,
        unit: &str,
    ) -> Result<Vec<String>> {
        self.storage
            .georadius(key, longitude, latitude, radius, unit)
            .await
    }

    /// Search locations within radius with coordinates
    pub async fn search_radius_with_coords(
        &self,
        key: &str,
        longitude: f64,
        latitude: f64,
        radius: f64,
        unit: &str,
    ) -> Result<Vec<(String, f64, f64)>> {
        self.storage
            .georadius_with_coords(key, longitude, latitude, radius, unit)
            .await
    }

    /// Search by member with radius
    pub async fn search_by_member(
        &self,
        key: &str,
        member: &str,
        radius: f64,
        unit: &str,
    ) -> Result<Vec<String>> {
        self.storage
            .georadiusbymember(key, member, radius, unit)
            .await
    }

    /// Search by member with coordinates
    pub async fn search_by_member_with_coords(
        &self,
        key: &str,
        member: &str,
        radius: f64,
        unit: &str,
    ) -> Result<Vec<(String, f64, f64)>> {
        self.storage
            .georadiusbymember_with_coords(key, member, radius, unit)
            .await
    }
}

// Filter condition types
#[derive(Debug, Clone)]
pub enum FilterCondition {
    Eq(JsonValue),
    Ne(JsonValue),
    Gt(JsonValue),
    Gte(JsonValue),
    Lt(JsonValue),
    Lte(JsonValue),
    Contains(String),
    In(Vec<JsonValue>),
}

// JSON filter conditions container
#[derive(Debug, Clone, Default)]
pub struct JsonFilterConditions {
    conditions: Vec<(String, FilterCondition)>,
}

impl JsonFilterConditions {
    pub fn new() -> Self {
        Self {
            conditions: Vec::new(),
        }
    }

    pub fn add_condition(&mut self, field: String, condition: FilterCondition) {
        self.conditions.push((field, condition));
    }

    pub fn is_empty(&self) -> bool {
        self.conditions.is_empty()
    }
}

// Filter options
#[derive(Debug, Clone, Default)]
pub struct FilterOptions {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub sort_by: Option<String>,
    pub sort_order: SortOrder,
}

#[derive(Debug, Clone)]
pub enum SortOrder {
    Asc,
    Desc,
}

impl Default for SortOrder {
    fn default() -> Self {
        SortOrder::Asc
    }
}

// Time series options
#[derive(Debug, Clone, Default)]
pub struct TimeSeriesOptions {
    pub aggregation: Option<Aggregation>,
    pub count: Option<usize>,
    pub filter_by_ts: Option<Vec<i64>>,
    pub min_value: Option<f64>,
    pub max_value: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct Aggregation {
    pub agg_type: AggregationType,
    pub time_bucket: i64,
}

#[derive(Debug, Clone)]
pub enum AggregationType {
    Avg,
    Sum,
    Min,
    Max,
    Count,
    First,
    Last,
}
