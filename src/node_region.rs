use once_cell::sync::OnceCell;
use serde::Deserialize;
use std::collections::HashMap;

/// Global cached node region
static NODE_REGION: OnceCell<String> = OnceCell::new();

#[derive(Debug, Deserialize)]
struct IpApiResponse {
    #[serde(rename = "countryCode")]
    country_code: String,
}

/// Get the cached node region
pub fn get_node_region() -> String {
    NODE_REGION
        .get()
        .cloned()
        .unwrap_or_else(|| "unknown".to_string())
}

/// Fetch node region from ip-api.com and map to AWS regions
pub async fn fetch_and_set_node_region() -> String {
    match fetch_region_internal().await {
        Ok(region) => {
            tracing::info!("Node region: {}", region);
            let _ = NODE_REGION.set(region.clone());
            region
        }
        Err(e) => {
            tracing::error!("Error getting node location on startup: {}", e);
            let unknown = "unknown".to_string();
            let _ = NODE_REGION.set(unknown.clone());
            unknown
        }
    }
}

async fn fetch_region_internal() -> anyhow::Result<String> {
    let response = reqwest::get("http://ip-api.com/json/")
        .await?
        .json::<IpApiResponse>()
        .await?;

    let country_code = response.country_code;

    // Map country codes to AWS regions (same as JS implementation)
    let aws_region_map: HashMap<&str, &str> = [
        ("US", "us-east-1"),
        ("CA", "ca-central-1"),
        ("BR", "sa-east-1"),
        ("IE", "eu-west-1"),
        ("GB", "eu-west-2"),
        ("FR", "eu-west-3"),
        ("DE", "eu-central-1"),
        ("IT", "eu-south-1"),
        ("SE", "eu-north-1"),
        ("IN", "ap-south-1"),
        ("SG", "ap-southeast-1"),
        ("AU", "ap-southeast-2"),
        ("JP", "ap-northeast-1"),
        ("KR", "ap-northeast-2"),
        ("ZA", "af-south-1"),
        ("AE", "me-south-1"),
    ]
    .iter()
    .copied()
    .collect();

    let region = aws_region_map
        .get(country_code.as_str())
        .map(|&s| s.to_string())
        .unwrap_or_else(|| format!("{}-region-1", country_code.to_lowercase()));

    Ok(region)
}
