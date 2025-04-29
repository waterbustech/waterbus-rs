use diesel::{
    PgConnection, RunQueryDsl,
    r2d2::{ConnectionManager, Pool, PooledConnection},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tracing::{info, warn};
use typesense_client::TypesenseClient;

use crate::core::{
    database::schema::users, entities::models::User, types::errors::general::GeneralError,
};

#[derive(Serialize, Deserialize)]
struct TypesenseUser {
    #[serde(rename = "id")]
    id: String,

    #[serde(rename = "userName")]
    username: String,

    #[serde(rename = "fullName")]
    full_name: Option<String>,

    #[serde(rename = "avatar")]
    avatar_url: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SearchService {
    client: TypesenseClient,
    pool: Pool<ConnectionManager<PgConnection>>,
}

impl SearchService {
    pub fn new(client: TypesenseClient, pool: Pool<ConnectionManager<PgConnection>>) -> Self {
        Self {
            client: client,
            pool: pool,
        }
    }

    pub async fn init(&self) {
        self._sync_db_to_typesense().await;
    }

    pub async fn search_users(
        &self,
        query: &str,
        page: Option<i64>,
        per_page: Option<i64>,
    ) -> Result<Value, GeneralError> {
        self.client
            .search_documents("users", query, "userName,fullName", page, per_page)
            .await
            .map_err(|err| {
                warn!("[typesense] Search error: {}", err);
                GeneralError::DbConnectionError
            })
    }

    fn _get_conn(&self) -> Result<PooledConnection<ConnectionManager<PgConnection>>, GeneralError> {
        self.pool.get().map_err(|_| GeneralError::DbConnectionError)
    }

    async fn _sync_db_to_typesense(&self) {
        if let Err(err) = self.client.delete_collection("users").await {
            eprintln!("Failed to delete: {}", err);
        }

        let schema = json!({
            "name": "users",
            "fields": [
                { "name": "id", "type": "int32", "facet": false },
                { "name": "userName", "type": "string", "facet": false },
                { "name": "fullName", "type": "string", "facet": false },
                { "name": "avatar", "type": "string", "facet": false, "optional": true }
            ]
        });

        match self.client.create_collection(&schema).await {
            Ok(_) => {
                info!("[typesense] Created collection");

                let mut conn = match self._get_conn() {
                    Ok(conn) => conn,
                    Err(err) => {
                        warn!("Failed to get DB connection: {:?}", err);
                        return;
                    }
                };

                match users::table.load::<User>(&mut conn) {
                    Ok(user_entities) => {
                        let typesense_users: Vec<TypesenseUser> = user_entities
                            .clone()
                            .into_iter()
                            .map(|u| TypesenseUser {
                                id: u.id.to_string(),
                                username: u.user_name,
                                full_name: u.full_name,
                                avatar_url: u.avatar,
                            })
                            .collect();

                        let jsonl = typesense_users
                            .into_iter()
                            .map(|u| serde_json::to_string(&u).unwrap())
                            .collect::<Vec<_>>()
                            .join("\n");

                        match self.client.import_documents("users", &jsonl).await {
                            Ok(_) => {
                                info!("[typesense] Imported user documents");
                            }
                            Err(err) => warn!("[typesense] Failed to import documents: {}", err),
                        }
                    }
                    Err(err) => {
                        warn!("Failed to query users from DB: {:?}", err);
                    }
                }
            }
            Err(err) => {
                warn!("[typesense] Failed to create collection: {}", err);
            }
        }
    }
}
