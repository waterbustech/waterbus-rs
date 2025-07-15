use redis::{Commands, cluster::ClusterClient};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ClientMetadata {
    pub room_id: String,
    pub participant_id: String,
    pub sfu_node_id: String,
    pub node_addr: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CacheKey {
    pub key: String,
}

impl CacheKey {
    pub fn new(key: String) -> Self {
        CacheKey { key }
    }
}

#[derive(Clone)]
pub struct CacheManager {
    client: Arc<Mutex<ClusterClient>>,
}

impl CacheManager {
    pub fn new(urls: Vec<String>) -> Self {
        let client = ClusterClient::new(urls).unwrap();
        Self {
            client: Arc::new(Mutex::new(client)),
        }
    }

    pub fn insert(&self, key: CacheKey, value: &ClientMetadata) -> Result<(), redis::RedisError> {
        let mut conn = self.client.lock().unwrap().get_connection()?;
        let serialized_value = serde_json::to_string(value).map_err(|e| {
            redis::RedisError::from((
                redis::ErrorKind::TypeError,
                "serialization error",
                e.to_string(),
            ))
        })?;

        let _: () = conn.set(key.clone().key, serialized_value)?;

        let secondary_key = format!("participant_id:{}", value.participant_id);
        let _: () = conn.set(secondary_key, &key.key)?;

        Ok(())
    }

    pub fn get(&self, key: &CacheKey) -> Result<Option<ClientMetadata>, redis::RedisError> {
        let mut conn = self.client.lock().unwrap().get_connection()?;
        let result: Option<String> = conn.get(&key.key)?;
        match result {
            Some(s) => {
                let metadata: ClientMetadata = serde_json::from_str(&s).map_err(|e| {
                    redis::RedisError::from((
                        redis::ErrorKind::TypeError,
                        "deserialization error",
                        e.to_string(),
                    ))
                })?;

                Ok(Some(metadata))
            }
            None => Ok(None),
        }
    }

    pub fn get_by_participant_id(
        &self,
        participant_id: &str,
    ) -> Result<Option<ClientMetadata>, redis::RedisError> {
        let mut conn = self.client.lock().unwrap().get_connection()?;
        let key: Option<String> = conn.get(format!("participant_id:{participant_id}"))?;
        match key {
            Some(actual_key) => self.get(&CacheKey::new(actual_key)),
            None => Ok(None),
        }
    }

    pub fn remove(&self, key: &CacheKey) -> Result<(), redis::RedisError> {
        let mut conn = self.client.lock().unwrap().get_connection()?;

        if let Some(meta) = self.get(key)? {
            let _: () = conn.del(format!("participant_id:{}", meta.participant_id))?;
        }

        conn.del::<_, ()>(&key.key)?;

        Ok(())
    }

    pub fn contains_key(&self, key: &CacheKey) -> Result<bool, redis::RedisError> {
        let mut conn = self.client.lock().unwrap().get_connection()?;
        let exists: i64 = conn.exists(&key.key)?;
        Ok(exists == 1)
    }
}
