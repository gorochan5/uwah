# Webhook Delivery

Webhook delivery is optional. Configure it in `uwah.toml`:

```toml
[webhook]
enabled = true
url = "https://hooks.example.com/uwah/events"
secret = "use-a-dedicated-random-signing-secret"
```

When enabled, eligible gateway events are persisted in `outbox_events` and sent asynchronously by a background worker. Successful HTTP 2xx responses remove the outbox row. Failed events are retried or moved to `webhook_dlq`.

## Request format and signatures

The worker sends an HTTP `POST` with `Content-Type: application/json`. When `[webhook].secret` is configured, the request includes `X-Uwah-Signature` in this format:

```text
t=<unix_timestamp_milliseconds>,v1=<hex_hmac>
```

The HMAC is SHA-256 over the UTF-8 bytes of `<timestamp>.<raw-request-body>`, using the configured secret. Verify the signature against the raw body before parsing JSON, compare digests in constant time, and reject malformed signatures. Although the timestamp is signed, Uwah's verifier does not enforce an age window; the receiver must implement its own replay policy if needed.

If no secret is configured, requests are sent without a signature header. Use HTTPS for the webhook URL when the receiver is not on a trusted local network.

## Retry behavior

Delivery is **at-least-once**. A receiver may process a request even if Uwah does not receive its response, so duplicate delivery is possible. Deduplicate using the event `id` in the JSON payload.

- Any successful 2xx response marks delivery complete.
- HTTP 4xx responses, except `429`, are treated as permanent failures and moved to the DLQ.
- HTTP `429`, other non-success responses, and transport failures are retried.
- Retry scheduling uses exponential backoff with deterministic jitter, capped at 15 minutes.
- An event is moved to the DLQ after a permanent failure or when the next retry reaches the configured limit of ten attempts.

The outbox is a delivery queue, not a durable event history. Successfully delivered rows are deleted.

## Dead-letter queue operations

All DLQ HTTP routes require the configured Bearer token. The gRPC equivalents are provided by `uwah.v1.WebhookDlqService`.

| Method | HTTP path | Action |
| --- | --- | --- |
| `GET` | `/webhooks/dlq` | List quarantined events. |
| `GET` | `/webhooks/dlq/{id}` | Retrieve one event and failure details. |
| `POST` | `/webhooks/dlq/{id}/replay` | Requeue one event. |
| `POST` | `/webhooks/dlq/replay-all` | Requeue all DLQ events. |
| `DELETE` | `/webhooks/dlq/{id}` | Permanently delete one DLQ entry. |

Replaying an event can produce another delivery attempt and may result in a duplicate at the receiver. Confirm the receiver's idempotency behavior before replaying large batches.

## Operations

Keep the endpoint reachable from the Uwah host, protect the signing secret, and monitor application logs and the DLQ. The current metrics expose aggregate webhook dispatch success and failure counters; they do not expose outbox depth or event age.
