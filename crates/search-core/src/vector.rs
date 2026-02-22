use ahash::AHasher;
use anyhow::Result;
use common::CodeChunk;
use qdrant_client::{
    Qdrant,
    qdrant::{
        CreateCollectionBuilder, DeletePointsBuilder, Distance, PointStruct, PointsIdsList,
        QuantizationType, QueryPointsBuilder, ScalarQuantizationBuilder, UpsertPointsBuilder,
        VectorParamsBuilder, value::Kind,
    },
};
use std::hash::{Hash, Hasher};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VectorSearchConfig {
    pub collection: String,
    pub distance: Distance,
    pub hnsw_m: u64,
    pub hnsw_ef_construct: u64,
    pub vector_dim: usize,
    pub quantization: QuantizationMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuantizationMode {
    None,
    Int8,
    UInt8,
}

impl Default for VectorSearchConfig {
    fn default() -> Self {
        Self {
            collection: "code_chunks".to_string(),
            distance: Distance::Cosine,
            hnsw_m: 16,
            hnsw_ef_construct: 100,
            vector_dim: 384,
            quantization: QuantizationMode::Int8,
        }
    }
}

pub struct QdrantVectorStore {
    config: VectorSearchConfig,
}

impl QdrantVectorStore {
    pub fn new(config: VectorSearchConfig) -> Self {
        Self { config }
    }

    pub async fn ensure_collection(&self, client: &Qdrant) -> Result<()> {
        let mut builder =
            CreateCollectionBuilder::new(self.config.collection.clone()).vectors_config(
                VectorParamsBuilder::new(self.config.vector_dim as u64, self.config.distance),
            );
        builder = match self.config.quantization {
            QuantizationMode::None => builder,
            QuantizationMode::Int8 => {
                builder.quantization_config(ScalarQuantizationBuilder::default())
            }
            QuantizationMode::UInt8 => builder.quantization_config(
                ScalarQuantizationBuilder::default().r#type(QuantizationType::Int8.into()),
            ),
        };

        let result = client.create_collection(builder).await;
        if let Err(err) = result {
            let msg = err.to_string().to_lowercase();
            if !msg.contains("already exists") {
                return Err(err.into());
            }
        }
        Ok(())
    }

    pub async fn upsert_chunks(
        &self,
        client: &Qdrant,
        chunks: &[CodeChunk],
        vectors: &[Vec<f32>],
    ) -> Result<()> {
        let points = chunks
            .iter()
            .zip(vectors.iter())
            .map(|(chunk, vector)| {
                PointStruct::new(
                    hash_id(&chunk.id),
                    vector.clone(),
                    [
                        ("path", chunk.file_path.clone().into()),
                        ("chunk_id", chunk.id.clone().into()),
                    ],
                )
            })
            .collect::<Vec<_>>();

        client
            .upsert_points(
                UpsertPointsBuilder::new(self.config.collection.clone(), points).wait(true),
            )
            .await?;
        Ok(())
    }

    pub async fn delete_points(&self, client: &Qdrant, ids: &[String]) -> Result<()> {
        let point_ids = ids.iter().map(|id| hash_id(id)).collect::<Vec<_>>();
        client
            .delete_points(
                DeletePointsBuilder::new(self.config.collection.clone())
                    .points(PointsIdsList {
                        ids: point_ids.into_iter().map(Into::into).collect(),
                    })
                    .wait(true),
            )
            .await?;
        Ok(())
    }

    pub async fn search_similar_ids(
        &self,
        client: &Qdrant,
        query_vector: Vec<f32>,
        top_k: usize,
    ) -> Result<Vec<String>> {
        let response = client
            .query(
                QueryPointsBuilder::new(self.config.collection.clone())
                    .query(query_vector)
                    .limit(top_k as u64)
                    .with_payload(true),
            )
            .await?;

        let ids = response
            .result
            .iter()
            .filter_map(|pt| pt.payload.get("chunk_id"))
            .filter_map(|value| value.kind.as_ref())
            .filter_map(|kind| match kind {
                Kind::StringValue(v) => Some(v.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();

        Ok(ids)
    }
}

fn hash_id(id: &str) -> u64 {
    let mut h = AHasher::default();
    id.hash(&mut h);
    h.finish()
}

#[cfg(test)]
mod tests {
    use qdrant_client::qdrant::Distance;

    use super::{VectorSearchConfig, hash_id};

    #[test]
    fn defaults_to_cosine_and_hnsw_baseline() {
        let cfg = VectorSearchConfig::default();
        assert_eq!(cfg.distance, Distance::Cosine);
        assert_eq!(cfg.hnsw_m, 16);
        assert_eq!(cfg.hnsw_ef_construct, 100);
    }

    #[test]
    fn hash_id_is_stable() {
        assert_eq!(hash_id("chunk-1"), hash_id("chunk-1"));
        assert_ne!(hash_id("chunk-1"), hash_id("chunk-2"));
    }
}
