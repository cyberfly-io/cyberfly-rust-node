//! GraphQL schema extensions for secondary indexing

use async_graphql::{Context, Object, Result, SimpleObject};
use crate::indexing::{IndexManager, IndexType, QueryOperator, IndexStats};

/// Input for creating a new index
#[derive(Debug, Clone)]
pub struct CreateIndexInput {
    pub db_name: String,
    pub index_name: String,
    pub field: String,
    pub index_type: String, // "exact", "range", "fulltext", "geo"
}

#[Object]
impl CreateIndexInput {
    async fn db_name(&self) -> &str {
        &self.db_name
    }
    
    async fn index_name(&self) -> &str {
        &self.index_name
    }
    
    async fn field(&self) -> &str {
        &self.field
    }
    
    async fn index_type(&self) -> &str {
        &self.index_type
    }
}

/// Input for querying an index
#[derive(Debug, Clone)]
pub struct QueryIndexInput {
    pub db_name: String,
    pub index_name: String,
    pub operator: String, // "equals", "gt", "lt", "between", "in", "contains", "startswith"
    pub value: Option<String>,
    pub values: Option<Vec<String>>,
    pub min: Option<f64>,
    pub max: Option<f64>,
}

#[Object]
impl QueryIndexInput {
    async fn db_name(&self) -> &str {
        &self.db_name
    }
    
    async fn index_name(&self) -> &str {
        &self.index_name
    }
    
    async fn operator(&self) -> &str {
        &self.operator
    }
}

/// Index query result for GraphQL
#[derive(SimpleObject)]
pub struct IndexQueryResult {
    pub keys: Vec<String>,
    pub count: i32,
    pub execution_time_ms: i32,
}

/// Index statistics for GraphQL
#[derive(SimpleObject)]
pub struct IndexStatsResult {
    pub name: String,
    pub field: String,
    pub index_type: String,
    pub total_keys: i32,
    pub unique_values: i32,
}

/// Convert string to IndexType
fn parse_index_type(type_str: &str) -> Result<IndexType> {
    match type_str.to_lowercase().as_str() {
        "exact" => Ok(IndexType::Exact),
        "range" => Ok(IndexType::Range),
        "fulltext" | "full_text" => Ok(IndexType::FullText),
        "geo" => Ok(IndexType::Geo),
        _ => Err(async_graphql::Error::new(format!(
            "Invalid index type: {}. Use 'exact', 'range', 'fulltext', or 'geo'",
            type_str
        ))),
    }
}

/// Convert query input to QueryOperator
fn parse_query_operator(input: &QueryIndexInput) -> Result<QueryOperator> {
    match input.operator.to_lowercase().as_str() {
        "equals" | "eq" => {
            let value = input.value.clone().ok_or_else(|| {
                async_graphql::Error::new("'value' required for equals operator")
            })?;
            Ok(QueryOperator::Equals(value))
        }
        "gt" | "greaterthan" => {
            let value = input.min.ok_or_else(|| {
                async_graphql::Error::new("'min' required for gt operator")
            })?;
            Ok(QueryOperator::GreaterThan(value))
        }
        "lt" | "lessthan" => {
            let value = input.max.ok_or_else(|| {
                async_graphql::Error::new("'max' required for lt operator")
            })?;
            Ok(QueryOperator::LessThan(value))
        }
        "between" | "range" => {
            let min = input.min.ok_or_else(|| {
                async_graphql::Error::new("'min' required for between operator")
            })?;
            let max = input.max.ok_or_else(|| {
                async_graphql::Error::new("'max' required for between operator")
            })?;
            Ok(QueryOperator::Between(min, max))
        }
        "in" => {
            let values = input.values.clone().ok_or_else(|| {
                async_graphql::Error::new("'values' required for in operator")
            })?;
            Ok(QueryOperator::In(values))
        }
        "contains" => {
            let value = input.value.clone().ok_or_else(|| {
                async_graphql::Error::new("'value' required for contains operator")
            })?;
            Ok(QueryOperator::Contains(value))
        }
        "startswith" | "prefix" => {
            let value = input.value.clone().ok_or_else(|| {
                async_graphql::Error::new("'value' required for startswith operator")
            })?;
            Ok(QueryOperator::StartsWith(value))
        }
        _ => Err(async_graphql::Error::new(format!(
            "Invalid operator: {}. Use 'equals', 'gt', 'lt', 'between', 'in', 'contains', or 'startswith'",
            input.operator
        ))),
    }
}

/// Queries for indexing
#[derive(Default)]
pub struct IndexQuery;

#[Object]
impl IndexQuery {
    /// Query an index
    async fn query_index(
        &self,
        ctx: &Context<'_>,
        db_name: String,
        index_name: String,
        operator: String,
        value: Option<String>,
        values: Option<Vec<String>>,
        min: Option<f64>,
        max: Option<f64>,
    ) -> Result<IndexQueryResult> {
        let index_manager = ctx.data::<IndexManager>()?;
        
        let input = QueryIndexInput {
            db_name: db_name.clone(),
            index_name: index_name.clone(),
            operator,
            value,
            values,
            min,
            max,
        };
        
        let query_op = parse_query_operator(&input)?;
        let result = index_manager.query_index(&db_name, &index_name, query_op).await?;
        
        Ok(IndexQueryResult {
            keys: result.keys,
            count: result.count as i32,
            execution_time_ms: result.execution_time_ms as i32,
        })
    }

    /// List all indexes in a database
    async fn list_indexes(&self, ctx: &Context<'_>, db_name: String) -> Result<Vec<String>> {
        let index_manager = ctx.data::<IndexManager>()?;
        Ok(index_manager.list_indexes(&db_name).await)
    }

    /// Get index statistics
    async fn get_index_stats(
        &self,
        ctx: &Context<'_>,
        db_name: String,
        index_name: String,
    ) -> Result<IndexStatsResult> {
        let index_manager = ctx.data::<IndexManager>()?;
        let stats = index_manager.get_index_stats(&db_name, &index_name).await?;
        
        let index_type_str = match stats.index_type {
            IndexType::Exact => "exact",
            IndexType::Range => "range",
            IndexType::FullText => "fulltext",
            IndexType::Geo => "geo",
        };
        
        Ok(IndexStatsResult {
            name: stats.name,
            field: stats.field,
            index_type: index_type_str.to_string(),
            total_keys: stats.total_keys as i32,
            unique_values: stats.unique_values as i32,
        })
    }
}

/// Mutations for indexing
#[derive(Default)]
pub struct IndexMutation;

#[Object]
impl IndexMutation {
    /// Create a new index
    async fn create_index(
        &self,
        ctx: &Context<'_>,
        db_name: String,
        index_name: String,
        field: String,
        index_type: String,
    ) -> Result<bool> {
        let index_manager = ctx.data::<IndexManager>()?;
        let idx_type = parse_index_type(&index_type)?;
        
        index_manager
            .create_index(db_name, index_name, field, idx_type)
            .await?;
        
        Ok(true)
    }

    /// Drop an index
    async fn drop_index(
        &self,
        ctx: &Context<'_>,
        db_name: String,
        index_name: String,
    ) -> Result<bool> {
        let index_manager = ctx.data::<IndexManager>()?;
        index_manager.drop_index(&db_name, &index_name).await?;
        Ok(true)
    }
}
