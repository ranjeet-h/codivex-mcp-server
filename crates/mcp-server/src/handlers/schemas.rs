use axum::Json;
use common::schema_bundle;

pub async fn schemas_handler() -> Json<common::SchemaBundle> {
    Json(schema_bundle())
}
