//! Dosya hash hesaplama API uçlarını yönetir.
use serde::Deserialize;
use serde_json::Value;

use crate::hash::{self, HashAlgorithm};
use crate::server::{Response, json_error, json_ok};

/// Dosya hash hesaplama API isteğini işler.
pub fn hash_endpoint(body: &[u8]) -> Response {
    #[derive(Deserialize)]
    struct HashRequest {
        path: String,
        algorithms: Option<Vec<String>>,
    }

    let request: HashRequest = match serde_json::from_slice(body) {
        Ok(request) => request,
        Err(err) => return json_error(400, err.to_string()),
    };

    let algorithms = match parse_algorithms(request.algorithms) {
        Ok(algorithms) => algorithms,
        Err(message) => return json_error(400, message),
    };

    match hash::calculate_multiple(&request.path, &algorithms) {
        Ok(results) => {
            let mut value = serde_json::Map::new();
            for result in results {
                value.insert(
                    result.algorithm.name().to_ascii_lowercase(),
                    Value::String(result.value),
                );
            }
            json_ok(Value::Object(value))
        }
        Err(err) => json_error(500, err.to_string()),
    }
}

/// API'den gelen hash algoritması stringlerini enum listesine çevirir.
fn parse_algorithms(values: Option<Vec<String>>) -> Result<Vec<HashAlgorithm>, String> {
    let Some(list) = values else {
        return Ok(vec![
            HashAlgorithm::Md5,
            HashAlgorithm::Sha1,
            HashAlgorithm::Sha256,
            HashAlgorithm::Sha512,
        ]);
    };
    list.iter()
        .map(|v| HashAlgorithm::parse(v).ok_or_else(|| format!("unsupported hash algorithm: {v}")))
        .collect()
}
