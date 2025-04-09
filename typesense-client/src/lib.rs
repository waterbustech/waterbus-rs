use reqwest::Client;
use serde_json::Value;

pub struct TypesenseClient {
    host: String,
    api_key: String,
    http: Client,
}

impl TypesenseClient {
    pub fn new(host: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self {
            host: host.into(),
            api_key: api_key.into(),
            http: Client::new(),
        }
    }

    fn url(&self, path: &str) -> String {
        format!(
            "{}/{}",
            self.host.trim_end_matches('/'),
            path.trim_start_matches('/')
        )
    }

    fn auth_header(&self) -> (&'static str, String) {
        ("X-TYPESENSE-API-KEY", self.api_key.clone())
    }

    pub async fn create_collection(&self, schema: &Value) -> Result<Value, reqwest::Error> {
        let res = self
            .http
            .post(self.url("/collections"))
            .header(self.auth_header().0, self.auth_header().1)
            .json(schema)
            .send()
            .await?
            .json()
            .await?;

        Ok(res)
    }

    pub async fn import_documents(
        &self,
        collection: &str,
        docs: &str,
    ) -> Result<String, reqwest::Error> {
        let url = format!("/collections/{}/documents/import?action=upsert", collection);
        let res = self
            .http
            .post(self.url(&url))
            .header(self.auth_header().0, self.auth_header().1)
            .header("Content-Type", "text/plain")
            .body(docs.to_string())
            .send()
            .await?
            .text()
            .await?;

        Ok(res)
    }

    pub async fn search_documents(
        &self,
        collection: &str,
        q: &str,
        query_by: &str,
    ) -> Result<Value, reqwest::Error> {
        let url = format!(
            "/collections/{}/documents/search?q={}&query_by={}",
            collection, q, query_by
        );

        let res = self
            .http
            .get(self.url(&url))
            .header(self.auth_header().0, self.auth_header().1)
            .send()
            .await?
            .json()
            .await?;

        Ok(res)
    }
}
