//! Observed CLI snapshots only. Token counts, context occupancy and account
//! windows are different measurements; repeated snapshots are replacements.

use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;
const MAX_WINDOWS: usize = 32;

pub fn now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenUsage {
    pub scope: String,
    pub source: String,
    pub quality: String,
    pub captured_at: u64,
    pub unit: String,
    pub input: Option<u64>,
    pub output: Option<u64>,
    pub cached_input: Option<u64>,
    pub cache_creation: Option<u64>,
    pub reasoning: Option<u64>,
    pub total: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextUsage {
    pub source: String,
    pub quality: String,
    pub captured_at: u64,
    pub unit: String,
    pub used: u64,
    pub capacity: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RateWindow {
    pub id: String,
    pub source: String,
    pub captured_at: u64,
    pub quality: String,
    pub unit: String,
    pub used_percent: Option<f64>,
    pub remaining_percent: Option<f64>,
    pub duration_minutes: Option<u64>,
    pub resets_at: Option<u64>,
    pub state: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaSnapshot {
    pub availability: String,
    pub account_scope: Option<String>,
    pub source: String,
    pub captured_at: u64,
    pub windows: Vec<RateWindow>,
    pub message: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageSnapshot {
    pub provider: String,
    pub session_id: String,
    pub run_id: String,
    pub source_version: Option<String>,
    pub captured_at: u64,
    pub tokens: Option<TokenUsage>,
    pub last_turn_tokens: Option<TokenUsage>,
    pub context: Option<ContextUsage>,
    pub quota: QuotaSnapshot,
}

/// One collector belongs to one native session/run. It never sums provider
/// totals, assistant deltas, model breakdowns or subagent usage into a snapshot.
/// The frontend must replace snapshots, and display `scope` when only a turn is
/// available. The private account digest contains no raw identity/credentials.
pub struct Collector {
    snapshot: UsageSnapshot,
    emitted: Option<UsageSnapshot>,
    codex_legacy_bucket: Option<String>,
}

impl Collector {
    pub fn new(provider: &str, session_id: &str, run_id: &str) -> Self {
        let now = now_seconds();
        Self {
            snapshot: UsageSnapshot {
                provider: provider.to_string(),
                session_id: session_id.to_string(),
                run_id: run_id.to_string(),
                source_version: None,
                captured_at: now,
                tokens: None,
                last_turn_tokens: None,
                context: None,
                quota: unavailable_quota(provider, None, now),
            },
            emitted: None,
            codex_legacy_bucket: None,
        }
    }

    pub fn version(&mut self, value: Option<&str>) {
        // Provider/user-agent metadata is untrusted. Only a numeric version is
        // retained, never arbitrary text that could contain credentials.
        self.snapshot.source_version = value.filter(|value| value.len() <= 200).and_then(|value| {
            value
                .split(|c: char| !c.is_ascii_digit() && c != '.')
                .find(|part| {
                    let parts = part.split('.').collect::<Vec<_>>();
                    parts.len() == 3
                        && parts.iter().all(|part| {
                            !part.is_empty()
                                && part.len() <= 8
                                && part.chars().all(|c| c.is_ascii_digit())
                        })
                })
                .map(str::to_string)
        });
    }

    pub fn account(&mut self, response: &Value) {
        let account = response.get("account");
        let identity = account
            .and_then(|account| account.get("id").or_else(|| account.get("email")))
            .and_then(Value::as_str);
        self.snapshot.quota.account_scope = identity
            .filter(|id| !id.is_empty() && id.len() <= 512)
            .map(|id| account_digest(&self.snapshot.provider, id));
    }

    pub fn unavailable(&mut self) {
        self.snapshot.quota = unavailable_quota(
            &self.snapshot.provider,
            self.snapshot.quota.account_scope.clone(),
            now_seconds(),
        );
    }

    pub fn take_update(&mut self) -> Option<Value> {
        let now = now_seconds();
        // A reset elapsed without a new provider value is stale, never 0% used.
        for window in &mut self.snapshot.quota.windows {
            if window.resets_at.is_some_and(|reset| reset <= now)
                && window.state != "awaiting_update"
            {
                window.state = "awaiting_update".into();
                window.remaining_percent = None;
            }
        }
        if self.emitted.as_ref() == Some(&self.snapshot) {
            return None;
        }
        self.snapshot.captured_at = now;
        let value = serde_json::to_value(&self.snapshot).ok()?;
        self.emitted = Some(self.snapshot.clone());
        Some(value)
    }

    /// Full reads replace all buckets, including absent/null windows. Rolling
    /// notifications in Codex 0.159.2 are documented as sparse; their nulls do
    /// not clear previous metadata or windows. The next full read reconciles it.
    pub fn codex_rate_limits(&mut self, response: &Value, full: bool) {
        let now = now_seconds();
        if full {
            let identity = response
                .get("accountId")
                .and_then(Value::as_str)
                .filter(|id| !id.is_empty() && id.len() <= 512);
            let scope = identity
                .map(|id| account_digest("codex", id))
                .or_else(|| self.snapshot.quota.account_scope.clone());
            self.snapshot.quota = unavailable_quota("codex", scope, now);
            self.codex_legacy_bucket = response
                .pointer("/rateLimits/limitId")
                .and_then(Value::as_str)
                .map(metric_id);
            if self.codex_legacy_bucket.is_none() {
                self.codex_legacy_bucket = match response
                    .get("rateLimitsByLimitId")
                    .and_then(Value::as_object)
                {
                    Some(buckets) if buckets.len() == 1 => {
                        buckets.keys().next().map(|id| metric_id(id))
                    }
                    Some(_) => None,
                    None => Some("legacy".into()),
                };
            }
        }
        let source = if full {
            "account/rateLimits/read"
        } else {
            "account/rateLimits/updated"
        };
        // An explicitly empty modern map is authoritative and cannot fall back
        // to a legacy bucket. A null map supports older single-bucket servers.
        if let Some(buckets) = response
            .get("rateLimitsByLimitId")
            .and_then(Value::as_object)
        {
            for (id, bucket) in buckets.iter().take(MAX_WINDOWS / 2) {
                self.codex_bucket(&metric_id(id), bucket, source, full, now);
            }
        } else if let Some(bucket) = response.get("rateLimits").filter(|value| value.is_object()) {
            let id = bucket
                .get("limitId")
                .and_then(Value::as_str)
                .map(metric_id)
                .or_else(|| self.codex_legacy_bucket.clone());
            // A sparse notification without a limit ID cannot identify one of
            // several account buckets. Preserve the full snapshot until reread.
            if let Some(id) = id {
                self.codex_bucket(&id, bucket, source, full, now);
            }
        }
        self.snapshot.quota.source = source.into();
        self.snapshot.quota.captured_at = now;
        self.snapshot.quota.availability = if self.snapshot.quota.windows.is_empty() {
            "unavailable"
        } else {
            "observed"
        }
        .into();
        self.snapshot.quota.message = self
            .snapshot
            .quota
            .windows
            .is_empty()
            .then(|| "A CLI não reportou janelas de cota para esta conta.".into());
    }

    fn codex_bucket(&mut self, id: &str, bucket: &Value, source: &str, full: bool, now: u64) {
        for name in ["primary", "secondary"] {
            let key = format!("{id}:{name}");
            if let Some(value) = bucket.get(name).filter(|value| value.is_object()) {
                self.put_window(window(
                    &key,
                    source,
                    number(value.get("usedPercent")),
                    count(value.get("windowDurationMins")),
                    count(value.get("resetsAt")),
                    now,
                ));
            } else if full {
                self.snapshot
                    .quota
                    .windows
                    .retain(|window| window.id != key);
            }
        }
    }

    fn put_window(&mut self, value: RateWindow) {
        self.snapshot
            .quota
            .windows
            .retain(|window| window.id != value.id);
        if self.snapshot.quota.windows.len() < MAX_WINDOWS {
            self.snapshot.quota.windows.push(value);
            self.snapshot
                .quota
                .windows
                .sort_by(|left, right| left.id.cmp(&right.id));
        }
    }

    pub fn codex_tokens(&mut self, params: &Value, turn_id: &str) {
        if params.get("threadId").and_then(Value::as_str) != Some(&self.snapshot.session_id)
            || params.get("turnId").and_then(Value::as_str) != Some(turn_id)
        {
            return;
        }
        let usage = &params["tokenUsage"];
        let now = now_seconds();
        self.snapshot.tokens = codex_tokens(&usage["total"], "conversation", now);
        self.snapshot.last_turn_tokens = codex_tokens(&usage["last"], "model_call", now);
        self.snapshot.context = self.snapshot.last_turn_tokens.as_ref().and_then(|last| {
            Some(ContextUsage {
                source: "thread/tokenUsage/updated".into(),
                quality: "observed".into(),
                captured_at: now,
                unit: "tokens".into(),
                used: last.total?,
                capacity: count(usage.get("modelContextWindow"))
                    .filter(|capacity| *capacity > 0)?,
            })
        });
    }

    /// Claude's result is a cumulative provider snapshot. Its modelUsage and
    /// assistant usage are overlapping views and must never be added to it.
    pub fn claude_message(&mut self, message: &Value) {
        if message.get("session_id").and_then(Value::as_str) != Some(&self.snapshot.session_id)
            || message
                .get("parent_tool_use_id")
                .is_some_and(|id| !id.is_null())
        {
            return;
        }
        let now = now_seconds();
        match message.get("type").and_then(Value::as_str) {
            Some("result") => {
                let usage = &message["usage"];
                let input = count(usage.get("input_tokens"));
                let output = count(usage.get("output_tokens"));
                let cached_input = count(usage.get("cache_read_input_tokens"));
                let cache_creation = count(usage.get("cache_creation_input_tokens"));
                // These Anthropic categories are disjoint. Unknown categories
                // remain null, and absence is never treated as a zero count.
                let total = checked_sum(&[input, output, cached_input, cache_creation]);
                self.snapshot.tokens = (input.is_some() || output.is_some()).then(|| TokenUsage {
                    scope: "conversation".into(),
                    source: "stream-json/result.usage".into(),
                    quality: if total.is_some() {
                        "observed"
                    } else {
                        "partial"
                    }
                    .into(),
                    captured_at: now,
                    unit: "tokens".into(),
                    input,
                    output,
                    cached_input,
                    cache_creation,
                    reasoning: None,
                    total,
                });
            }
            Some("rate_limit_event") => {
                let info = &message["rate_limit_info"];
                let Some(kind) = info
                    .get("rateLimitType")
                    .and_then(Value::as_str)
                    .filter(|kind| {
                        matches!(
                            *kind,
                            "five_hour"
                                | "seven_day"
                                | "seven_day_opus"
                                | "seven_day_sonnet"
                                | "overage"
                        )
                    })
                else {
                    return;
                };
                let used = number(info.get("utilization"))
                    .filter(|value| (0.0..=1.0).contains(value))
                    .map(|value| value * 100.0);
                let duration = match kind {
                    "five_hour" => Some(300),
                    "seven_day" | "seven_day_opus" | "seven_day_sonnet" => Some(10_080),
                    _ => None,
                };
                self.put_window(window(
                    kind,
                    "stream-json/rate_limit_event",
                    used,
                    duration,
                    count(info.get("resetsAt")),
                    now,
                ));
                self.snapshot.quota.availability = "observed".into();
                self.snapshot.quota.source = "stream-json/rate_limit_event".into();
                self.snapshot.quota.captured_at = now;
                self.snapshot.quota.message = None;
            }
            _ => {}
        }
    }

    pub fn acp_update(&mut self, params: &Value) {
        if params.get("sessionId").and_then(Value::as_str) != Some(&self.snapshot.session_id)
            || params
                .pointer("/update/sessionUpdate")
                .and_then(Value::as_str)
                != Some("usage_update")
        {
            return;
        }
        // ACP used/size measures context, not paid-plan or rolling quota.
        let update = &params["update"];
        self.snapshot.context = count(update.get("used"))
            .zip(count(update.get("size")).filter(|capacity| *capacity > 0))
            .map(|(used, capacity)| ContextUsage {
                source: "ACP/session/update.usage_update".into(),
                quality: "observed".into(),
                captured_at: now_seconds(),
                unit: "tokens".into(),
                used,
                capacity,
            });
    }

    pub fn acp_result(&mut self, result: &Value) {
        let usage = &result["usage"];
        let input = count(usage.get("inputTokens"));
        let output = count(usage.get("outputTokens"));
        let total = count(usage.get("totalTokens")).or_else(|| checked_sum(&[input, output]));
        self.snapshot.last_turn_tokens = (input.is_some() || output.is_some() || total.is_some())
            .then(|| TokenUsage {
                scope: "turn".into(),
                source: "ACP/session/prompt.usage".into(),
                quality: if total.is_some() {
                    "observed"
                } else {
                    "partial"
                }
                .into(),
                captured_at: now_seconds(),
                unit: "tokens".into(),
                input,
                output,
                cached_input: count(usage.get("cachedReadTokens")),
                cache_creation: count(usage.get("cachedWriteTokens")),
                reasoning: count(usage.get("thoughtTokens")),
                total,
            });
        // Gemini _meta.quota.token_count repeats usage; it is not account quota.
    }
}

fn codex_tokens(value: &Value, scope: &str, now: u64) -> Option<TokenUsage> {
    let input = count(value.get("inputTokens"));
    let output = count(value.get("outputTokens"));
    let total = count(value.get("totalTokens"));
    (input.is_some() || output.is_some() || total.is_some()).then(|| TokenUsage {
        scope: scope.into(),
        source: "thread/tokenUsage/updated".into(),
        quality: if total.is_some() {
            "observed"
        } else {
            "partial"
        }
        .into(),
        captured_at: now,
        unit: "tokens".into(),
        input,
        output,
        cached_input: count(value.get("cachedInputTokens")),
        cache_creation: count(value.get("cacheWriteInputTokens")),
        reasoning: count(value.get("reasoningOutputTokens")),
        total,
    })
}

fn count(value: Option<&Value>) -> Option<u64> {
    value?.as_u64().filter(|count| *count <= MAX_SAFE_INTEGER)
}

fn number(value: Option<&Value>) -> Option<f64> {
    value?
        .as_f64()
        .filter(|number| number.is_finite() && *number >= 0.0)
}

fn checked_sum(values: &[Option<u64>]) -> Option<u64> {
    values
        .iter()
        .try_fold(0u64, |sum, value| sum.checked_add((*value)?))
        .filter(|sum| *sum <= MAX_SAFE_INTEGER)
}

fn account_digest(provider: &str, identity: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(provider.as_bytes());
    digest.update([0]);
    digest.update(identity.as_bytes());
    format!("acct:{:x}", digest.finalize())
}

fn metric_id(id: &str) -> String {
    if !id.is_empty()
        && id.len() <= 128
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    {
        id.to_string()
    } else {
        format!("bucket:{:x}", Sha256::digest(id.as_bytes()))
    }
}

fn window(
    id: &str,
    source: &str,
    used_percent: Option<f64>,
    duration_minutes: Option<u64>,
    resets_at: Option<u64>,
    now: u64,
) -> RateWindow {
    let stale = resets_at.is_some_and(|reset| reset <= now);
    RateWindow {
        id: id.into(),
        source: source.into(),
        captured_at: now,
        quality: if used_percent.is_some() {
            "observed"
        } else {
            "partial"
        }
        .into(),
        unit: "percent".into(),
        used_percent,
        remaining_percent: if stale {
            None
        } else {
            used_percent.map(|used| (100.0 - used).clamp(0.0, 100.0))
        },
        duration_minutes,
        resets_at,
        state: if stale {
            "awaiting_update"
        } else if used_percent.is_some() {
            "available"
        } else {
            "unavailable"
        }
        .into(),
    }
}

fn unavailable_quota(provider: &str, account_scope: Option<String>, now: u64) -> QuotaSnapshot {
    QuotaSnapshot {
        availability: "unavailable".into(), account_scope, source: if provider == "codex" { "account/rateLimits/read" } else { "CLI/passive" }.into(), captured_at: now, windows: Vec::new(),
        message: Some("Cota da conta indisponível neste canal da CLI. Tokens e contexto não representam o saldo do plano.".into()),
    }
}

/// Safe activity payloads contain native identifiers/status only; no prompts,
/// tool input/output, account identity or environment variables are forwarded.
pub fn activity(
    provider: &str,
    session_id: &str,
    run_id: &str,
    kind: &str,
    id: &str,
    status: &str,
    source: &str,
) -> Option<Value> {
    if !matches!(kind, "tool" | "subagent")
        || id.is_empty()
        || id.len() > 256
        || id.chars().any(char::is_control)
    {
        return None;
    }
    let status = match status {
        "running" | "in_progress" => "running",
        "pending" | "pendingInit" => "pending",
        "completed" | "success" | "shutdown" | "closed" => "completed",
        "failed" | "error" | "errored" => "failed",
        _ => "unknown",
    };
    Some(
        serde_json::json!({ "provider": provider, "sessionId": session_id, "runId": run_id, "kind": kind, "id": id, "status": status, "source": source, "capturedAt": now_seconds(), "quality": "observed" }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn codex_full_snapshot_removes_buckets_and_does_not_assume_window_lengths() {
        let mut collector = Collector::new("codex", "s", "r");
        collector.codex_rate_limits(&json!({"accountId":"private-account", "rateLimits":{}, "rateLimitsByLimitId":{"codex":{"primary":{"usedPercent":40,"windowDurationMins":120,"resetsAt":MAX_SAFE_INTEGER},"secondary":{"usedPercent":10,"windowDurationMins":500}}}}), true);
        let first = collector.take_update().unwrap();
        assert_eq!(first["quota"]["windows"][0]["durationMinutes"], 120);
        assert_eq!(first["quota"]["windows"][0]["remainingPercent"], 60.0);
        assert!(!first.to_string().contains("private-account"));
        collector.codex_rate_limits(&json!({"rateLimits":{},"rateLimitsByLimitId":{}}), true);
        assert!(collector.take_update().unwrap()["quota"]["windows"]
            .as_array()
            .unwrap()
            .is_empty());
        collector.codex_rate_limits(
            &json!({"rateLimits":{"primary":null,"secondary":null}}),
            true,
        );
        assert!(collector.snapshot.quota.windows.is_empty());
        if let Some(snapshot) = collector.take_update() {
            assert!(snapshot["quota"]["windows"].as_array().unwrap().is_empty());
        }
    }

    #[test]
    fn codex_sparse_null_retains_window_until_full_snapshot_and_reset_is_stale() {
        let mut collector = Collector::new("codex", "s", "r");
        collector.codex_rate_limits(
            &json!({"rateLimits":{"limitId":"codex","primary":{"usedPercent":25,"resetsAt":1}}}),
            true,
        );
        collector.codex_rate_limits(
            &json!({"rateLimits":{"limitId":"codex","primary":null,"secondary":null}}),
            false,
        );
        let snapshot = collector.take_update().unwrap();
        assert_eq!(snapshot["quota"]["windows"][0]["state"], "awaiting_update");
        assert!(snapshot["quota"]["windows"][0]["remainingPercent"].is_null());
        assert_eq!(snapshot["quota"]["windows"][0]["usedPercent"], 25.0);
    }

    #[test]
    fn repeated_cumulative_tokens_never_add_cache_reasoning_or_child_snapshots() {
        let mut collector = Collector::new("codex", "s", "r");
        let event = json!({"threadId":"s","turnId":"t","tokenUsage":{"total":{"inputTokens":100,"outputTokens":30,"cachedInputTokens":80,"reasoningOutputTokens":20,"totalTokens":130},"last":{"totalTokens":55},"modelContextWindow":200000}});
        collector.codex_tokens(&event, "t");
        collector.codex_tokens(&event, "t");
        let snapshot = collector.take_update().unwrap();
        assert_eq!(snapshot["tokens"]["total"], 130);
        assert_eq!(snapshot["context"]["used"], 55);
        collector.codex_tokens(
            &json!({"threadId":"child","turnId":"t","tokenUsage":{"total":{"totalTokens":1000}}}),
            "t",
        );
        assert!(collector.take_update().is_none());
    }

    #[test]
    fn claude_result_is_replaced_and_overlapping_views_are_not_summed() {
        let mut collector = Collector::new("claude", "s", "r");
        let event = json!({"type":"result","session_id":"s","usage":{"input_tokens":10,"output_tokens":20,"cache_read_input_tokens":30,"cache_creation_input_tokens":40},"modelUsage":{"model":{"inputTokens":9999}},"total_cost_usd":0.123});
        collector.claude_message(&event);
        collector.claude_message(&event);
        assert_eq!(collector.take_update().unwrap()["tokens"]["total"], 100);
        collector.claude_message(
            &json!({"type":"assistant","session_id":"s","message":{"usage":{"input_tokens":9000}}}),
        );
        assert!(collector.take_update().is_none());
        collector.claude_message(&json!({"type":"result","session_id":"s","usage":null}));
        assert!(collector.take_update().unwrap()["tokens"].is_null());
    }

    #[test]
    fn acp_context_and_turn_usage_are_not_subscription_quota() {
        let mut collector = Collector::new("gemini", "s", "r");
        collector.acp_update(&json!({"sessionId":"s","update":{"sessionUpdate":"usage_update","used":123,"size":1000}}));
        collector.acp_result(&json!({"usage":{"inputTokens":100,"outputTokens":23,"cachedReadTokens":80,"thoughtTokens":12,"totalTokens":123},"_meta":{"quota":{"token_count":{"input_tokens":100,"output_tokens":23}}}}));
        let snapshot = collector.take_update().unwrap();
        assert_eq!(snapshot["context"]["used"], 123);
        assert_eq!(snapshot["lastTurnTokens"]["total"], 123);
        assert_eq!(snapshot["quota"]["availability"], "unavailable");
        assert!(snapshot["tokens"].is_null());
    }

    #[test]
    fn malformed_numbers_and_unknown_activity_are_never_reported_as_zero() {
        let mut collector = Collector::new("copilot", "s", "r");
        collector.acp_result(&json!({"usage":{"inputTokens":-1,"outputTokens":"123","totalTokens":MAX_SAFE_INTEGER+1}}));
        assert!(collector.take_update().unwrap()["lastTurnTokens"].is_null());
        assert!(activity("codex", "s", "r", "subagent", "", "running", "fixture").is_none());
        assert_eq!(
            activity("codex", "s", "r", "subagent", "child", "notFound", "fixture").unwrap()
                ["status"],
            "unknown"
        );
    }

    #[test]
    fn resume_replaces_cumulative_value_and_account_scopes_do_not_share_windows() {
        let mut first = Collector::new("codex", "same-session", "r1");
        first.codex_rate_limits(
            &json!({"accountId":"a", "rateLimits":{"primary":{"usedPercent":90}}}),
            true,
        );
        first.codex_tokens(&json!({"threadId":"same-session","turnId":"t1","tokenUsage":{"total":{"totalTokens":400},"last":{"totalTokens":100}}}), "t1");
        let initial = first.take_update().unwrap();
        let mut resumed = Collector::new("codex", "same-session", "r2");
        resumed.codex_rate_limits(
            &json!({"accountId":"b", "rateLimits":{"primary":{"usedPercent":10}}}),
            true,
        );
        resumed.codex_tokens(&json!({"threadId":"same-session","turnId":"t2","tokenUsage":{"total":{"totalTokens":500},"last":{"totalTokens":100}}}), "t2");
        let snapshot = resumed.take_update().unwrap();
        assert_eq!(snapshot["tokens"]["total"], 500);
        assert_eq!(snapshot["lastTurnTokens"]["scope"], "model_call");
        assert_ne!(
            snapshot["quota"]["accountScope"],
            initial["quota"]["accountScope"]
        );
        assert_eq!(snapshot["quota"]["windows"][0]["usedPercent"], 10.0);
    }

    #[test]
    fn ambiguous_sparse_bucket_has_no_invented_identity_or_duplicate_window() {
        let mut collector = Collector::new("codex", "s", "r");
        collector.codex_rate_limits(&json!({"rateLimits":{}, "rateLimitsByLimitId":{"a":{"primary":{"usedPercent":10}},"b":{"primary":{"usedPercent":20}}}}), true);
        collector.codex_rate_limits(
            &json!({"rateLimits":{"limitId":null,"primary":{"usedPercent":80}}}),
            false,
        );
        assert_eq!(collector.snapshot.quota.windows.len(), 2);
        assert!(!collector
            .snapshot
            .quota
            .windows
            .iter()
            .any(|window| window.used_percent == Some(80.0)));
        collector.codex_rate_limits(
            &json!({"rateLimits":{},"rateLimitsByLimitId":{"a":{"primary":{"usedPercent":10}}}}),
            true,
        );
        collector.codex_rate_limits(
            &json!({"rateLimits":{"limitId":null,"primary":{"usedPercent":80}}}),
            false,
        );
        assert_eq!(collector.snapshot.quota.windows.len(), 1);
        assert_eq!(collector.snapshot.quota.windows[0].used_percent, Some(80.0));
    }

    #[test]
    fn claude_rate_limit_fields_are_optional_fraction_and_session_scoped() {
        let mut collector = Collector::new("claude", "s", "r");
        collector.claude_message(&json!({"type":"rate_limit_event","session_id":"other","rate_limit_info":{"rateLimitType":"five_hour","utilization":0.6,"resetsAt":MAX_SAFE_INTEGER}}));
        assert!(collector.snapshot.quota.windows.is_empty());
        collector.claude_message(&json!({"type":"rate_limit_event","session_id":"s","rate_limit_info":{"rateLimitType":"five_hour","utilization":0.6,"resetsAt":MAX_SAFE_INTEGER}}));
        assert_eq!(
            collector.snapshot.quota.windows[0].remaining_percent,
            Some(40.0)
        );
        collector.claude_message(&json!({"type":"rate_limit_event","session_id":"s","rate_limit_info":{"rateLimitType":"five_hour","utilization":null,"resetsAt":null}}));
        assert_eq!(collector.snapshot.quota.windows.len(), 1);
        assert!(collector.snapshot.quota.windows[0].used_percent.is_none());
        assert_eq!(collector.snapshot.quota.windows[0].state, "unavailable");
    }

    #[test]
    fn version_metadata_keeps_only_numeric_version_not_secrets() {
        let mut collector = Collector::new("codex", "s", "r");
        collector.version(Some("codex/0.159.2 windows bearer secret"));
        assert_eq!(
            collector.snapshot.source_version.as_deref(),
            Some("0.159.2")
        );
        collector.version(Some("sk-ant-secret"));
        assert!(collector.snapshot.source_version.is_none());
    }
}
