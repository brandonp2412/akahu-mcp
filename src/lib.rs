use std::{env, time::Duration as StdDuration};

use chrono::{DateTime, Duration, SecondsFormat, Utc};
use reqwest::StatusCode;
use rmcp::{
    ErrorData, Json, ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{Implementation, ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router,
};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Map, Value, json};
use tokio::time::sleep;
use url::Url;

const API_BASE: &str = "https://api.akahu.io/v1/";
const MAX_PAGES: usize = 100;
const MAX_RESULT_LIMIT: usize = 1000;
const MAX_ATTEMPTS: usize = 3;
const REDACTED: &str = "[REDACTED]";

#[derive(Debug, Clone)]
struct AkahuClient {
    http: reqwest::Client,
    base_url: Url,
    access_token: String,
    app_id: String,
}

impl AkahuClient {
    fn from_env() -> Result<Self, String> {
        let access_token = env::var("AKAHU_ACCESS_TOKEN").unwrap_or_default();
        let app_id = env::var("AKAHU_APP_ID_TOKEN").unwrap_or_default();
        if access_token.is_empty() || app_id.is_empty() {
            return Err("Akahu credentials are not configured".to_string());
        }

        let http = reqwest::Client::builder()
            .connect_timeout(StdDuration::from_secs(10))
            .timeout(StdDuration::from_secs(30))
            .build()
            .map_err(|_| "Failed to initialize Akahu HTTP client".to_string())?;
        let base_url =
            Url::parse(API_BASE).map_err(|_| "Akahu API base URL is invalid".to_string())?;

        Ok(Self {
            http,
            base_url,
            access_token,
            app_id,
        })
    }

    fn endpoint(&self, segments: &[&str]) -> Result<Url, String> {
        let mut url = self.base_url.clone();
        let mut path = url
            .path_segments_mut()
            .map_err(|_| "Akahu API base URL cannot accept path segments".to_string())?;
        path.pop_if_empty();
        path.extend(segments.iter().copied());
        drop(path);
        Ok(url)
    }

    async fn request(
        &self,
        segments: &[&str],
        params: &[(String, String)],
    ) -> Result<Value, String> {
        let url = self.endpoint(segments)?;

        for attempt in 0..MAX_ATTEMPTS {
            let response = self
                .http
                .get(url.clone())
                .bearer_auth(&self.access_token)
                .header("X-Akahu-Id", &self.app_id)
                .header("Accept", "application/json")
                .query(params)
                .send()
                .await;

            let response = match response {
                Ok(response) => response,
                Err(_) if attempt + 1 < MAX_ATTEMPTS => {
                    sleep(backoff(attempt)).await;
                    continue;
                }
                Err(_) => return Err("Akahu request failed due to a network error".to_string()),
            };

            let status = response.status();
            if status == StatusCode::UNAUTHORIZED {
                return Err("Akahu access token is invalid or expired".to_string());
            }
            if status == StatusCode::FORBIDDEN {
                return Err("Akahu permissions or app ID header are insufficient".to_string());
            }
            if (status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error())
                && attempt + 1 < MAX_ATTEMPTS
            {
                sleep(retry_delay(&response, attempt)).await;
                continue;
            }
            if !status.is_success() {
                return Err(format!("Akahu API returned HTTP {}", status.as_u16()));
            }

            return response
                .json::<Value>()
                .await
                .map_err(|_| "Akahu API returned invalid JSON".to_string());
        }

        Err("Akahu request retry limit reached".to_string())
    }

    async fn paged(
        &self,
        segments: &[&str],
        params: &[(String, String)],
        limit: Option<usize>,
    ) -> Result<Value, String> {
        let limit = limit.map(|value| value.clamp(1, MAX_RESULT_LIMIT));
        let mut items = Vec::new();
        let mut cursor: Option<String> = None;

        for _ in 0..MAX_PAGES {
            let mut current = params.to_vec();
            if let Some(cursor) = &cursor {
                current.push(("cursor".to_string(), cursor.clone()));
            }

            let payload = self.request(segments, &current).await?;
            if let Some(page_items) = payload
                .get("items")
                .or_else(|| payload.get("data"))
                .and_then(Value::as_array)
            {
                items.extend(page_items.iter().cloned());
            }

            if let Some(limit) = limit
                && items.len() >= limit
            {
                items.truncate(limit);
                return Ok(json!({
                    "items": items,
                    "count": limit,
                    "pages_complete": true,
                    "result_limited": true
                }));
            }

            cursor = next_cursor(&payload);
            if cursor.is_none() {
                return Ok(json!({
                    "count": items.len(),
                    "items": items,
                    "pages_complete": true,
                    "result_limited": false
                }));
            }
        }

        Ok(json!({
            "count": items.len(),
            "items": items,
            "pages_complete": false,
            "warning": "Safety page limit reached"
        }))
    }
}

fn next_cursor(payload: &Value) -> Option<String> {
    for key in ["cursor", "next_cursor"] {
        if let Some(value) = payload.get(key) {
            if let Some(next) = value.get("next").and_then(Value::as_str) {
                return Some(next.to_string());
            }
            if let Some(next) = value.as_str() {
                return Some(next.to_string());
            }
        }
    }
    None
}

fn backoff(attempt: usize) -> StdDuration {
    StdDuration::from_millis(250 * (1_u64 << attempt.min(4)))
}

fn retry_delay(response: &reqwest::Response, attempt: usize) -> StdDuration {
    let fallback = backoff(attempt);
    response
        .headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|seconds| *seconds <= 30)
        .map(StdDuration::from_secs)
        .unwrap_or(fallback)
}

fn resolve_dates(start: Option<&str>, end: Option<&str>) -> Result<(String, String), String> {
    let now = Utc::now();
    let end = match end {
        Some(value) => {
            DateTime::parse_from_rfc3339(value)
                .map_err(|_| "end must be an ISO 8601 timestamp".to_string())?;
            value.to_string()
        }
        None => now.to_rfc3339_opts(SecondsFormat::Millis, true),
    };
    let start = match start {
        Some(value) => {
            DateTime::parse_from_rfc3339(value)
                .map_err(|_| "start must be an ISO 8601 timestamp".to_string())?;
            value.to_string()
        }
        None => (now - Duration::days(30)).to_rfc3339_opts(SecondsFormat::Millis, true),
    };
    Ok((start, end))
}

fn pii_masking_enabled(value: Option<&str>) -> bool {
    !matches!(
        value.map(str::trim).map(str::to_ascii_lowercase).as_deref(),
        Some("0" | "false" | "no" | "off")
    )
}

fn compact_key(key: &str) -> String {
    key.chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .map(|character| character.to_ascii_lowercase())
        .collect()
}

fn is_sensitive_key(key: &str) -> bool {
    matches!(
        compact_key(key).as_str(),
        "formattedaccount"
            | "accountnumber"
            | "bankaccount"
            | "bankaccountnumber"
            | "iban"
            | "authorisation"
            | "authorization"
            | "credential"
            | "credentials"
            | "accesstoken"
            | "appidtoken"
            | "secret"
            | "user"
            | "userid"
            | "hash"
            | "cardnumber"
            | "cardsuffix"
            | "pan"
            | "email"
            | "emailaddress"
            | "phone"
            | "phonenumber"
            | "mobile"
            | "mobilenumber"
            | "address"
            | "postaladdress"
            | "streetaddress"
            | "accountholder"
            | "holdername"
            | "ownername"
            | "legalname"
            | "firstname"
            | "lastname"
            | "givenname"
            | "familyname"
            | "fullname"
    )
}

fn is_person_name_context(key: &str) -> bool {
    matches!(
        compact_key(key).as_str(),
        "account"
            | "otheraccount"
            | "counterparty"
            | "payer"
            | "payee"
            | "contact"
            | "accountholder"
            | "holder"
            | "owner"
            | "user"
            | "beneficiary"
            | "sender"
            | "recipient"
    )
}

fn redact(value: &mut Value) {
    *value = Value::String(REDACTED.to_string());
}

fn mask_nested_pii(value: &mut Value, parent_key: Option<&str>) {
    match value {
        Value::Object(object) => {
            let mask_names = parent_key.is_some_and(is_person_name_context);
            for (key, child) in object {
                if is_sensitive_key(key) || (compact_key(key) == "name" && mask_names) {
                    redact(child);
                } else {
                    mask_nested_pii(child, Some(key));
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                mask_nested_pii(item, parent_key);
            }
        }
        _ => {}
    }
}

fn mask_account_payload(value: &mut Value) {
    mask_nested_pii(value, None);

    for collection_key in ["items", "data"] {
        if let Some(items) = value.get_mut(collection_key).and_then(Value::as_array_mut) {
            for account in items {
                if let Some(name) = account.get_mut("name") {
                    redact(name);
                }
            }
        }
    }
}

fn apply_pii_policy(mut value: Value, mask_pii: bool, accounts_payload: bool) -> Value {
    if mask_pii {
        if accounts_payload {
            mask_account_payload(&mut value);
        } else {
            mask_nested_pii(&mut value, None);
        }
    }
    value
}

fn as_tool_error(message: String) -> ErrorData {
    ErrorData::internal_error(message, None)
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetTransactionsRequest {
    #[serde(default)]
    #[schemars(description = "UTC ISO 8601 lower bound; exclusive. Defaults to 30 days ago.")]
    pub start: Option<String>,
    #[serde(default)]
    #[schemars(description = "UTC ISO 8601 upper bound; inclusive. Defaults to now.")]
    pub end: Option<String>,
    #[serde(default)]
    #[schemars(description = "Optional Akahu account ID; omit for all accounts.")]
    pub account_id: Option<String>,
    #[serde(default)]
    #[schemars(
        description = "Optional maximum number of transactions to return, clamped to 1..1000."
    )]
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetPendingTransactionsRequest {
    #[serde(default)]
    #[schemars(description = "Optional Akahu account ID; omit for all accounts.")]
    pub account_id: Option<String>,
    #[serde(default)]
    #[schemars(
        description = "Optional maximum number of pending transactions to return, clamped to 1..1000."
    )]
    pub limit: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct AkahuServer {
    client: AkahuClient,
    mask_pii: bool,
    tool_router: ToolRouter<Self>,
}

impl AkahuServer {
    pub fn from_env() -> Result<Self, String> {
        let mask_pii = pii_masking_enabled(env::var("AKAHU_MASK_PII").ok().as_deref());
        Ok(Self {
            client: AkahuClient::from_env()?,
            mask_pii,
            tool_router: Self::tool_router(),
        })
    }

    pub async fn health_check(&self) -> Result<(), String> {
        self.client.request(&["accounts"], &[]).await.map(|_| ())
    }
}

#[tool_router]
impl AkahuServer {
    #[tool(
        description = "List connected accounts, balances, types, status, and attributes. Personally identifying fields are masked by default.",
        annotations(title = "List Akahu accounts", read_only_hint = true)
    )]
    async fn list_accounts(&self) -> Result<Json<Value>, ErrorData> {
        let payload = self
            .client
            .request(&["accounts"], &[])
            .await
            .map_err(as_tool_error)?;
        let mut payload = apply_pii_policy(payload, self.mask_pii, true);
        if let Some(object) = payload.as_object_mut() {
            object.insert("pii_masked".to_string(), Value::Bool(self.mask_pii));
        }
        Ok(Json(payload))
    }

    #[tool(
        description = "Get settled transactions. UTC ISO timestamps; start is exclusive and end is inclusive. Defaults to 30 days. Personally identifying fields are masked by default.",
        annotations(title = "Get settled Akahu transactions", read_only_hint = true)
    )]
    async fn get_transactions(
        &self,
        Parameters(request): Parameters<GetTransactionsRequest>,
    ) -> Result<Json<Value>, ErrorData> {
        let (start, end) = resolve_dates(request.start.as_deref(), request.end.as_deref())
            .map_err(as_tool_error)?;
        let params = vec![
            ("start".to_string(), start.clone()),
            ("end".to_string(), end.clone()),
        ];

        let account_scope = request.account_id.as_deref().unwrap_or("all").to_string();
        let result = match request.account_id.as_deref() {
            Some(account_id) => {
                self.client
                    .paged(
                        &["accounts", account_id, "transactions"],
                        &params,
                        request.limit,
                    )
                    .await
            }
            None => {
                self.client
                    .paged(&["transactions"], &params, request.limit)
                    .await
            }
        }
        .map_err(as_tool_error)?;
        let result = apply_pii_policy(result, self.mask_pii, false);

        let mut output = Map::new();
        output.insert("start_exclusive".to_string(), Value::String(start));
        output.insert("end_inclusive".to_string(), Value::String(end));
        output.insert("account_id".to_string(), Value::String(account_scope));
        output.insert("status".to_string(), Value::String("settled".to_string()));
        output.insert("pii_masked".to_string(), Value::Bool(self.mask_pii));
        if let Some(result) = result.as_object() {
            output.extend(result.clone());
        }
        Ok(Json(Value::Object(output)))
    }

    #[tool(
        description = "Get every page of pending transactions, clearly provisional and separate from settled totals. Personally identifying fields are masked by default.",
        annotations(title = "Get pending Akahu transactions", read_only_hint = true)
    )]
    async fn get_pending_transactions(
        &self,
        Parameters(request): Parameters<GetPendingTransactionsRequest>,
    ) -> Result<Json<Value>, ErrorData> {
        let account_scope = request.account_id.as_deref().unwrap_or("all").to_string();
        let result = match request.account_id.as_deref() {
            Some(account_id) => {
                self.client
                    .paged(
                        &["accounts", account_id, "transactions", "pending"],
                        &[],
                        request.limit,
                    )
                    .await
            }
            None => {
                self.client
                    .paged(&["transactions", "pending"], &[], request.limit)
                    .await
            }
        }
        .map_err(as_tool_error)?;
        let result = apply_pii_policy(result, self.mask_pii, false);

        let mut output = Map::new();
        output.insert("account_id".to_string(), Value::String(account_scope));
        output.insert(
            "status".to_string(),
            Value::String("pending_provisional".to_string()),
        );
        output.insert("pii_masked".to_string(), Value::Bool(self.mask_pii));
        if let Some(result) = result.as_object() {
            output.extend(result.clone());
        }
        Ok(Json(Value::Object(output)))
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for AkahuServer {
    fn get_info(&self) -> ServerInfo {
        let mut info = ServerInfo::new(ServerCapabilities::builder().enable_tools().build());
        info.server_info = Implementation::new("Akahu MCP", env!("CARGO_PKG_VERSION"));
        info.instructions = Some(
            "Read-only access to Akahu-connected accounts and transactions. Never perform payments, transfers, authorizations, connection changes, or account modifications. Treat all returned data as sensitive. Personally identifying fields are masked by default; operators can explicitly disable masking with AKAHU_MASK_PII=false."
                .to_string(),
        );
        info
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_client() -> AkahuClient {
        AkahuClient {
            http: reqwest::Client::new(),
            base_url: Url::parse(API_BASE).unwrap(),
            access_token: "token".into(),
            app_id: "app".into(),
        }
    }

    fn test_server(mask_pii: bool) -> AkahuServer {
        AkahuServer {
            client: test_client(),
            mask_pii,
            tool_router: AkahuServer::tool_router(),
        }
    }

    #[test]
    fn exposes_only_the_three_read_tools() {
        let server = test_server(true);
        let mut names: Vec<_> = server
            .tool_router
            .list_all()
            .into_iter()
            .map(|tool| tool.name.to_string())
            .collect();
        names.sort();
        assert_eq!(
            names,
            vec![
                "get_pending_transactions",
                "get_transactions",
                "list_accounts"
            ]
        );
        assert!(server.tool_router.list_all().into_iter().all(|tool| {
            tool.annotations
                .as_ref()
                .and_then(|annotations| annotations.read_only_hint)
                == Some(true)
        }));
    }

    #[test]
    fn endpoint_encodes_untrusted_path_segments() {
        let client = test_client();
        let url = client
            .endpoint(&["accounts", "acc/../me?x=1#frag", "transactions"])
            .unwrap();
        assert_eq!(
            url.as_str(),
            "https://api.akahu.io/v1/accounts/acc%2F..%2Fme%3Fx=1%23frag/transactions"
        );
    }

    #[test]
    fn cursor_supports_akahu_object_shape_and_string_fallback() {
        assert_eq!(
            next_cursor(&json!({"cursor": {"next": "abc"}})),
            Some("abc".to_string())
        );
        assert_eq!(
            next_cursor(&json!({"next_cursor": "xyz"})),
            Some("xyz".to_string())
        );
        assert_eq!(next_cursor(&json!({})), None);
    }

    #[test]
    fn supplied_dates_are_validated_and_preserved() {
        let (start, end) = resolve_dates(
            Some("2026-08-01T00:00:00Z"),
            Some("2026-09-01T00:00:00+00:00"),
        )
        .unwrap();
        assert_eq!(start, "2026-08-01T00:00:00Z");
        assert_eq!(end, "2026-09-01T00:00:00+00:00");
        assert!(resolve_dates(Some("not-a-date"), None).is_err());
    }

    #[test]
    fn default_range_is_approximately_thirty_days() {
        let before = Utc::now();
        let (start, end) = resolve_dates(None, None).unwrap();
        let start = DateTime::parse_from_rfc3339(&start)
            .unwrap()
            .with_timezone(&Utc);
        let end = DateTime::parse_from_rfc3339(&end)
            .unwrap()
            .with_timezone(&Utc);
        let after = Utc::now();
        assert!(end >= before - Duration::seconds(1));
        assert!(end <= after + Duration::seconds(1));
        let span = end - start;
        assert!(span >= Duration::days(30) - Duration::seconds(1));
        assert!(span <= Duration::days(30) + Duration::seconds(1));
    }

    #[test]
    fn result_limit_is_bounded() {
        assert_eq!(0_usize.clamp(1, MAX_RESULT_LIMIT), 1);
        assert_eq!(5000_usize.clamp(1, MAX_RESULT_LIMIT), MAX_RESULT_LIMIT);
    }

    #[test]
    fn pii_masking_defaults_to_enabled_and_can_be_disabled() {
        assert!(pii_masking_enabled(None));
        assert!(pii_masking_enabled(Some("true")));
        assert!(!pii_masking_enabled(Some("false")));
        assert!(!pii_masking_enabled(Some("0")));
        assert!(!pii_masking_enabled(Some("off")));
    }

    #[test]
    fn account_payload_masks_personal_fields_but_keeps_institution_name() {
        let payload = json!({
            "items": [{
                "name": "Example Person",
                "formatted_account": "12-3456-7890123-00",
                "_authorisation": "authorisation_example",
                "_credentials": "credentials_example",
                "connection": {"name": "Example Bank"}
            }]
        });
        let masked = apply_pii_policy(payload, true, true);
        assert_eq!(masked["items"][0]["name"], REDACTED);
        assert_eq!(masked["items"][0]["formatted_account"], REDACTED);
        assert_eq!(masked["items"][0]["_authorisation"], REDACTED);
        assert_eq!(masked["items"][0]["_credentials"], REDACTED);
        assert_eq!(masked["items"][0]["connection"]["name"], "Example Bank");
    }

    #[test]
    fn transaction_payload_masks_counterparty_pii_but_keeps_merchant_data() {
        let payload = json!({
            "items": [{
                "description": "CARD PURCHASE",
                "_user": "user_example",
                "hash": "transaction_hash",
                "meta": {"card_suffix": "1234"},
                "merchant": {"name": "Example Store"},
                "other_account": {
                    "name": "Example Person",
                    "account_number": "12-3456-7890123-00"
                }
            }]
        });
        let masked = apply_pii_policy(payload, true, false);
        assert_eq!(masked["items"][0]["other_account"]["name"], REDACTED);
        assert_eq!(
            masked["items"][0]["other_account"]["account_number"],
            REDACTED
        );
        assert_eq!(masked["items"][0]["_user"], REDACTED);
        assert_eq!(masked["items"][0]["hash"], REDACTED);
        assert_eq!(masked["items"][0]["meta"]["card_suffix"], REDACTED);
        assert_eq!(masked["items"][0]["merchant"]["name"], "Example Store");
        assert_eq!(masked["items"][0]["description"], "CARD PURCHASE");
    }

    #[test]
    fn disabled_pii_masking_returns_payload_unchanged() {
        let payload = json!({
            "items": [{"name": "Example Person", "formatted_account": "12-3456"}]
        });
        assert_eq!(apply_pii_policy(payload.clone(), false, true), payload);
    }
}
