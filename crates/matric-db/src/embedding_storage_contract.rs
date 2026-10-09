use matric_core::{
    validate_embedding_dimension, validate_embedding_values, EmbeddingVectorType, Error, Result,
};
use pgvector::Vector;
use sqlx::{PgConnection, Row};
use uuid::Uuid;

#[derive(Debug, Clone, Copy)]
pub(crate) struct EmbeddingStorageContract {
    pub embedding_set_id: Uuid,
    pub dimension: usize,
    pub vector_type: EmbeddingVectorType,
    pub scope: ContractScope,
}

/// Which embedding rows a similarity query may compare against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ContractScope {
    /// An explicitly selected embedding set.
    Set,
    /// Unscoped search: the default set plus legacy rows with no set,
    /// restricted to the default set's dimension.
    DefaultWithLegacy,
    /// Unscoped search where no default set is configured: any row whose
    /// dimension matches the query vector (pre-#1175 behavior).
    QueryDimension,
}

impl EmbeddingStorageContract {
    pub(crate) fn storage_param(&self, placeholder: &str) -> String {
        match self.vector_type {
            EmbeddingVectorType::Vector => format!("{placeholder}::vector({})", self.dimension),
            EmbeddingVectorType::Halfvec => {
                format!(
                    "({placeholder}::vector::halfvec({})::vector)",
                    self.dimension
                )
            }
        }
    }

    pub(crate) fn similarity_lhs(&self, column: &str) -> String {
        match self.vector_type {
            EmbeddingVectorType::Vector => {
                format!("({column}::vector({}))", self.dimension)
            }
            EmbeddingVectorType::Halfvec => {
                format!("({column}::halfvec({}))", self.dimension)
            }
        }
    }

    pub(crate) fn similarity_rhs(&self, placeholder: &str) -> String {
        match self.vector_type {
            EmbeddingVectorType::Vector => {
                format!("{placeholder}::vector({})", self.dimension)
            }
            EmbeddingVectorType::Halfvec => {
                format!("({placeholder}::vector::halfvec({}))", self.dimension)
            }
        }
    }

    pub(crate) fn distance_expr(&self, column: &str, placeholder: &str) -> String {
        format!(
            "{} <=> {}",
            self.similarity_lhs(column),
            self.similarity_rhs(placeholder)
        )
    }

    /// SQL predicate selecting the rows this contract may compare against.
    /// `placeholder` is always referenced, so the bound set id keeps a type.
    pub(crate) fn set_predicate(&self, alias: &str, placeholder: &str) -> String {
        let dims = format!("vector_dims({alias}.vector) = {}", self.dimension);
        match self.scope {
            ContractScope::Set => format!("{alias}.embedding_set_id = {placeholder}"),
            ContractScope::DefaultWithLegacy => format!(
                "({alias}.embedding_set_id = {placeholder} OR {alias}.embedding_set_id IS NULL) AND {dims}"
            ),
            ContractScope::QueryDimension => {
                format!("({placeholder}::uuid IS NULL OR TRUE) AND {dims}")
            }
        }
    }

    pub(crate) fn validate_vector(&self, vector: &Vector) -> Result<()> {
        validate_embedding_values(vector.as_slice(), self.dimension)
            .map_err(|error| Error::InvalidInput(error.to_string()))
    }
}

pub(crate) async fn contract_for_set(
    connection: &mut PgConnection,
    embedding_set_id: Uuid,
) -> Result<EmbeddingStorageContract> {
    let row = sqlx::query(
        r#"
        SELECT es.id AS embedding_set_id, ec.dimension, ec.vector_type
        FROM embedding_set es
        JOIN public.embedding_config ec ON ec.id = es.embedding_config_id
        WHERE es.id = $1
        "#,
    )
    .bind(embedding_set_id)
    .fetch_optional(connection)
    .await
    .map_err(Error::Database)?
    .ok_or_else(|| Error::NotFound("Embedding set config not found".to_string()))?;

    contract_from_row(&row)
}

pub(crate) async fn default_contract(
    connection: &mut PgConnection,
    query_dimension: usize,
) -> Result<EmbeddingStorageContract> {
    let row = sqlx::query(
        r#"
        SELECT es.id AS embedding_set_id, ec.dimension, ec.vector_type
        FROM embedding_set es
        JOIN public.embedding_config ec ON ec.id = es.embedding_config_id
        WHERE es.id = get_default_embedding_set_id()
        "#,
    )
    .fetch_optional(connection)
    .await
    .map_err(Error::Database)?;

    match row {
        Some(row) => {
            let mut contract = contract_from_row(&row)?;
            contract.scope = ContractScope::DefaultWithLegacy;
            Ok(contract)
        }
        None => Ok(EmbeddingStorageContract {
            embedding_set_id: Uuid::nil(),
            dimension: query_dimension,
            vector_type: EmbeddingVectorType::Vector,
            scope: ContractScope::QueryDimension,
        }),
    }
}

fn contract_from_row(row: &sqlx::postgres::PgRow) -> Result<EmbeddingStorageContract> {
    let embedding_set_id = row.get("embedding_set_id");
    let dimension: i32 = row.get("dimension");
    let dimension = usize::try_from(dimension)
        .map_err(|_| Error::InvalidInput("embedding dimension must be positive".to_string()))?;
    let vector_type = row
        .get::<String, _>("vector_type")
        .parse::<EmbeddingVectorType>()
        .map_err(|error| Error::InvalidInput(error.to_string()))?;
    validate_embedding_dimension(dimension, vector_type)
        .map_err(|error| Error::InvalidInput(error.to_string()))?;
    Ok(EmbeddingStorageContract {
        embedding_set_id,
        dimension,
        vector_type,
        scope: ContractScope::Set,
    })
}
